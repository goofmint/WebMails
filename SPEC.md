# Eluma — Technical Specification

> **Status:** Draft v0.3 · **Target platforms:** macOS 14+, Windows 10/11 · **Framework:** Tauri v2 (Rust)

---

## 1. Summary

Eluma is an open-source desktop shell that hosts multiple **webmail services** — Gmail, iCloud Mail, Outlook.com, Fastmail, Zoho, Roundcube, or anything with a web UI — in a single window with a Slack-style service sidebar.

Three decisions define it:

1. **No bundled browser engine.** Tauri v2 over the OS webview (WKWebView / WebView2), not Electron.
2. **Unread state is read in JavaScript, inside the service's own webview**, by per-service recipes built into the app and implemented against a single interface. All service webviews stay resident.
3. **Notifications and badges are owned by Rust.** The webview reports counts; Rust decides what is new, what to notify, and what to render. Notification delivery never depends on the web page's own `Notification` API.

Point 2 means background throttling is a first-class correctness problem rather than an afterthought — §9 exists for that reason and is the highest-risk area of the project.

### 1.1 Relationship to Ferdium

Eluma's unread mechanism is structurally similar to Ferdium's: a live DOM, observed from JavaScript. The differences that matter:

| | Ferdium | Eluma |
|---|---|---|
| Runtime | Electron (bundled Chromium) | Tauri (OS webview) |
| Per-service process | Full Electron renderer | WKWebView / WebView2 content process |
| Background throttling | Not addressed; badges silently go stale | Explicitly disabled where the platform allows; staleness surfaced where it does not |
| Notifications | Page-originated | Rust-originated, uniform across services |
| Session isolation | Always on, per service | Per-service choice (shared or isolated) |
| Hibernation | Present, defaults defeat it | Not offered; all services resident by design |

**Honest expectation setting:** keeping every service resident means N live webmail SPAs, and Gmail is heavy wherever it runs. Eluma's savings over Ferdium come from dropping the bundled Chromium, from WebKit/WebView2 sharing more system-level resources than N independent Electron partitions, and from a far smaller shell. They do not come from running fewer web pages. If memory becomes the dominant complaint, §17 Q4 records the escape hatch.

---

## 2. Goals and non-goals

### 2.1 Goals

| # | Goal |
|---|------|
| G1 | Host arbitrary webmail services; adding one requires only a URL |
| G2 | Accurate unread badges on sidebar icons, kept live regardless of which service is focused or whether the window is visible. No Dock / taskbar badge |
| G3 | Native OS notifications for new mail on **every** configured service, including iCloud |
| G4 | No bundled browser engine |
| G5 | Per-service session: shared or isolated, user's choice |
| G6 | Per-service icons: favicon by default, user-overridable |
| G7 | Service support added by writing one recipe module against a fixed interface; no config-file recipes |

### 2.2 Non-goals

- Eluma is **not a mail client.** It renders no messages, stores no mail, implements no composer, speaks no IMAP or SMTP. The web UI is the UI.
- No unified inbox, no local search, no offline archive, no encryption features.
- No mobile targets.
- **Linux is out of scope.** WebKitGTK supports neither `background_throttling` nor `dataStoreIdentifier`, so both load-bearing mechanisms would need separate designs.
- Tauri now ships an optional CEF runtime (`tauri-runtime-cef`). It is **rejected**: bundling CEF reintroduces exactly the Chromium payload G4 exists to avoid.

---

## 3. Architecture

```
┌───────────────────────────────────────────────────────────────┐
│ Tauri Shell — always on screen, never throttled               │
│  ┌────────────┐  ┌─────────────────────────────────────────┐  │
│  │  Sidebar   │  │  Webview host                           │  │
│  │            │  │  ┌───────────────────────────────────┐  │  │
│  │ ● Personal3│  │  │  active service webview (visible) │  │  │
│  │ ● Work  12 │  │  └───────────────────────────────────┘  │  │
│  │ ● iCloud   │  │    other service webviews (resident,    │  │
│  │ ● Outlook 1│  │    offscreen, throttling disabled)      │  │
│  └─────▲──────┘  └──────────────────┬──────────────────────┘  │
└────────┼────────────────────────────┼─────────────────────────┘
         │ badge state                │ IPC: UnreadReport
┌────────┴────────────────────────────┴─────────────────────────┐
│ Rust core                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────────┐  │
│  │ Unread state │─▶│ Diff engine  │─▶│ Notification        │  │
│  │ store        │  │ (what's new) │  │ dispatcher          │  │
│  └──────────────┘  └──────────────┘  └─────────────────────┘  │
│  ┌──────────────┐  ┌──────────────┐  ┌─────────────────────┐  │
│  │ Webview      │  │ Service      │  │ Liveness monitor    │  │
│  │ lifecycle    │  │ registry     │  │ (staleness watchdog)│  │
│  └──────────────┘  └──────────────┘  └─────────────────────┘  │
└───────────────────────────────────────────────────────────────┘
```

**Key invariant:** the sidebar belongs to the Tauri shell's own frontend, which is always on screen and therefore never throttled. Service webviews push `UnreadReport`s inward; they never render the badge themselves.

**Trust boundary:** a service webview is loading third-party code. It is not trusted. The injected agent posts structured messages over a narrow IPC surface (§7.4); Rust validates everything and ignores reports from services it did not ask about.

---

## 4. Core concepts

**Service** — one sidebar entry: name, URL, icon, profile, unread strategy, notification settings.

**Profile** — a webview data partition (cookies, localStorage, IndexedDB). Services naming the same profile share a session; services with different profiles cannot see each other's cookies. See §5.

**Strategy** — a reusable technique for obtaining an unread count (`title`, `fetch`, `selector`). Implemented once in the agent as helpers that recipes call. See §7.

**Recipe** — a per-service TypeScript module, compiled into the agent, implementing one `Recipe` interface: which service URLs it matches, its default profile, and how it reads the unread count (usually by calling a strategy helper). Recipes live in the repository and are added by PR. There is no recipe file format and no loading from disk. The recipe for a service is chosen automatically from the service URL; the user never selects or configures it.

---

## 5. Sessions and profiles

Each service names a profile. Services sharing a profile name share one cookie jar.

```toml
profile = "default"    # shared with every other service naming "default"
profile = "isolated"   # sugar: a private profile derived from the service id
profile = "work"       # a named profile, shareable between chosen services
```

### 5.1 Why this shape

Two distinct needs collapse into one mechanism:

- **Gmail multi-account needs sharing.** Google's multi-login keeps every signed-in account in one cookie jar addressed by the `/u/{n}/` path segment. Three Gmail services on `profile = "default"` are three views of one session — which is correct and also cheaper than three isolated logins.
- **Everything else needs isolation.** Two iCloud accounts, or two Outlook accounts, cannot coexist in one cookie jar. They need separate profiles.

Defaulting Gmail services to `"default"` and everything else to `"isolated"` (§6.2) gets both right with no user thought required.

### 5.2 Platform mapping

| Platform | Mechanism | Availability |
|---|---|---|
| macOS 14+ | `WebviewWindowBuilder::data_store_identifier` → `WKWebsiteDataStore(forIdentifier:)` | Tauri v2.2+ |
| Windows | `WebviewWindowBuilder::data_directory` → WebView2 user data folder | Tauri v2 |

Note the asymmetry: `data_directory` is **unsupported on macOS** (WKWebView has no equivalent), and `data_store_identifier` is **unsupported on Windows**. A `ProfileBackend` abstraction picks per platform. Profile names map to stable UUIDs (UUIDv5 over the profile name) persisted in `state.json` (§13).

Data lands in `~/Library/WebKit/WebsiteDataStore/<UUID>/` on macOS and the configured user data folder on Windows.

> ℹ️ **Resolved upstream:** tauri#12843 (crash with `data_store_identifier`) was closed on 2025-03-13 by wry#1512. One reported cause was an invalid identifier such as `[0u8; 16]`; identifiers must be well-formed UUIDs. Still confirm in the M1 spike that two isolated stores run side by side without crashing.

> ⚠️ **Windows constraint:** WebView2 requires that *webviews with different browser arguments also use different data directories*. Since §9.2 applies throttling arguments, those arguments **must be identical across every webview** or profile sharing silently breaks.

---

## 6. Configuration

Location: `{config_dir}/config.toml`. The app writes back to it (reordering, icon changes), so use `toml_edit` to preserve user comments and formatting on round-trip.

```toml
version = 1

[settings]
reconcile_interval_seconds = 60   # sweep; observers are the primary signal
notifications = true
notification_batch_threshold = 5
badge_sidebar = true

# ─── Gmail ×2, sharing one Google session ───
[[services]]
id = "gmail-personal"
name = "Personal"
url = "https://mail.google.com/mail/u/me@example.com/"
profile = "default"
notifications = true
icon = { source = "favicon" }

[[services]]
id = "gmail-work"
name = "Work"
url = "https://mail.google.com/mail/u/me@company.com/"
profile = "default"
notifications = true
icon = { source = "favicon" }

# ─── iCloud, its own session ───
[[services]]
id = "icloud"
name = "iCloud"
url = "https://www.icloud.com/mail/"
profile = "isolated"
notifications = true
icon = { source = "file", value = "icons/icloud.png" }

# ─── Outlook.com, its own session ───
[[services]]
id = "outlook"
name = "Outlook"
url = "https://outlook.live.com/mail/0/"
profile = "isolated"
notifications = true
icon = { source = "favicon" }
```

### 6.1 No credential storage

Eluma stores **no credentials of any kind.** No passwords, no app-specific passwords, no OAuth tokens, no keychain entries. Every service authenticates the way it always has — in its own web UI, with its cookies in its own profile. `config.toml` is safe to paste into a GitHub issue.

*(This section replaces the keychain/PKCE design of draft v0.1, which existed only to serve IMAP providers. With IMAP removed, it has no purpose.)*

### 6.2 Defaults on service creation

When the user adds a URL, Eluma matches it against the built-in recipes and fills in the profile from the matching recipe's default. Unmatched URLs get the generic title recipe and `profile = "isolated"` — the safest generic pair.

The recipe itself is **not written to `config.toml`**. It is re-derived from the service URL every time the service starts, so a recipe improvement in a new release applies to existing services with no migration.

---

## 7. Unread detection

All acquisition happens **in JavaScript, inside the service's webview.** Rust never makes authenticated HTTP requests, which means no cookie bridge — and with it, none of `Webview::cookies_for_url()`'s Windows problems (deadlock in synchronous contexts, tauri#13300's freeze under periodic background polling). The service's own origin already has its cookies; `fetch` inside the page is simply the right tool.

### 7.1 Strategies

Three strategies, implemented once in the agent as helpers. Recipes call them (§7.5).

| Strategy | Mechanism | Fragility | Notification detail |
|---|---|---|---|
| `title` | `MutationObserver` on `<title>`, regex-extract the count | Low | count only |
| `fetch` | same-origin `fetch()` of a lightweight endpoint, parse the response | Low | sender + subject |
| `selector` | `MutationObserver` on a CSS-selected node, read text or count rows | High | count, optionally sender + subject |

**`title` is the default and the recommended generic strategy.** Nearly every webmail writes the unread count into the document title (`Inbox (3)`), and page titles change far less often than DOM structure. It is the single most durable generic signal available, which is why G1 rests on it rather than on `selector`.

**`fetch` is preferred where a cheap endpoint exists.** For Gmail, `GET /mail/u/{n}/feed/atom` returns `<fullcount>` plus one entry per unread message with sender, subject, and a permalink — everything needed for a rich notification, in one request, authenticated by the ambient session.

> **Status of the Gmail feed — read carefully.** Google's published documentation covers `https://mail.google.com/mail/feed/atom`, states it is *"only available for Gmail accounts on Google Workspace domains"*, and names OAuth 2.0 as the preferred authentication method. The `/u/{n}/feed/atom` form, and its use with consumer `@gmail.com` accounts via session cookies, is **undocumented community knowledge**. It works empirically and has for two decades, but it is not a supported API and carries no compatibility promise. The Gmail recipe must therefore validate on first use and fall back to `title` on any non-200, malformed body, or login redirect. Treat a `fetch` recipe breaking as expected maintenance, not as an incident.

**`selector` is the last resort.** Fragile in exactly the way Ferdium recipes are fragile. Offered because some services expose the count nowhere else.

### 7.2 Observation, not polling

`MutationObserver` on `<title>` or a selected node makes unread changes **push-based and effectively instantaneous** — no polling latency, no wasted cycles. `reconcile_interval_seconds` (default 60s) is a reconciliation sweep to catch missed mutations and to re-run `fetch`-based recipes, not the primary signal.

A `fetch` recipe polls at the reconcile interval with jitter. 60s is a starting value, not a validated one (§17 Q2).

### 7.3 Agent injection

A small JS agent is injected into every service webview at document start:

- Receives its service id and service URL at injection time, and selects its recipe by matching the service URL (not the current page URL, which changes during login).
- Runs only in the top-level frame. (On Windows the initialization script is also injected into subframes; the agent exits immediately there.)
- Installs the recipe's observer.
- Posts `UnreadReport` on change and on reconcile.
- Re-posts the latest `UnreadReport` every 30s. This periodic report **is** the heartbeat; there is no separate heartbeat message (§9.4).
- Replaces the page's `Notification` API with a stub that reports `denied` (§10).
- Collects icon candidates from the live document (§11.1) and attaches them to the first report after each page load.
- Is **read-only with respect to page state.** It never clicks, submits, or mutates the DOM.

### 7.4 IPC contract

```rust
struct UnreadReport {
    service_id: String,
    count: Option<u32>,          // None = signed out, or service not reachable
    messages: Vec<MessageRef>,   // may be empty (title/selector strategies); always empty when count is None
    recipe_id: String,
    observed_at: SystemTime,
    icon_candidates: Vec<String>, // absolute URLs, best first (§11.1 steps 2–4); sent once per page load, otherwise empty
}

struct MessageRef {
    id: String,                  // stable per service; hash of link or message id
    from: Option<String>,
    subject: Option<String>,
    link: Option<String>,
}
```

`count` carries both the number and the health of the service:

- `Some(n)` — the service is signed in and readable; `n` unread.
- `None` — the agent is running but cannot produce a count: the page is on a login screen, the service failed to load, or the recipe could not read it. Rendered as `NeedsAttention` (§11.3). Never treated as zero, and never triggers or clears notifications.

When the webview navigates away from the service origin (typically to a third-party login page such as `accounts.google.com`), the agent there has no permission to report (see Transport below). Rust detects this itself from the webview's page-load events and records the same `None` state, so a signed-out service shows `NeedsAttention`, not `Stale`.

Rust rejects reports whose `service_id` does not match the webview they arrived on.

**Transport.** The report is sent with Tauri's `invoke` to a single command. Because service pages are remote origins, Tauri (2.11.1+) only lets them reach app commands through an explicit `remote` capability. Eluma adds one capability per service at runtime (`Manager::add_capability`), scoped to that service's webview label and origin and granting only this command. The page's own scripts can call it too, which is why the command accepts nothing but this report and validates every field.

### 7.5 Recipe interface

Every supported service is one TypeScript module implementing a single `Recipe` interface: its id and display name, a URL matcher, a default profile, a human-readable description of what it reads (§14), and the read logic. A generic title-based recipe matches every URL and is used when no specific recipe does. Adding a service means adding one module and registering it; Rust is not touched.

---

## 8. Webview lifecycle

### 8.1 Hosting model

Tauri v2 supports multiple webviews per window via `Window::add_child`, but the API remains behind the `unstable` Cargo feature in 2.11.x, with open positioning and resizing bugs (tauri#10420, tauri#11376).

**Decision:** build against the unstable API behind an internal `WebviewHost` trait, with a second implementation using one child `WebviewWindow` per service, geometry-synced to the parent content rect. If the unstable API proves unshippable, switching is a flag, not a rewrite.

### 8.2 Residency

**All configured services are instantiated at launch and stay resident for the process lifetime.** Inactive services are positioned offscreen, not hidden — `isHidden`/`IsVisible = false` is a stronger suspension signal to both engines than being out of view.

There is no hibernation, no LRU eviction, and no unload. Unread accuracy depends on a live document (§7), so evicting a webview means losing the signal it exists to produce.

Startup is staggered: services are instantiated in sidebar order with a short delay between each, so a cold start does not load N webmail SPAs simultaneously.

---

## 9. Background survival

This is the highest-risk area of the project. Because unread state lives in the page, a throttled or suspended webview is a **silently wrong badge** — the failure mode that makes an app like this untrustworthy.

### 9.1 macOS

- `background_throttling(BackgroundThrottlingPolicy::Disabled)` → `WKPreferences.inactiveSchedulingPolicy`. Requires **macOS 14.0+**; the platform default is `.suspend`. Apple scopes this policy to a web view *not in a window*, and exempts views playing media or doing media capture.
- Hold an `NSProcessInfo.beginActivity` assertion with `NSActivityUserInitiated` while any service is being observed, to keep App Nap from suspending the process when Eluma is not frontmost.
- Keep webviews in the view hierarchy at non-zero size, positioned offscreen (§8.2).

macOS 13 and earlier have no equivalent lever, which is the substantive reason for the 14+ floor.

### 9.2 Windows

wry's `background_throttling` is **unsupported on Windows**. The intended mitigation is `additionalBrowserArgs`:

```
--disable-background-timer-throttling
--disable-renderer-backgrounding
--disable-backgrounding-occluded-windows
```

> ⚠️ **Unverified.** These are Chromium switches; Microsoft does not document them for WebView2, and no authoritative confirmation was found that they suppress WebView2 background throttling. **This is an assumption, and it is the entire Windows mitigation.** Validate empirically before committing to the Windows target (§17 Q1).

> ⚠️ `additionalBrowserArgs` **replaces** wry's defaults rather than appending. wry normally passes `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`; setting custom args silently drops those and they must be re-specified.

> ⚠️ Arguments must be **identical across all webviews**, or WebView2 profile sharing breaks (§5.2).

### 9.3 Platform summary

| | macOS 14+ | Windows |
|---|---|---|
| Throttling control | `background_throttling` ✅ | `additionalBrowserArgs` ⚠️ unverified |
| Process-level | App Nap assertion ✅ | n/a |
| Session isolation | `dataStoreIdentifier` ✅ | `data_directory` ✅ |
| Residual risk | WebContent suspension under memory pressure | unknown until §17 Q1 resolves |

### 9.4 Liveness monitor — the safety net

None of the above is guaranteed, so Eluma assumes it will sometimes fail.

- Every agent re-posts its latest report every 30s; that report is the heartbeat.
- Two consecutive missed reports mark the service `Stale`.
- `Stale` (no reports) and `NeedsAttention` (reports arriving with `count = None`) are different states: the first means the webview is not running the agent; the second means the agent is running and the service needs the user.
- **A `Stale` service's badge renders as a distinct stale indicator, never as a number and never as zero.** A wrong count presented confidently is worse than an honest "unknown".
- Recovery: reload the webview. If that fails, destroy and recreate it. No state is lost; the page reloads from the session in its profile.
- Repeated staleness is logged and surfaced in settings, because it is the signal that §9.1/§9.2 are not holding on the user's machine.

---

## 10. Notifications

The Rust dispatcher owns notifications. Page-originated notifications are suppressed in every service webview, preventing duplicates and giving uniform behaviour across services that implement web push and those that don't. The agent replaces `window.Notification` with a stub that reports permission as `denied`. On Windows, WebView2's `PermissionRequested` event additionally denies the notification permission. Whether WKWebView exposes the web `Notification` API at all is unverified and is checked in M4.

- **Trigger:** a report contains `MessageRef` ids absent from the previous report. For `title`/`selector` strategies with no message detail, an *increase* in count is the trigger. Reports with `count = None` never trigger, and do not reset the comparison baseline.
- **Dedupe:** per-service bounded ring buffer of seen ids (last 500), persisted to `state.json`.
- **Cold start:** the first report after launch seeds state silently. Never notify for an existing inbox at startup.
- **Batching:** more than `notification_batch_threshold` new messages in one cycle collapses to `"Work — 12 new messages"`.
- **Content:** `fetch` strategies produce `From — Subject`. `title`/`selector` strategies produce `"iCloud — 3 new messages"`. This asymmetry is visible to users and is the main practical argument for writing a `fetch` recipe for a service.
- **Activation:** focuses the window, selects the service, and — where a deep link is available (Gmail's `#all/{id}`) — navigates to the message.
- ⚠️ `tauri-plugin-notification` ignores action options on desktop and delivers **no click event** (plugins-workspace#2150, open). Activation therefore needs a different delivery crate. `user-notify` is the candidate named in that issue; its click API is unverified. Validate it at the start of M4. If no crate provides clicks on both platforms, activation is dropped and notifications are display-only.

---

## 11. Sidebar and icons

Vertical strip, ~64px, Slack-style. Drag to reorder, writes back to `config.toml`.

### 11.1 Icon resolution

First success wins:

1. User override — `source = "file"` (copied into `{data_dir}/icons/`) or `"url"`.
2. `<link rel="apple-touch-icon">` from the service origin — usually the best square asset available.
3. `<link rel="icon">` / `shortcut icon`, largest declared `sizes`.
4. `/favicon.ico` at the origin root.
5. Generated fallback: first letter of the service name on a colour derived deterministically from the service id.

Fetched once, normalised to 128×128 PNG, cached on disk. Refetched on user request.

Because every service has a resident webview already loaded on its own origin (§8.2), steps 2–4 are read **from the live document**, which is both more accurate than guessing URLs and free.

### 11.2 No third-party favicon services

Google's `s2/favicons` and equivalents are not used.

The concern is not the user's credentials — those are never involved. It is that requesting `s2/favicons?domain=mail.internal.example.co.jp` tells a third party, alongside the user's IP address, that this machine uses that mail host. For Gmail or Outlook that discloses nothing. For a self-hosted, corporate, or otherwise non-obvious webmail host, it discloses the host's existence and name to a party with no reason to learn it.

It is a minor leak, and it is also entirely avoidable: §11.1 step 2–4 reads the icon from a page already open. The third-party service would add a dependency and a disclosure in exchange for nothing.

### 11.3 Badge rendering

Unread count as a rounded pill, top-right of the icon. Zero renders no pill. `Stale` (§9.4) renders a distinct indicator. `NeedsAttention` — a report with `count = None` (signed out, service unreachable, recipe failing) — renders a warning glyph.

---

## 12. Application badge — removed

*Removed in v0.3.* Eluma shows no Dock (macOS) or taskbar (Windows) badge. Unread counts appear only on the sidebar icons (§11.3). The section number is kept so that references elsewhere stay valid.

---

## 13. Storage layout

```
{config_dir}/
  config.toml
{data_dir}/
  icons/              normalised PNG cache
  state.json          seen-message ids, last counts, profile UUID map
  webview/            WebView2 user data folders (Windows)
  logs/
```

macOS: `~/Library/Application Support/{bundle-id}/`, plus WebKit-managed stores at `~/Library/WebKit/WebsiteDataStore/<UUID>/`.
Windows: `%APPDATA%\{app}\`.

---

## 14. Security

- **No credentials stored** (§6.1). The attack surface for stored secrets is empty because there are no stored secrets.
- **Recipes are reviewed code, compiled into the app.** Nothing is loaded from disk or the network at runtime; no `eval`. A recipe reaches users only through a reviewed PR and a release.
- **Recipe `fetch` is same-origin only**, restricted to the service URL's origin. The shared `fetch` helper enforces this; a recipe cannot pass a cross-origin URL through it.
- **The injected agent is read-only** with respect to page state, and its IPC surface is one message type (§7.4).
- **The report command is reachable by the page's own scripts** (§7.4). It is the only command granted to service webviews, it is scoped per webview label and origin, and Rust validates every field (id matches the calling webview, bounded string lengths, bounded message count).
- A per-service panel shows what its recipe does: which strategy, which selector or endpoint, what it reads.

---

## 15. Known risks

| # | Risk | Mitigation |
|---|---|---|
| R1 | Windows throttling flags may not work (§9.2) | Validate before committing to Windows. Liveness monitor makes failure visible rather than silent. |
| R2 | `Window::add_child` unstable (§8.1) | `WebviewHost` abstraction with a child-window fallback |
| R3 | `data_store_identifier` crash, tauri#12843 (§5.2) | Closed upstream (wry#1512). Use well-formed UUIDs; confirm in the M1 spike |
| R4 | Gmail Atom feed undocumented, may vanish (§7.1) | Automatic fallback to `title`; treat breakage as routine |
| R5 | `selector` recipes break on vendor redesigns | `title` is the default; `selector` is last resort |
| R6 | macOS WebContent suspension under memory pressure | Liveness monitor destroys and recreates (§9.4) |
| R7 | N resident SPAs is memory-expensive (§1.1) | Accepted by design. Escape hatch recorded in §17 Q4. |
| R8 | Desktop notification clicks unsupported by `tauri-plugin-notification`, plugins-workspace#2150 (§10) | Validate an alternative crate at the start of M4; otherwise notifications are display-only |
| R9 | Service pages can call the report command themselves (§7.4) | Single narrow command, per-webview scope, strict validation; a forged report can only mislead that service's own badge |

---

## 16. Roadmap

| Milestone | Contents | Exit criterion |
|---|---|---|
| **M1** Shell | Tauri app, sidebar, `WebviewHost`, service CRUD in the app UI, profiles (shared/isolated), favicon resolution, TOML config | Two iCloud accounts signed in side by side; a URL becomes a usable mail tab |
| **M2** Unread | Agent injection, `Recipe` interface, `title` + `fetch` + `selector` strategy helpers, Gmail / iCloud / Outlook / generic recipes, IPC, sidebar badges | N Gmail accounts plus iCloud show live counts while the window is in the background |
| **M3** Background survival | `background_throttling`, App Nap assertion, Windows browser args, liveness monitor, stale indicator | Badges still correct after 2 hours minimised, on both platforms — or the Windows story is honestly documented as degraded |
| **M4** Notifications | Rust dispatcher, dedupe, cold-start seeding, batching, click-to-activate, deep links | iCloud fires a native notification with Eluma in the background |
| **M5** Recipes | Additional recipes, recipe description panel, contribution docs | A contributor adds Fastmail by adding one recipe module, without touching Rust |

Release engineering (code signing, notarization, installers, auto-update, crash reporting, Ferdium import) was a milestone in v0.2 and is out of scope as of v0.3.

M3 is deliberately early: it is the project's central risk, and discovering late that Windows cannot keep webviews alive would invalidate the platform target.

---

## 17. Open questions

1. **Do the Windows throttling flags actually work?** (§9.2) The highest-value experiment in the project. A one-afternoon WebView2 spike answers it and determines whether Windows is a first-class target.
2. **Reconcile interval.** Is 60s right for `fetch` recipes? Needs testing against Google's rate limiting; jitter is assumed, the acceptable floor is unknown.
3. **Gmail account addressing.** Does `/mail/u/{email}/` resolve reliably, or is `?authuser=` required? Affects whether services can be pinned by address rather than by shift-prone numeric index.
4. **Memory escape hatch.** If N resident webviews proves too heavy, the alternative is Rust-side polling via `Webview::cookies_for_url()`, which permits evicting webviews entirely at the cost of the cookie bridge's Windows problems (§7). Recorded, not planned.
5. ~~**Recipe distribution.**~~ Resolved in v0.3: recipes are code in this repository (§4, §7.5).
6. **Licence.** MIT / Apache-2.0 / GPL-3.0. Tauri itself is MIT/Apache-2.0 dual-licensed.
7. **Name.** Confirm `Eluma` is clear on crates.io, npm, GitHub, and trademark registries before first release.
8. **Notification activation.** Does any maintained crate deliver notification click events on both macOS and Windows (§10)?
9. **Web `Notification` in WKWebView.** Does WKWebView expose `window.Notification` to remote pages at all, and does a page-side stub also cover Service Worker `showNotification` (§10)?

---

## 18. References

**Tauri / wry**
- [tauri#11798 — add `data_store_identifier`](https://github.com/tauri-apps/tauri/pull/11798) · [config reference](https://v2.tauri.app/reference/config/)
- [tauri#12843 — `data_store_identifier` crash](https://github.com/tauri-apps/tauri/issues/12843)
- [tauri#12181 — disable background throttling](https://github.com/tauri-apps/tauri/pull/12181) · [wry#1246 — macOS background suspension](https://github.com/tauri-apps/wry/issues/1246)
- [tauri#8280 — multiple webviews per window](https://github.com/tauri-apps/tauri/pull/8280) · [tauri#10420 — multiwebview positioning](https://github.com/tauri-apps/tauri/issues/10420)
- [tauri#15266 — remote-origin IPC requires an explicit `remote` capability (2.11.1)](https://github.com/tauri-apps/tauri/pull/15266)
- [plugins-workspace#2150 — Notification onclick event](https://github.com/tauri-apps/plugins-workspace/issues/2150)

**Platform**
- [WKPreferences.inactiveSchedulingPolicy](https://developer.apple.com/documentation/webkit/wkpreferences/inactiveschedulingpolicy-swift.property)
- [Gmail Inbox Feed](https://developers.google.com/workspace/gmail/gmail_inbox_feed)

**Prior art**
- [When a rewrite isn't: rebuilding Slack on the desktop](https://slack.engineering/rebuilding-slack-on-the-desktop-308d6fe94ae4)
- [ferdium/ferdium-app](https://github.com/ferdium/ferdium-app)