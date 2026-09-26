# Eluma — Detailed Design

> **Based on:** `SPEC.md` Draft v0.3 · **Scope:** milestones M1–M5 · **Status:** Draft for review
>
> Section references of the form "§n" point to `SPEC.md`. This document does not restate the spec; it decides *how* to build it.

---

## 0. Decisions recorded before this design

These were agreed with the project owner on 2026-09-23 and are already reflected in SPEC v0.3.

| # | Decision |
|---|---|
| D1 | `UnreadReport.count` is `Option<u32>`. `Some(n)` = healthy, `None` = signed out or service unavailable. The same report is re-sent every 30s and doubles as the heartbeat. There is only one message type. |
| D2 | No Dock / taskbar badge. Counts appear only on sidebar icons. |
| D3 | Recipes are TypeScript code compiled into the agent, one module per service, all implementing one `Recipe` interface. There is no recipe file format and no loading from disk. |
| D4 | The recipe for a service is selected automatically from its URL. It is never stored in `config.toml`, and the user cannot override it. |
| D5 | Service CRUD is done entirely in the app's GUI. The app writes changes back to `config.toml`. |
| D6 | The shell UI is built with React, Vite and TypeScript, and pnpm is the package manager. |
| D7 | Testing uses `cargo test` and Vitest, plus a manual verification checklist against real services. |
| D8 | Release engineering (the former M6) is out of scope. |

---

## 1. Architecture overview

### 1.1 System diagram

```
 Main window (Tauri Window, multiwebview via `add_child`)
┌──────────────────────────────────────────────────────────────────────┐
│ ┌──────────┐ ┌─────────────────────────────────────────────────────┐ │
│ │ shell    │ │ svc-<id> (active)  : positioned at content rect     │ │
│ │ webview  │ └─────────────────────────────────────────────────────┘ │
│ │ (React,  │   svc-<id> (inactive): same size, positioned offscreen  │
│ │ sidebar) │   ... one per configured service, all resident          │
│ └────▲─────┘                                                         │
└──────┼───────────────────────────────────────┬───────────────────────┘
       │ events: services-changed,             │ invoke("report_unread")
       │ status-changed, select-service        │ (per-service remote capability)
       │                                       ▼
┌──────┴───────────────────────────────────────────────────────────────┐
│ Rust core (src-tauri)                                                │
│  config ── services ── host (WebviewHost) ── profile (ProfileBackend)│
│                  │           │                                       │
│                  ▼           ▼                                       │
│              unread::store ◀── agent_bridge (validate report)        │
│                  │  └──▶ liveness (30s ticks, Stale, recovery)       │
│                  ▼                                                   │
│              notify::diff ─▶ notify::dispatcher ─▶ NotificationSink  │
│  state (state.json) · icons (cache, normalise) · platform (App Nap,  │
│  WebView2 permission handler)                                        │
└──────────────────────────────────────────────────────────────────────┘

 Settings window (separate WebviewWindow "settings", same React bundle, route #/settings)
```

### 1.2 Technology stack

Versions are the latest stable releases as of 2026-09-23. They were checked against crates.io, npm, and the tauri-v2.11.6 source.

| Area | Choice |
|---|---|
| Rust | stable toolchain, edition 2021 |
| App framework | `tauri` 2.11.x with Cargo feature `unstable` (for `Window::add_child`). `dynamic-acl` is on by default and is required (§4.3). |
| Build | `tauri-build` 2.x (with `AppManifest` for the remote-callable command) |
| Config | `toml_edit` 0.25 (round-trip writes), `serde` + `toml` (typed reads) |
| IDs | `uuid` (v5 feature) |
| Errors / logs | `thiserror`, `tracing`, `tracing-subscriber`, `tracing-appender` (daily file in `{data_dir}/logs/`) |
| Async | `tokio` (via Tauri's async runtime) |
| Icons | `reqwest` (unauthenticated icon download only), `image` (decode + resize to 128×128 PNG) |
| Notifications | `tauri-plugin-notification` 2.4 as the baseline sink. A click-capable sink is chosen in the M4 spike (§6.3). |
| macOS | `objc2-foundation` 0.3 (`NSProcessInfo`, `NSString` features) for the App Nap assertion |
| Windows | `webview2-com` (the version matching wry 0.55) for the `PermissionRequested` handler |
| Frontend | React 19, Vite, TypeScript (strict), `@tauri-apps/api` 2.11 |
| Agent | TypeScript, bundled by Vite in library mode into one IIFE file, embedded in Rust with `include_str!` |
| Tests | `cargo test`; Vitest + jsdom + `@testing-library/react` |
| Lint / format | `cargo fmt`, `cargo clippy -D warnings`, ESLint (typescript-eslint), `tsc --noEmit`, Prettier |

### 1.3 Repository layout

```
/
├── SPEC.md  design.md  tasks.md  README.md  LICENSE
├── package.json  pnpm-lock.yaml  tsconfig.json  vite.config.ts  vite.agent.config.ts
├── src/                        # React shell (sidebar + settings window)
│   ├── main.tsx  App.tsx
│   ├── ipc/                    # typed wrappers over invoke/listen
│   ├── sidebar/                # Sidebar, ServiceIcon, Badge
│   └── settings/               # ServiceList, ServiceForm, GlobalSettings, RecipePanel, Diagnostics
├── agent/                      # injected agent (runs inside service webviews)
│   ├── main.ts                 # entry: frame guard, recipe selection, loop
│   ├── core/                   # report.ts, loop.ts, notification-stub.ts, icons.ts
│   ├── strategies/             # title.ts, fetch.ts, selector.ts
│   └── recipes/                # types.ts, registry.ts, generic.ts, gmail.ts, icloud.ts, outlook.ts, ...
├── shared/                     # types shared by agent and shell (Recipe metadata, DTOs)
└── src-tauri/
    ├── Cargo.toml  build.rs  tauri.conf.json
    ├── capabilities/shell.json
    └── src/
        ├── main.rs  lib.rs  error.rs  paths.rs
        ├── config/      (model.rs, store.rs, validate.rs)
        ├── state/       (model.rs, store.rs)
        ├── profile/     (mod.rs, macos.rs, windows.rs)
        ├── host/        (mod.rs, multiwebview.rs, child_windows.rs, layout.rs)
        ├── services/    (mod.rs — lifecycle orchestration)
        ├── agent_bridge/(mod.rs, dto.rs, validate.rs, capability.rs)
        ├── unread/      (store.rs, status.rs)
        ├── liveness/    (mod.rs)
        ├── notify/      (diff.rs, seen.rs, dispatcher.rs, sink.rs)
        ├── icons/       (mod.rs)
        ├── platform/    (app_nap.rs [macOS], webview2.rs [Windows])
        └── commands/    (shell-facing commands)
```

`agent/recipes/*` is imported by **both** the agent bundle and the shell. The shell needs each recipe's metadata (default profile and description) to fill in defaults when a service is added (§6.2) and to render the recipe panel (§14). This keeps Rust recipe-agnostic: Rust never knows which recipe a service uses, except through the `recipe_id` string it receives in reports.

---

## 2. Component design

### 2.1 Component list

| Component | Responsibility | Depends on |
|---|---|---|
| `config` | Load and validate `config.toml`; apply edits with `toml_edit`, preserving comments | `paths` |
| `state` | Load and save `state.json` (profile UUID map, seen ids, staleness counters) | `paths` |
| `profile` | Map a profile name to a UUID and to platform-specific webview options | `state` |
| `host` | `WebviewHost` trait: create, destroy, show and hide service webviews and lay them out | `profile`, `agent_bridge` |
| `services` | Orchestrate the service lifecycle: staggered start, CRUD applied to live webviews | `config`, `host`, `unread` |
| `agent_bridge` | Build the agent injection script; add runtime capabilities; implement the `report_unread` command and its validation | `unread` |
| `unread` | Per-service status store; emit `status-changed` | — |
| `liveness` | 30s ticker; mark services `Stale`; run recovery | `unread`, `host`, `state` |
| `notify` | Diff engine, seen-id ring buffer, batching, dispatch to a `NotificationSink`, activation | `unread`, `state`, `host` |
| `icons` | Resolve and cache icons; normalise them to PNG | `config`, `paths` |
| `platform` | macOS App Nap assertion; Windows permission handler | — |
| `commands` | Commands the shell and settings UI call | all of the above |
| shell `src/` | Sidebar, badges, settings UI | `@tauri-apps/api`, `agent/recipes` metadata |
| `agent/` | Read unread counts inside each service webview | — |

### 2.2 Component details

#### 2.2.1 `config`

- **Purpose:** single source of truth for user configuration (§6).
- **Model:**

```rust
pub struct Config {
    pub version: u32,                 // must equal 1
    pub settings: Settings,
    pub services: Vec<ServiceConfig>, // sidebar order = vector order
}
pub struct Settings {
    pub reconcile_interval_seconds: u32,
    pub notifications: bool,
    pub notification_batch_threshold: u32,
    pub badge_sidebar: bool,
}
pub struct ServiceConfig {
    pub id: ServiceId,        // [a-z0-9-]{1,48}, unique
    pub name: String,
    pub url: Url,             // http(s) only
    pub profile: ProfileName, // "default" | "isolated" | named
    pub notifications: bool,
    pub icon: IconSource,     // Favicon | File(PathBuf) | Url(Url)
}
```

- **Public interface:**

```rust
pub fn load(path: &Path) -> Result<Config, ConfigError>;
pub fn write_initial(path: &Path) -> Result<Config, ConfigError>;       // first launch only
pub fn apply(path: &Path, edit: ConfigEdit) -> Result<Config, ConfigError>;
pub enum ConfigEdit {
    AddService(ServiceConfig), UpdateService(ServiceId, ServicePatch),
    RemoveService(ServiceId), Reorder(Vec<ServiceId>), UpdateSettings(SettingsPatch),
}
```

- **Implementation policy:**
  - Every key is **required**. A missing or invalid key is a `ConfigError` that names the file, the key and the reason. The shell shows it on an error screen. Nothing falls back to a default.
  - When the file does not exist (first launch), `write_initial` writes a complete file with every settings key set explicitly (the values from §6) and no services.
  - `apply` re-reads the file, applies the edit to a `toml_edit::DocumentMut`, validates the result, and writes it atomically (temp file + rename). If the file changed on disk since the last load, the latest on-disk content is used as the base.

#### 2.2.2 `state`

```rust
pub struct State {
    pub profiles: BTreeMap<ProfileName, Uuid>,
    pub seen: BTreeMap<ServiceId, SeenRing>,        // ring of up to 500 message ids
    pub staleness: BTreeMap<ServiceId, StalenessStats>, // count, last_at
}
```

- The file is written atomically, debounced (at most once per second, and on exit).
- If the file is missing on first launch, an empty state is created. If a file exists but cannot be parsed, that is an error surfaced in the UI. It is never silently replaced.
- `StalenessStats.last_at` is the time (Unix epoch ms) `count` was last incremented — i.e. the last time the service was marked `Stale` (§2.2.8) — never a report time. A service's last-*report* time is not persisted here; it lives only in the liveness monitor's in-memory tracking (§2.2.8), which `get_diagnostics` (§2.2.12) reads directly.

#### 2.2.3 `profile`

```rust
pub fn resolve(name: &ProfileName, service: &ServiceId, state: &mut State) -> Uuid;
pub trait ProfileBackend {
    fn apply(&self, builder: WebviewBuilder<Wry>, uuid: Uuid) -> WebviewBuilder<Wry>;
    async fn remove(&self, app: &AppHandle, uuid: Uuid) -> Result<(), AppError>;
}
```

- Key derivation:
  - `"default"` → key `default`
  - `"isolated"` → key `isolated:<service_id>`
  - any other name → key `named:<name>`
  - UUID = UUIDv5 over the key, using a fixed project namespace UUID (a constant in code). The result is recorded in `state.profiles`.
- **macOS:** `builder.data_store_identifier(uuid.into_bytes())`. Removal uses `AppHandle::remove_data_store(uuid)` and runs after the webview is closed.
- **Windows:** `builder.data_directory({data_dir}/webview/<uuid>)`. Removal deletes that directory after the webview is closed.
- **Removing a service:** its data store is removed only when it is an `isolated` profile, the user ticked "delete session data" in the confirmation, and no other service uses it.

#### 2.2.4 `host` — `WebviewHost`

```rust
pub trait WebviewHost: Send + Sync {
    fn create(&self, spec: ServiceWebviewSpec) -> Result<(), AppError>;
    fn destroy(&self, id: &ServiceId) -> Result<(), AppError>;
    fn reload(&self, id: &ServiceId) -> Result<(), AppError>;
    fn navigate(&self, id: &ServiceId, url: Url) -> Result<(), AppError>;
    fn activate(&self, id: &ServiceId) -> Result<(), AppError>;   // move to content rect, focus
    fn relayout(&self, content: Rect) -> Result<(), AppError>;    // on window resize
}
pub struct ServiceWebviewSpec {
    pub id: ServiceId, pub url: Url, pub profile: Uuid,
    pub init_script: String,       // agent + injected config
    pub on_page_load: PageLoadHandler, // reports origin changes to unread store
}
```

- **`MultiwebviewHost`** (default):
  - The main window is created with `WindowBuilder`.
  - The shell is `add_child` at `(0, 0, SIDEBAR_WIDTH, height)`.
  - Each service is `add_child` with label `svc-<id>` and the same size as the content rect.
  - The active service is placed at `x = SIDEBAR_WIDTH`. Inactive ones are placed at `x = -(content width + SIDEBAR_WIDTH)`. They are **never** hidden (§8.2). Placing them offscreen, rather than overlapping them, also avoids depending on z-order between child webviews (tauri#11376).
- **`ChildWindowHost`** (fallback, Cargo feature `host-child-windows`):
  - The main window is a `WebviewWindow` hosting the shell.
  - Each service is a borderless `WebviewWindow` with `.parent(&main)`.
  - Service windows follow the main window's `Moved` and `Resized` events.
  - Inactive windows are moved offscreen, not hidden.
- **Common builder settings, per service:**
  - `initialization_script(agent)`, `on_page_load`, `on_navigation` (allow all; used only for logging)
  - `background_throttling(Disabled)` on macOS
  - `additional_browser_args(WEBVIEW2_ARGS)` on Windows
  - the profile backend
- **Windows browser arguments** are one constant, used for **every** webview, the shell included (§5.2, §9.2):
  `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows`
- `SIDEBAR_WIDTH` is a single Rust constant (64px). The shell reads it from `get_snapshot`, so the width is not duplicated.

#### 2.2.5 `services`

- **Startup:** services are created in sidebar order, with a delay of `STARTUP_STAGGER` (1500ms, a named constant) between each. The first service is activated immediately.
- **CRUD:** each command first applies the `ConfigEdit`, then reconciles the live webviews:
  - add → create
  - remove → destroy (and remove the profile if confirmed)
  - URL or profile change → destroy and recreate
  - name, icon or notifications change → update in place, no webview change
  - reorder → emit `services-changed` only
- **Id generation for new services:** slugify the name to `[a-z0-9-]`, then append `-2`, `-3`, … on collision.

#### 2.2.6 `agent_bridge`

- **Injection script:** `format!("window.__ELUMA__ = {json};\n{AGENT_JS}")`. The JSON object is:
  - `serviceId`
  - `serviceUrl`
  - `reportIntervalMs` (30000)
  - `reconcileIntervalMs` (from settings)
- **Runtime capability,** added before the webview is created:

```rust
app.add_capability(
    CapabilityBuilder::new(format!("svc-{id}"))
        .remote(format!("{origin}/*"))   // service URL origin only
        .webview(format!("svc-{id}"))
        .permission("allow-report-unread"),
)?;
```

  `build.rs` declares the command with `tauri_build::Attributes::new().app_manifest(AppManifest::new().commands(&["report_unread"]))`, so the `allow-report-unread` permission exists. Tauri has no API to remove a capability at runtime. When a service is deleted, its capability stays in place but applies to a label that no longer exists, so it is inert. A recreated service reuses the same identifier.

- **Command:**

```rust
#[tauri::command]
async fn report_unread(webview: tauri::Webview, report: UnreadReportDto, …) -> Result<(), ReportError>;
```

- **DTO** (camelCase JSON): `{ serviceId, count: number|null, messages: MessageRefDto[], recipeId, observedAt: number /* ms epoch */, iconCandidates: string[] }`.
- **Validation** (`validate.rs`, pure and unit-tested):
  - `webview.label() == format!("svc-{serviceId}")` and the service exists; otherwise the report is rejected and logged at debug level.
  - `count` is either `null` or an integer ≤ 1,000,000.
  - At most 100 `messages`. Each string is at most 512 characters.
  - `link`, if present, must be `https` and on the service origin.
  - `recipeId` is at most 64 characters and matches `[a-z0-9-]`.
  - At most 8 `iconCandidates`, each `https` or `http`. Candidates whose host is a loopback, private or link-local address are rejected (see §2.2.10).
- `observedAt` is informational only. Liveness uses the time Rust receives the report.

#### 2.2.7 `unread`

```rust
pub enum ServiceStatus {
    Loading,                                   // created, no report yet
    Ok { count: u32 },
    NeedsAttention { reason: AttentionReason }, // count = None
    Stale,
}
pub enum AttentionReason { ReportedNone, OffOrigin, CreateFailed }
```

- **Inputs:**
  - Validated reports: `Some(n)` → `Ok`, `None` → `NeedsAttention(ReportedNone)`.
  - `on_page_load` events: if the page URL's origin differs from the service origin → `NeedsAttention(OffOrigin)`. The status leaves `OffOrigin` on the next report received from the service origin.
  - Liveness: → `Stale`.
- **Output:** `status-changed { serviceId, status }` is emitted to the `shell` webview only, and only when the status actually changes. The shell also receives the full status map via `get_snapshot`.

#### 2.2.8 `liveness`

- A single tokio interval ticks every 30s. For each service:
  - `Loading` or `Ok`/`NeedsAttention(ReportedNone)` with `now − last_report > 2 × 30s + 5s grace` → `Stale`. Increment `state.staleness`, log at warn level, and call `host.reload`.
  - Still `Stale` two ticks after the reload → `host.destroy` then `host.create`.
  - `NeedsAttention(OffOrigin)` is **exempt**, because the agent is not permitted to report there.
- **macOS App Nap:** `platform::app_nap::begin()` returns a token that is kept in managed state while at least one service exists. It uses `NSProcessInfo::processInfo().beginActivityWithOptions_reason(...)`. See §10 for the choice of activity option.

#### 2.2.9 `notify`

- **`diff.rs`** (pure):

```rust
pub fn evaluate(prev: &ServiceNotifyState, report: &ValidReport, seen: &SeenRing) -> DiffOutcome;
pub enum DiffOutcome { Seed, Nothing, NewMessages(Vec<MessageRef>), CountIncrease(u32) }
```

  - The first `Some` report per service since launch → `Seed`. It records the ids and the count, and no notification is sent (cold start).
  - `count = None` → `Nothing`. The baseline is not changed.
  - `messages` present → ids not in `seen` → `NewMessages`.
  - No messages → `count > last_count` → `CountIncrease(count − last_count)`.

- **`seen.rs`:** a ring of 500 ids per service, persisted in `state.json`.
- **`dispatcher.rs`:**
  - Notifications are sent only when `settings.notifications && service.notifications`.
  - Batching: more than `notification_batch_threshold` new messages → one notification `"{name} — {n} new messages"`.
  - Rich content: `"{from} — {subject}"`.
  - Count only: `"{name} — {n} new messages"`.
  - All user-visible strings are English literals in one module, `notify/text.rs`; the spec defines no i18n.
- **`sink.rs`:**

```rust
pub trait NotificationSink: Send + Sync {
    fn show(&self, n: OutgoingNotification) -> Result<(), AppError>;
}
pub struct OutgoingNotification { pub service: ServiceId, pub title: String, pub body: String, pub link: Option<Url> }
```

  - The baseline sink is `tauri-plugin-notification`, which is display-only.
  - If the M4 spike finds a click-capable crate, a second sink is added. On click it:
    - focuses the main window,
    - calls `host.activate(service)` and emits `select-service`,
    - calls `host.navigate(service, link)` when a link is present.

#### 2.2.10 `icons`

- **Resolution** follows §11.1:
  1. User override: a file (copied into `{data_dir}/icons/<id>.src`) or a URL.
  2. The agent's `iconCandidates`, already ordered best first: `apple-touch-icon`, then `rel=icon` by the largest declared `sizes`, then `/favicon.ico`.
  3. If nothing succeeds, no PNG is stored and the shell renders the generated letter icon (step 5).
- Download uses `reqwest` without cookies, a 5s timeout and a 1 MiB cap. The destination of the request and of every redirect it follows must not resolve to a loopback, private or link-local address; otherwise the download fails. The image is decoded, resized to 128×128 PNG, and stored at `{data_dir}/icons/<id>.png`.
- Resolution runs only when there is no cached PNG or the user clicked "Refresh icon". After it succeeds, the shell receives `services-changed`.
- The shell renders the PNG through Tauri's asset protocol. `assetProtocol.scope` is limited to `$APPDATA/icons/**`.

#### 2.2.11 `platform`

- **`app_nap.rs`** (macOS only): `begin() -> ActivityToken` and `end(token)`.
- **`webview2.rs`** (Windows only): after a service webview is created, `webview.with_webview(|pv| …)` gets the `ICoreWebView2` and registers `add_PermissionRequested`. The handler sets `COREWEBVIEW2_PERMISSION_STATE_DENY` for `COREWEBVIEW2_PERMISSION_KIND_NOTIFICATIONS` and leaves every other permission at the default.

#### 2.2.12 `commands` (shell and settings webviews only)

The static capability `capabilities/shell.json` grants these commands to the `shell` and `settings` webviews only.

| Command | Input | Output |
|---|---|---|
| `get_snapshot` | — | `{ settings, services[], statuses, sidebarWidth, configError? }` |
| `add_service` | `{ name, url, profile }` | `ServiceConfig` |
| `update_service` | `{ id, patch }` | `ServiceConfig` |
| `remove_service` | `{ id, deleteSessionData }` | — |
| `reorder_services` | `{ ids }` | — |
| `select_service` | `{ id }` | — |
| `update_settings` | `{ patch }` | `Settings` |
| `set_icon_override` | `{ id, source: favicon \| file(path) \| url }` | — |
| `refresh_icon` | `{ id }` | — |
| `reload_service` | `{ id }` | — |
| `get_diagnostics` | — | `{ services: [{ serviceId, name, status, lastReportAgeMs, staleCount, lastStaleAt }] }`, one entry per configured service in sidebar order; `lastReportAgeMs` is `null` for a service that has never reported, `lastStaleAt` is `null` before its first stale episode |
| `open_settings` | — | opens or focuses the settings window |

Events sent to the shell: `services-changed`, `status-changed`, `select-service`.

#### 2.2.13 Shell UI (`src/`)

- **Sidebar** (`shell` webview, 64px):
  - A vertical list of `ServiceIcon`s: icon, `Badge`, and the name in a tooltip.
  - HTML5 drag and drop reorders the list and calls `reorder_services`.
  - A "+" button at the bottom and a settings (gear) button both call `open_settings`.
- **Badge rendering** (§11.3), when `badge_sidebar` is true:
  - `Ok(0)`: no pill.
  - `Ok(n)`: a pill showing `n` (capped at "999+").
  - `Stale`: a distinct hollow grey dot.
  - `NeedsAttention`: a warning glyph.
  - `Loading`: nothing.
- **Settings window** (`#/settings`):
  - Service list with edit and delete.
  - Add form: URL, then name. The profile is pre-filled from `matchRecipe(url).defaultProfile`. It shows `default`, `isolated`, or a named profile chosen from the existing names or entered as a new one.
  - Per-service edit: name, URL, profile, notifications, icon override.
  - Delete confirmation, with the "delete session data" option shown only for isolated profiles.
  - Global settings: every `[settings]` key.
  - Recipe panel (§14): recipe name, strategy, and what it reads, from `recipe.describe()`.
  - Diagnostics (§9.4): a table (service name, status, last report age, stale count, last stale time) from `get_diagnostics`, with a manual refresh button; also refetched on `services-changed`.
- **State management:** React context plus `useSyncExternalStore` over a small store fed by `get_snapshot` and the events. No state library.

#### 2.2.14 Agent (`agent/`)

- **Entry (`main.ts`):**
  1. If `window.top !== window`, return (frame guard; needed on Windows).
  2. Read `window.__ELUMA__`, then `delete` it from the page globals.
  3. Install the `Notification` stub.
  4. `recipe = matchRecipe(new URL(serviceUrl))`.
  5. If `location.origin !== serviceOrigin`, return. Rust handles the off-origin state.
  6. Start the loop.
- **Loop (`core/loop.ts`):**
  - `recipe.watch(ctx, onChange)` calls `read()` whenever a change is observed, debounced by 500ms.
  - The reconcile timer (interval ± 20% jitter) calls `read()`.
  - The report timer (30s) re-sends the last result.
  - Each result is sent through `report.ts`, which calls `window.__TAURI_INTERNALS__.invoke('report_unread', …)`. If the call fails, the error is logged to the console and the loop continues; the next report retries.
  - `iconCandidates` are attached to the first report after load only.
- **Recipe interface (`recipes/types.ts`):**

```ts
export type ProfileDefault = 'default' | 'isolated';

export type UnreadResult =
  | { readonly count: number; readonly messages: readonly MessageRef[] }
  | { readonly count: null };

export interface MessageRef {
  readonly id: string;
  readonly from: string | null;
  readonly subject: string | null;
  readonly link: string | null;
}

export interface RecipeContext {
  readonly serviceUrl: URL;
  readonly document: Document;
  readonly fetch: SameOriginFetch; // rejects any URL not on serviceUrl's origin
}

export interface RecipeDescription {
  readonly strategy: 'title' | 'fetch' | 'selector';
  readonly reads: string;          // human-readable, e.g. "GET /mail/u/<addr>/feed/atom"
}

export interface Recipe {
  readonly id: string;
  readonly displayName: string;
  readonly defaultProfile: ProfileDefault;
  matches(serviceUrl: URL): boolean;
  describe(serviceUrl: URL): RecipeDescription;
  read(ctx: RecipeContext): Promise<UnreadResult>;
  watch(ctx: RecipeContext, onChange: () => void): () => void; // returns unsubscribe
}
```

- **Registry (`recipes/registry.ts`):** an ordered array of specific recipes, then `generic`. `matchRecipe` returns the first recipe whose `matches()` is true. `generic.matches` always returns true.
- **Strategy helpers:**
  - `titleCount(doc, pattern: RegExp) → number | null`. `watchTitle(doc, cb)` puts a `MutationObserver` on `<head>`, so title elements that are replaced are also caught.
  - `fetchText(ctx, path) → { status, redirected, url, body }`. `parseGmailAtom(body) → { fullcount, entries }` uses `DOMParser`.
  - `selectorCount(doc, selector, mode: 'text' | 'rows') → number | null`. `watchSelector(doc, selector, cb)`.
- **Recipes in M2:**
  - `gmail`:
    - Matches `mail.google.com`. `defaultProfile: 'default'`.
    - Uses `fetch` of `/mail/u/<segment>/feed/atom`, where `<segment>` comes from the service URL.
    - On first use, validates `status 200`, not redirected, and a parseable `<fullcount>`. If validation fails, it switches to the title strategy for the rest of the page's life (§7.1).
    - `messages` come from feed entries. `id` is a hash of the entry id; `link` is a deep link `#all/<message id>` on the service URL.
    - `watch` uses `watchTitle`, so a change to the title triggers a fetch.
  - `icloud`: matches `www.icloud.com/mail`, `isolated`, title strategy.
  - `outlook`: matches `outlook.live.com`, `isolated`, title strategy.
  - `generic`: matches any URL, `isolated`, title pattern `\((\d+)\)`.
- **The `count = null` rule** (D1) differs by recipe:
  - The title strategy returns `0` when the title contains no count; a same-origin login page cannot be told apart from an empty inbox.
  - Specific recipes return `null` when they detect a signed-out state. Gmail detects it from a feed redirect or a 401 together with a title that has no count. The iCloud and Outlook detection rules are defined in M2 from observation of the real pages (task 2.8).
- **Notification stub:** `window.Notification` is replaced with a class whose `permission` is `'denied'`, whose `requestPermission()` resolves to `'denied'`, and whose constructor throws. It is installed before any page script runs.

---

## 3. Data flow

### 3.1 Unread path

```
page DOM/title ─(MutationObserver)─▶ recipe.read()
                                          │ UnreadResult
                                          ▼
                     agent report.ts ── invoke("report_unread", dto)
                                          │  (remote capability: origin + label)
                                          ▼
          agent_bridge::validate ── reject? ─▶ log, drop
                                          │ ValidReport
                        ┌─────────────────┼──────────────────────┐
                        ▼                 ▼                      ▼
                unread::store      liveness.touch(id)     notify::evaluate
                        │ status changed?                         │ DiffOutcome
                        ▼                                         ▼
              emit_to("shell", status-changed)          dispatcher ─▶ sink
                        ▼
                  Sidebar Badge
```

### 3.2 Data transformations

| Stage | Format |
|---|---|
| Agent → Rust | JSON DTO (camelCase), validated into `ValidReport` |
| Rust → shell | `ServiceStatus` serialized as `{ kind: "loading" \| "ok" \| "needsAttention" \| "stale", count?, reason? }` |
| Persisted | `config.toml` (user-owned, comments preserved); `state.json` (app-owned) |
| Icons | remote bytes → `image` decode → 128×128 PNG on disk → asset URL in the shell |

---

## 4. API interfaces

### 4.1 Internal APIs

- Shell ↔ Rust: the commands and events in §2.2.12.
- Agent → Rust: the `report_unread` command only (§2.2.6).
- Rust → agent: none after injection. Rust never calls `eval` into a service page.

### 4.2 External interfaces

- **Service pages:** loaded as-is. The only additions are the agent and the Notification stub.
- **Gmail Atom feed:** same-origin `GET`, made by the agent inside the page. It is undocumented for consumer accounts (§7.1).
- **Icon URLs:** unauthenticated `GET` from Rust, only for URLs the agent found on the service's own page, or a URL the user entered. No third-party favicon service is used (§11.2).
- **OS notifications:** through the selected `NotificationSink`.

### 4.3 Capabilities

| Capability | Scope | Grants |
|---|---|---|
| `shell` (static, `capabilities/shell.json`) | webviews `shell`, `settings`; local app origin | the app commands in §2.2.12; `core:event:default`; `core:window` focus |
| `svc-<id>` (runtime) | webview `svc-<id>`; remote `<service origin>/*` | `allow-report-unread` only |

---

## 5. Error handling

### 5.1 Error classes

| Error | Handling |
|---|---|
| `ConfigError` (parse, missing key, invalid value) | The app starts in an error state. The shell shows the path, the key and the reason. No services are started and no defaults are substituted. |
| `StateError` (corrupt `state.json`) | Same as `ConfigError`. The file is not overwritten. |
| Webview creation failure | That service → `NeedsAttention(CreateFailed)`. Logged at error level. Other services continue. |
| Invalid report | Dropped and logged at debug level. The agent is not notified. |
| Recipe read failure (exception, network) | The agent reports `count: null` for that cycle (D1). |
| Icon resolution failure | Logged. The shell shows the generated letter icon (§11.1 step 5 is part of the spec, not a fallback). |
| Notification sink failure | Logged at warn level. Unread state is unaffected. |
| Profile removal failure | Reported to the settings UI after the service has been removed; the data store is left in place. |

### 5.2 Reporting and logging

- `tracing` writes to stderr in development and to a daily file in `{data_dir}/logs/` in all builds.
- Every `AppError` that reaches a command is serialized as `{ kind, message }` and shown in the settings UI.
- The settings Diagnostics view shows repeated staleness per service (§9.4).

---

## 6. Security design

### 6.1 Authentication and authorization

- Eluma stores no credentials (§6.1). Each service authenticates in its own webview.
- Service pages can call only `report_unread`, through a capability scoped to one webview label and one origin. Because the page's own scripts can call it too (R9), the command validates every field, and a forged report can affect only that service's badge and notifications.

### 6.2 Data protection

- `config.toml` contains no secrets.
- `state.json` contains message ids (hashes) only, never subjects or senders.
- Notification text (sender, subject) is held in memory only.
- Recipe `fetch` goes through `SameOriginFetch`, which throws on any cross-origin URL.
- The agent never mutates the DOM, never clicks, and never submits (§7.3). Review checks recipes for this.

### 6.3 Items that need verification (spikes)

| Id | Question | When |
|---|---|---|
| SP1 | Does `add_child` multiwebview with 4+ children, offscreen positioning and resize behave on macOS 14 and Windows 11? | M1 start |
| SP2 | Do two `data_store_identifier` stores (two iCloud logins) run side by side without crashing? | M1 start |
| SP3 | Does a runtime `remote` capability let a remote page `invoke` `report_unread`, with the exact `AppManifest` permission name? | M1 start |
| SP4 | Do the WebView2 throttling flags keep a hidden or minimized page's timers and `MutationObserver` running (§17 Q1)? | M1 start (§17 Q1, highest-value experiment) |
| SP5 | Which crate delivers notification clicks on macOS and Windows (`user-notify` first)? | M4 start |
| SP6 | Does WKWebView expose `window.Notification` to remote pages, and does the stub also suppress Service Worker `showNotification`? | M4 start |
| SP7 | On a load failure, does the agent run on the error page (so it can report `null`), or does the service go `Stale`? | M2 |

---

## 7. Test strategy

### 7.1 Unit tests

- **Rust (`cargo test`):**
  - config load and validation for every missing or invalid key; round-trip edits that preserve comments
  - profile key → UUID determinism
  - report validation
  - the diff engine (seed, None, new ids, count increase, batching threshold)
  - the seen ring (capacity 500, persistence)
  - the liveness state machine, with an injected clock
  - service id slug generation
- **Agent (Vitest + jsdom):**
  - title parsing and observer (including a replaced `<title>` element)
  - Gmail Atom parsing from fixture files, and fallback to title on non-200, redirect or malformed body
  - selector modes
  - `SameOriginFetch` rejection
  - the frame guard
  - the Notification stub
  - recipe matching for each recipe
  - the loop timers (fake timers)
- **Shell (Vitest + Testing Library):**
  - Badge in every status
  - the add form pre-fills the profile from the recipe
  - reorder calls the command with the new order
- **Coverage:** no numeric target. Every pure module listed above has tests for each branch.

### 7.2 Integration and manual verification

- A manual checklist (`docs/manual-checks.md`, started in M1 and extended each milestone) maps each milestone exit criterion in §16 to concrete steps on real Gmail, iCloud and Outlook accounts, on macOS 14+ and Windows 11.
- Spikes SP1–SP7 each record their result in `docs/spikes/SPn.md`.

---

## 8. Performance

### 8.1 Expected load

- Target: N ≈ 5–10 resident webmail SPAs. Memory is accepted as the dominant cost (R7). It is measured, not optimized, in M3.
- IPC volume: about one report per service every 30s, plus one per change. This is negligible.

### 8.2 Approach

- Startup is staggered (§2.2.5).
- Fetch reconcile uses ±20% jitter so services do not poll in lockstep.
- Observer callbacks are debounced by 500ms.
- `status-changed` is emitted only on change, not on every report.
- `state.json` writes are debounced.

---

## 9. Build and configuration

### 9.1 Build

- `pnpm build` runs both `vite build` (shell) and `vite build -c vite.agent.config.ts` (agent → `src-tauri/agent-dist/agent.js`).
- `tauri.conf.json` `build.beforeBuildCommand` / `beforeDevCommand` call these scripts. `include_str!("../agent-dist/agent.js")` embeds the agent, so a missing build fails at compile time.
- Development: the user runs `pnpm tauri dev` manually.
- Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`, `cargo clippy`, `cargo test`.
- No installers, signing or notarization (D8).

### 9.2 Configuration management

- `{config_dir}/config.toml` and `{data_dir}/…` are resolved with Tauri's path resolver (`app_config_dir`, `app_data_dir`).
- No environment variables are used for configuration.
- `tauri.conf.json`: `app.windows` is empty, because the main window is built in code (it needs `add_child`). `security.csp` applies to the shell. `assetProtocol` is enabled with the icons scope.

---

## 10. Implementation notes

- **Minimum Tauri version: 2.11.1.** It is needed for remote-IPC ACL behaviour. `background_throttling` needs 2.3, `data_store_identifier` 2.2 and `on_document_title_changed` 2.8.
- `WebviewBuilder` and `add_child` are behind the `unstable` feature. Keep every call to them inside `host/multiwebview.rs`.
- **App Nap activity option:** SPEC §9.1 specifies `NSActivityUserInitiated`. That option also disables idle system sleep, so the Mac would never sleep while Eluma runs. This design proposes `NSActivityUserInitiatedAllowingIdleSystemSleep`. **This needs owner confirmation** (see "Open points").
- On Windows the initialization script also runs in subframes, so the frame guard must come first in `agent/main.ts`.
- `additional_browser_args` replaces wry's defaults. The constant in §2.2.4 re-includes them.
- Use a real UUIDv5 for data store identifiers. Never use a zero or placeholder value (tauri#12843).
- Do not use `WebviewWindow` as a command parameter for `report_unread`. Child webviews are not `WebviewWindow`s, so that parameter fails with an error. Use `tauri::Webview`.
- No fallback values anywhere in config handling (project rule).

### Open points for owner review

1. The App Nap activity option (above).
2. Settings and service CRUD live in a **separate settings window**, not inside the 64px sidebar webview.
3. `STARTUP_STAGGER` = 1500ms and fetch jitter = ±20% are internal constants, not config keys.
4. Deleting an isolated service offers to delete its session data (a checkbox, off by default).
5. The generic title recipe cannot detect a signed-out state on the same origin, so it reports `0` there, not `null`.
