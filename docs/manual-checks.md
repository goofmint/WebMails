# Manual verification checklist

This checklist maps each milestone exit criterion in `SPEC.md` §16 to manual checks against real services. The owner runs the app (`pnpm tauri dev`) and ticks each item after verifying it. Record the date, platform and result under each item.

Platforms: macOS 14+, Windows 10 and Windows 11 (SPEC target platforms).

## M1 — Shell

Exit criterion (SPEC §16): two iCloud accounts signed in side by side; a URL becomes a usable mail tab.

- [ ] Two iCloud accounts are signed in side by side (macOS)
- [ ] Two iCloud accounts are signed in side by side (Windows 11)
- [ ] Adding a URL in the settings window produces a usable mail tab

### Task 1.6 — `WebviewHost` / `MultiwebviewHost`

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] The shell sidebar renders at exactly 64px wide, full window height, with no gap or overlap against the content area (macOS)
- [ ] The shell sidebar renders at exactly 64px wide, full window height, with no gap or overlap against the content area (Windows 11)
- [ ] With one service configured in `config.toml`, it is visible and interactive in the content area on launch (macOS) (Task 1.8 replaced the hard-coded `https://example.com` test service used to check this before `config.toml`-driven startup existed — use a real configured service now)
- [ ] Same single-service startup rendering (Windows 11)
- [ ] Drag-resizing the window relays out the shell and the active service with no stale geometry, gap or overlap (macOS)
- [ ] Drag-resizing the window relays out the shell and the active service with no stale geometry, gap or overlap (Windows 11)
- [ ] Changing the display scale factor (moving the window to a different-DPI display, or changing OS scaling) relays out both webviews correctly (macOS)
- [ ] Changing the display scale factor relays out both webviews correctly (Windows 11)
- [ ] Minimising and restoring the window leaves the shell and the active service in their prior layout, with no crash (macOS)
- [ ] Minimising and restoring the window leaves the shell and the active service in their prior layout, with no crash (Windows 11)

### Task 1.8 — Service lifecycle orchestration

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] With two or more services in `config.toml`, they are created in sidebar order at launch, each roughly `STARTUP_STAGGER` (1500ms) after the previous one (macOS)
- [ ] Same staggered startup order and timing (Windows 11)
- [ ] The first service to finish creating is activated immediately, before the rest have started (macOS)
- [ ] Same immediate first-activation behaviour (Windows 11)
- [ ] One service configured with an unreachable URL fails to create without stopping the other configured services from starting (macOS)
- [ ] Same unreachable-URL isolation (Windows 11)
- [ ] Starting the app with an invalid `config.toml` (e.g. a missing required key) starts no services, and the file on disk is byte-for-byte unchanged afterward (macOS)
- [ ] Same invalid-`config.toml` behaviour (Windows 11)

### Task 1.7 — Fallback host (`ChildWindowHost`, `cargo tauri dev --features host-child-windows`)

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] Built with `--features host-child-windows`: the shell and the hard-coded test service render correctly, switching (would-be) services relays out and focuses correctly, and drag-resizing, drag-moving, minimising/restoring and closing the main window all keep the service window(s) in sync (matching geometry offscreen/onscreen, closed on quit) with no gap, overlap, stale geometry or crash (macOS)
- [ ] Same, on Windows 11

### Task 1.9 — Shell commands, events and capability

`shell`/`settings` have no frontend UI yet (Task 1.10/1.12), so every check below is driven from DevTools, following SP3's procedure (`docs/spikes/SP3.md`): open the `shell` webview's DevTools and run `window.__TAURI_INTERNALS__.invoke("<command>", { ... })` / `window.__TAURI_INTERNALS__.invoke("get_snapshot")` in its console. Run once with the default `MultiwebviewHost` and once with `--features host-child-windows` (`ChildWindowHost`) — both build `shell.json`'s two webview labels the same way, but only a real build proves each backend applies it. For each item, record date, platform and result (pass/fail, with notes) before checking it off.

- [ ] `invoke("get_snapshot")` from the `shell` webview's DevTools console succeeds and returns `settings`, `services`, an empty `statuses`, and `sidebarWidth` equal to `64` (macOS, default host)
- [ ] Same, Windows 11
- [ ] Same, macOS with `--features host-child-windows`
- [ ] The same `invoke("get_snapshot")` call from a `svc-<id>` service webview's DevTools console is rejected (no `allow-get-snapshot` permission there) — try both the default host and `--features host-child-windows` (macOS)
- [ ] Same rejection, Windows 11
- [ ] With `config.toml` edited to an invalid file (e.g. a missing required key) before launch, `get_snapshot` returns a `configError` with the file path, the offending key and a reason, and `settings` is `null` (macOS)
- [ ] Same invalid-`config.toml` → `configError` behaviour, Windows 11
- [ ] Calling `invoke("open_settings")` twice in a row leaves exactly one settings window open (not two), and it is focused after the second call (macOS)
- [ ] Same single-window behaviour, Windows 11
- [ ] `add_service`, `update_service`, `remove_service`, `reorder_services` each produce a `services-changed` event observable via `window.__TAURI_INTERNALS__.invoke` + a `listen`-equivalent DevTools snippet (or `open_settings` + DevTools on that window) in both the `shell` and the `settings` webview (macOS)
- [ ] Same cross-webview `services-changed` delivery, Windows 11
- [ ] `select_service` moves the target service's webview into the content area and emits `select-service` (`{ serviceId }`) observable from the `shell` webview only (macOS)
- [ ] Same, Windows 11

### Task 1.10 — Shell store and sidebar

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] Drag-and-drop reorder works in the real shell (macOS/Windows)

### Task 1.12 — Settings window: service CRUD

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] Launch the app (`pnpm tauri dev`), click the sidebar's "+" or gear button, and the settings window opens showing the `#/settings` screen (macOS)
- [ ] Same, Windows 11
- [ ] Adding a URL in the settings window's add form produces a usable mail tab: the service appears in the sidebar, becomes the active tab, and its page loads and can be signed into (macOS)
- [ ] Same, Windows 11

## M2 — Unread

Exit criterion (SPEC §16): N Gmail accounts plus iCloud show live counts while the window is in the background.

- [ ] With N Gmail accounts and iCloud configured at the same time, every service shows a live count while the window is in the background

### Task 2.2 — Runtime capability and agent injection

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off. The agent (`agent/main.ts`) is still a placeholder that only logs on load as of this task (Task 2.4 implements the real agent) — these checks call `report_unread` by hand from DevTools, standing in for the agent.

- [ ] With a real service configured (e.g. Gmail or iCloud) and its webview created, that webview's own DevTools console has `window.__ELUMA__` present right after load, shaped as `{ serviceId, serviceUrl, reportIntervalMs: 30000, reconcileIntervalMs }` (design.md §2.2.6) (macOS)
- [ ] Same `window.__ELUMA__` shape check (Windows 11)
- [ ] From that same service webview's DevTools console, `window.__TAURI_INTERNALS__.invoke('report_unread', { report: { serviceId: '<that service's id>', count: 1, messages: [], recipeId: 'generic', observedAt: Date.now(), iconCandidates: [] } })` resolves, and Rust's log (stderr in dev, or `{data_dir}/logs/`) shows a debug line `accepted unread report` naming that service id (macOS)
- [ ] Same successful invoke and matching debug log line (Windows 11)
- [ ] The same invoke from the **shell** webview's own DevTools console (not a service's) is rejected — `shell` is not a `svc-<id>` webview and holds no runtime capability for `report_unread` (macOS)
- [ ] Same shell-origin rejection (Windows 11)
- [ ] Navigate a service's webview to a different origin (e.g. an outbound link), then repeat the invoke from that webview's DevTools console. It is rejected, since the granted capability's origin pattern no longer matches (macOS; mechanism confirmed by `docs/spikes/SP3.md`'s B2)
- [ ] Same off-origin rejection after navigation (Windows 11)
- [ ] The agent does not run inside an embedded `<iframe>` on the service page — record as not applicable for this task, since the frame guard is Task 2.4's job and the placeholder agent does nothing frame-aware yet

## M3 — Background survival

Exit criterion (SPEC §16): badges still correct after 2 hours minimised, on both platforms — or the Windows story is honestly documented as degraded.

- [ ] Badges match the real inboxes after 2 hours minimised (macOS)
- [ ] Badges match the real inboxes after 2 hours minimised (Windows 11), or the degraded state is documented
- [ ] Badges match the real inboxes after 2 hours minimised (Windows 10), or the degraded state is documented
- [ ] Memory use per service recorded (R7)

### Task 3.1 — macOS App Nap assertion

- [ ] With at least one service configured, run `pmset -g assertions` while Eluma runs (e.g. minimised or backgrounded) and confirm a `PreventUserIdleSystemSleep` (or equivalent `NSActivityUserInitiated`-backed) assertion is listed for the Eluma process, plus the `app_nap: acquired activity assertion` / `app_nap: released activity assertion` log lines appear exactly once each as the last service is added/removed (macOS)

### Task 3.3 — Diagnostics view

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] From the settings window (`#/settings`), the Diagnostics section lists every configured service with its status, last report age, stale count and last stale time, and its "Refresh" button re-fetches successfully via `invoke("get_diagnostics")` (macOS)
- [ ] Same, Windows 11
- [ ] `window.__TAURI_INTERNALS__.invoke("get_diagnostics")` from a `svc-<id>` service webview's DevTools console is rejected (no `allow-get-diagnostics` permission there) (macOS)
- [ ] Same rejection, Windows 11

## M4 — Notifications

Exit criterion (SPEC §16): iCloud fires a native notification with Eluma in the background.

- [ ] iCloud fires a native notification with Eluma in the background

## M5 — Recipes

Exit criterion (SPEC §16): a contributor adds Fastmail by adding one recipe module, without touching Rust.

- [ ] Fastmail is added by adding one recipe module, without touching Rust, and shows a live count
