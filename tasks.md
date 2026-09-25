# Eluma — Task List

> **Based on:** `SPEC.md` v0.3, `design.md` · **Scope:** M0 (setup and spikes) + M1–M5

## Overview

- Total tasks: 49
- Estimated effort: ~116 hours (excluding the multi-hour soak tests in M3)
- Priority: M0 spikes and M3 are highest, because they carry the project's central risks (SPEC §9, §16).
- Each task is sized to one commit (1–4h). Every implementation task includes its unit tests, `pnpm lint`, `pnpm typecheck`, `cargo clippy` and `cargo test`.
- Running the app (`pnpm tauri dev`) is done by the owner. Tasks marked **[manual]** need the owner to run the app.

---

## Phase 0 (M0): Setup and risk spikes

#### Task 0.1: Project scaffold

- [ ] Create a Tauri 2.11.x + React + Vite + TypeScript project with pnpm
- [ ] Enable the `unstable` feature on `tauri`; set the minimum version to 2.11.1
- [ ] Configure strict TypeScript, ESLint, Prettier, Vitest (jsdom) and Testing Library
- [ ] Configure rustfmt and clippy (`-D warnings`)
- [ ] Add scripts: `lint`, `typecheck`, `test`, `build` (shell + agent)
- **Done when:** `pnpm lint && pnpm typecheck && pnpm test && cargo clippy && cargo test` pass on an empty app
- **Depends on:** none
- **Estimate:** 2h

#### Task 0.2: Agent build pipeline

- [ ] Add `vite.agent.config.ts` (library mode, single IIFE) → `src-tauri/agent-dist/agent.js`
- [ ] Add `include_str!` of the agent in Rust, and wire `beforeBuildCommand` / `beforeDevCommand`
- [ ] Add a placeholder `agent/main.ts` that only logs
- **Done when:** `cargo build` fails if the agent was not built and succeeds after `pnpm build`
- **Depends on:** 0.1
- **Estimate:** 1.5h

#### Task 0.3: SP1 — Multiwebview spike [manual]

- [ ] Throwaway branch: a main `Window` with a shell child plus 4 service children via `add_child`
- [ ] Place inactive children offscreen; switch between them; resize the window
- [ ] Record positioning and resizing behaviour on macOS 14+ and Windows 11 (tauri#10420, #11376)
- [ ] Write `docs/spikes/SP1.md` and decide: `MultiwebviewHost` or `ChildWindowHost` as the default
- **Done when:** the decision is recorded with evidence
- **Depends on:** 0.1
- **Estimate:** 3h

#### Task 0.4: SP2 — Data store isolation spike [manual]

- [ ] Two children with different UUIDv5 `data_store_identifier`s (macOS) or `data_directory`s (Windows)
- [ ] Sign in to two different iCloud accounts side by side; restart; confirm both sessions persist
- [ ] Write `docs/spikes/SP2.md`
- **Done when:** two sessions coexist and survive a restart without a crash, or the failure is documented
- **Depends on:** 0.3
- **Estimate:** 2h

#### Task 0.5: SP3 — Remote IPC capability spike [manual]

- [ ] Declare `report_unread` in `build.rs` with `AppManifest::commands`
- [ ] Add a runtime capability (`CapabilityBuilder::remote(origin).webview(label).permission(...)`) and confirm the exact permission identifier
- [ ] Confirm that `invoke` succeeds from the service origin and is rejected from another origin and from another webview label
- [ ] Confirm the `tauri::Webview` parameter yields the calling label
- [ ] Write `docs/spikes/SP3.md`
- **Done when:** all four behaviours are confirmed, or the blocker is documented
- **Depends on:** 0.3
- **Estimate:** 3h

#### Task 0.6: SP4 — WebView2 throttling spike (SPEC §17 Q1) [manual]

- [ ] On Windows, use the browser-argument constant from design §2.2.4 for all webviews
- [ ] A test page that logs a timestamp every 5s and on a `<title>` mutation driven by a timer
- [ ] Minimize the window for 30 minutes; also test the webview while it is offscreen and the window is occluded
- [ ] Record whether timers and observers keep running; write `docs/spikes/SP4.md`
- **Done when:** a clear result is recorded, and it determines whether Windows remains a first-class target (to be reported to the owner)
- **Depends on:** 0.3
- **Estimate:** 3h

#### Task 0.7: Manual checklist skeleton

- [ ] Create `docs/manual-checks.md` with one section per milestone exit criterion (SPEC §16)
- **Done when:** the file exists with M1–M5 sections
- **Depends on:** none
- **Estimate:** 0.5h

---

## Phase 1 (M1): Shell

#### Task 1.1: Paths and error types

- [ ] `paths.rs` (config and data directories via Tauri's path resolver)
- [ ] `error.rs` (`AppError`, serialization `{ kind, message }`)
- **Done when:** unit tests for error serialization pass
- **Depends on:** 0.1
- **Estimate:** 1h

#### Task 1.2: Config model and loading

- [ ] `Config`, `Settings`, `ServiceConfig`, `IconSource` models
- [ ] Validation: every key required, `version == 1`, id format and uniqueness, http(s) URL, profile names
- [ ] `write_initial` for first launch
- [ ] Tests: each missing or invalid key yields a `ConfigError` that names the key
- **Done when:** tests pass; no default is substituted anywhere
- **Depends on:** 1.1
- **Estimate:** 3h

#### Task 1.3: Config edits with `toml_edit`

- [ ] `apply(ConfigEdit)` for add, update, remove, reorder and settings
- [ ] Atomic write (temp file + rename)
- [ ] Tests: comments and formatting survive every edit type; reordering preserves each service's comments
- **Done when:** round-trip tests pass
- **Depends on:** 1.2
- **Estimate:** 3h

#### Task 1.4: State store

- [ ] `State` model (profiles, seen, staleness), atomic debounced save, error on a corrupt file
- **Done when:** tests pass for create, load, save and corrupt-file error
- **Depends on:** 1.1
- **Estimate:** 2h

#### Task 1.5: Profile resolution and backends

- [ ] Key derivation (`default` / `isolated:<id>` / `named:<name>`) and UUIDv5 with a fixed namespace
- [ ] `ProfileBackend` for macOS (`data_store_identifier`, `remove_data_store`) and Windows (`data_directory`, directory removal)
- **Done when:** determinism tests pass; the backends compile on both targets
- **Depends on:** 1.4, 0.4
- **Estimate:** 2.5h

#### Task 1.6: `WebviewHost` trait and default host

- [ ] Trait and `ServiceWebviewSpec`
- [ ] The host chosen in SP1: shell child plus service children, offscreen layout, `activate`, `relayout` on resize
- [ ] Per-webview settings: `background_throttling(Disabled)` on macOS; the browser-argument constant on Windows for every webview
- [ ] Layout math in `layout.rs`, with unit tests
- **Done when:** layout tests pass; the app shows the shell and one hard-coded test service [manual]
- **Depends on:** 0.3, 1.5
- **Estimate:** 4h

#### Task 1.7: Fallback host

- [ ] The other `WebviewHost` implementation behind Cargo feature `host-child-windows`, with geometry sync on `Moved` / `Resized`
- **Done when:** the app builds and runs with the feature on [manual]
- **Depends on:** 1.6
- **Estimate:** 3h

#### Task 1.8: Service lifecycle orchestration

- [ ] Staggered startup in sidebar order (`STARTUP_STAGGER`)
- [ ] Apply each `ConfigEdit` to live webviews (design §2.2.5)
- [ ] Service id slug generation, with tests
- **Done when:** services from `config.toml` open at launch [manual]; slug tests pass
- **Depends on:** 1.3, 1.6
- **Estimate:** 3h

#### Task 1.9: Shell commands, events and capability

- [ ] `get_snapshot`, `add_service`, `update_service`, `remove_service`, `reorder_services`, `select_service`, `update_settings`, `open_settings`, `reload_service`
- [ ] Events `services-changed` and `select-service`; `capabilities/shell.json` scoped to `shell` and `settings`
- [ ] `configError` included in the snapshot when loading failed
- **Done when:** commands are reachable from the shell only
- **Depends on:** 1.8
- **Estimate:** 3h

#### Task 1.10: Shell store and sidebar

- [ ] Typed `ipc/` wrappers; a store based on `useSyncExternalStore`
- [ ] Sidebar: icon list, selection, tooltip, "+" and gear buttons, a config error screen
- [ ] HTML5 drag-and-drop reorder → `reorder_services`
- **Done when:** component tests pass for selection and reorder
- **Depends on:** 1.9
- **Estimate:** 4h

#### Task 1.11: Recipe metadata and generic recipe (shared)

- [ ] `agent/recipes/types.ts` (the full `Recipe` interface from design §2.2.14)
- [ ] `registry.ts` with `matchRecipe`, and a `generic` recipe with metadata only (read logic comes in M2)
- [ ] Metadata stubs for `gmail`, `icloud` and `outlook` (id, displayName, defaultProfile, matches)
- **Done when:** matching tests pass for each recipe URL
- **Depends on:** 0.2
- **Estimate:** 2h

#### Task 1.12: Settings window — service CRUD

- [ ] `#/settings` route in a separate `settings` window
- [ ] Add form (URL → name; profile pre-filled from `matchRecipe(url).defaultProfile`; default, isolated or named)
- [ ] Edit form (name, URL, profile, notifications); delete confirmation, with "delete session data" for isolated profiles
- **Done when:** component tests pass; adding a URL produces a usable mail tab [manual]
- **Depends on:** 1.10, 1.11
- **Estimate:** 4h

#### Task 1.13: Settings window — global settings

- [ ] Form for every `[settings]` key → `update_settings`
- **Done when:** edits persist to `config.toml` with comments intact
- **Depends on:** 1.12
- **Estimate:** 1.5h

#### Task 1.14: Icon resolution

- [ ] Agent side: collect `iconCandidates` in order (apple-touch-icon → largest `rel=icon` → `/favicon.ico`). Until M2 wires the report command, use a temporary path from SP3's working IPC.
- [ ] Rust side: download (no cookies, 5s timeout, 1 MiB cap), normalise to 128×128 PNG, cache, `refresh_icon`, `set_icon_override` (file or URL)
- [ ] Shell: render the PNG through the asset protocol; generated letter icon with a deterministic colour from the id
- **Done when:** candidate-ordering and colour-determinism tests pass; real services show their icons [manual]
- **Depends on:** 1.10, 0.5
- **Estimate:** 4h

#### Task 1.15: M1 exit check [manual]

- [ ] Two iCloud accounts signed in side by side; a URL becomes a usable mail tab
- [ ] Record the result in `docs/manual-checks.md`
- **Done when:** the M1 exit criterion (SPEC §16) passes on macOS; Windows is recorded per SP4
- **Depends on:** 1.1–1.14
- **Estimate:** 1h

---

## Phase 2 (M2): Unread

#### Task 2.1: Report command and validation

- [ ] `UnreadReportDto`, `ValidReport`; `validate.rs` with every rule from design §2.2.6
- [ ] `report_unread(webview: tauri::Webview, …)`; the label must match the service
- **Done when:** tests pass for each rejection rule
- **Depends on:** 0.5, 1.8
- **Estimate:** 3h

#### Task 2.2: Runtime capability and agent injection

- [ ] Add a `svc-<id>` capability per service before creating its webview
- [ ] Injection script `window.__ELUMA__ = {...}; <agent>`
- **Done when:** the agent in a real service can invoke `report_unread` [manual]
- **Depends on:** 2.1
- **Estimate:** 2h

#### Task 2.3: Unread status store and events

- [ ] `ServiceStatus` and `AttentionReason`; transitions from reports and from `on_page_load` origin checks (`OffOrigin`)
- [ ] `status-changed` sent to `shell` only, and only on change
- **Done when:** transition tests pass
- **Depends on:** 2.1
- **Estimate:** 2.5h

#### Task 2.4: Agent core

- [ ] `main.ts`: frame guard, read and delete `__ELUMA__`, origin check, recipe selection
- [ ] `loop.ts`: 500ms debounced watch → read, reconcile with ±20% jitter, 30s re-report
- [ ] `report.ts`; `iconCandidates` on the first report per load only
- **Done when:** fake-timer tests cover debounce, jitter bounds, re-report and the frame guard
- **Depends on:** 1.11, 2.2
- **Estimate:** 3h

#### Task 2.5: Title strategy and generic recipe

- [ ] `titleCount` and `watchTitle` (an observer on `<head>`, so replaced `<title>` elements are caught)
- [ ] Complete the `generic` recipe
- **Done when:** tests pass for count, no count (→ 0) and a replaced title element
- **Depends on:** 2.4
- **Estimate:** 2h

#### Task 2.6: Fetch strategy and `SameOriginFetch`

- [ ] `SameOriginFetch` (throws on cross-origin); `fetchText`
- **Done when:** tests prove cross-origin rejection
- **Depends on:** 2.4
- **Estimate:** 1.5h

#### Task 2.7: Gmail recipe

- [ ] Feed path from the service URL; Atom parsing (`fullcount`, entries → `MessageRef` with hashed id and `#all/<id>` deep link)
- [ ] First-use validation; a sticky fallback to title on non-200, redirect or malformed body; `null` on a signed-out state
- [ ] Test fixtures: valid feed, empty feed, malformed body, login redirect
- **Done when:** fixture tests pass; real Gmail shows a live count [manual]
- **Depends on:** 2.5, 2.6
- **Estimate:** 4h

#### Task 2.8: iCloud and Outlook recipes [manual observation]

- [ ] Observe the real iCloud and Outlook titles when signed in and signed out; record the findings in `docs/recipes-notes.md`
- [ ] Implement both recipes with title strategy and signed-out detection derived from those observations
- **Done when:** tests based on the recorded titles pass; real services show counts [manual]
- **Depends on:** 2.5
- **Estimate:** 3h

#### Task 2.9: Selector strategy

- [ ] `selectorCount` (`text` / `rows`) and `watchSelector`
- **Done when:** tests pass for both modes and for DOM changes
- **Depends on:** 2.4
- **Estimate:** 1.5h

#### Task 2.10: Sidebar badges

- [ ] `Badge` for Loading, Ok(0), Ok(n) with a "999+" cap, Stale and NeedsAttention; respects `badge_sidebar`
- **Done when:** component tests pass for every status
- **Depends on:** 2.3, 1.10
- **Estimate:** 2h

#### Task 2.11: SP7 — Load-failure behaviour [manual]

- [ ] Point a service at an unreachable host; observe whether the agent runs on the error page
- [ ] Record the result in `docs/spikes/SP7.md` and adjust the status handling if it is needed
- **Done when:** the behaviour is documented and handled
- **Depends on:** 2.3, 2.4
- **Estimate:** 1.5h

#### Task 2.12: M2 exit check [manual]

- [ ] N Gmail accounts plus iCloud show live counts while the window is in the background
- **Done when:** recorded in `docs/manual-checks.md`
- **Depends on:** 2.1–2.11
- **Estimate:** 1h

---

## Phase 3 (M3): Background survival

#### Task 3.1: macOS App Nap assertion

- [ ] `platform/app_nap.rs` with `objc2-foundation`; hold the token while at least one service exists
- [ ] Use the activity option confirmed by the owner (design §10, open point 1)
- **Done when:** the token is held and released correctly (a log line)
- **Depends on:** 1.8
- **Estimate:** 1.5h

#### Task 3.2: Liveness monitor

- [ ] A 30s ticker; `Stale` after 2 missed reports plus a 5s grace; `OffOrigin` is exempt
- [ ] Recovery: reload, then destroy and recreate after 2 more ticks; staleness counters in `state.json`
- [ ] Tests with an injected clock
- **Done when:** state machine tests pass
- **Depends on:** 2.3, 1.4
- **Estimate:** 3h

#### Task 3.3: Diagnostics view

- [ ] `get_diagnostics`; a settings panel showing staleness history, last report age and status
- **Done when:** component test passes
- **Depends on:** 3.2, 1.12
- **Estimate:** 2h

#### Task 3.4: M3 soak test [manual]

- [ ] Minimize for 2 hours on macOS and on Windows; compare the badges with the real inboxes
- [ ] Record memory use per service (R7) at the same time
- [ ] If Windows fails, document the degraded state honestly (SPEC §16 M3)
- **Done when:** the result is recorded in `docs/manual-checks.md`
- **Depends on:** 3.1, 3.2
- **Estimate:** 1h active (plus 2h waiting per platform)

---

## Phase 4 (M4): Notifications

#### Task 4.1: SP5 — Notification click crate [manual]

- [ ] Evaluate `user-notify` (and alternatives if it is unsuitable) for click callbacks on macOS and Windows
- [ ] Write `docs/spikes/SP5.md`, then decide on click-capable or display-only
- **Done when:** the decision is recorded and reported to the owner
- **Depends on:** 0.1
- **Estimate:** 3h

#### Task 4.2: SP6 — Page notification suppression [manual]

- [ ] Check whether `window.Notification` exists in WKWebView on remote pages; check the Service Worker `showNotification` path
- [ ] Write `docs/spikes/SP6.md`
- **Done when:** documented
- **Depends on:** 2.4
- **Estimate:** 1.5h

#### Task 4.3: Notification stub and WebView2 permission handler

- [ ] Agent `notification-stub.ts` (installed before page scripts)
- [ ] Windows `platform/webview2.rs`: deny `NOTIFICATIONS` in `PermissionRequested`
- **Done when:** stub tests pass; Gmail shows no page-originated notification [manual]
- **Depends on:** 4.2
- **Estimate:** 2.5h

#### Task 4.4: Seen ring and diff engine

- [ ] `seen.rs` (500 per service, persisted); `diff.rs` (Seed, Nothing, NewMessages, CountIncrease)
- **Done when:** tests cover cold start, `None` not resetting the baseline, new ids, count increase and ring eviction
- **Depends on:** 1.4, 2.1
- **Estimate:** 3h

#### Task 4.5: Dispatcher, batching and text

- [ ] Global and per-service toggles, batching threshold, text formats in `notify/text.rs`
- [ ] `NotificationSink` trait; baseline sink on `tauri-plugin-notification`
- **Done when:** dispatcher tests (with a fake sink) pass
- **Depends on:** 4.4
- **Estimate:** 3h

#### Task 4.6: Activation (only if SP5 found a click-capable crate)

- [ ] The click-capable sink: focus the window, `host.activate`, emit `select-service`, navigate to the link
- **Done when:** clicking a Gmail notification opens the message [manual]. If SP5 is negative, this task is closed as not applicable.
- **Depends on:** 4.1, 4.5
- **Estimate:** 3h

#### Task 4.7: M4 exit check [manual]

- [ ] iCloud fires a native notification with Eluma in the background
- **Done when:** recorded in `docs/manual-checks.md`
- **Depends on:** 4.3–4.6
- **Estimate:** 1h

---

## Phase 5 (M5): Recipes

#### Task 5.1: Recipe panel

- [ ] Settings panel showing `recipe.displayName` and `describe()` (strategy and what it reads) for each service (SPEC §14)
- **Done when:** component test passes
- **Depends on:** 2.7, 1.12
- **Estimate:** 1.5h

#### Task 5.2: Contribution guide

- [ ] `docs/recipes.md`: the interface, rules (read-only, same-origin, no `eval`), fixture-based tests, registration, the review checklist
- **Done when:** a new contributor can follow it without reading the Rust code
- **Depends on:** 2.7
- **Estimate:** 2h

#### Task 5.3: Fastmail recipe by the guide (exit rehearsal)

- [ ] Add a Fastmail recipe by following `docs/recipes.md` only, touching no Rust
- **Done when:** Fastmail shows a live count [manual]; the M5 exit criterion is recorded
- **Depends on:** 5.2
- **Estimate:** 2h

#### Task 5.4: Additional recipes (Zoho, Roundcube)

- [ ] One recipe per service with tests, following the guide
- **Done when:** tests pass; each works on a real account [manual]
- **Depends on:** 5.2
- **Estimate:** 3h

---

## Implementation order

1. **Phase 0 comes first.** SP1–SP4 can run in parallel after 0.1. SP4's result is reported to the owner before M1 continues on Windows.
2. **M1:**
   - 1.1 → 1.2 → 1.3, and 1.4 → 1.5 → 1.6 → 1.8 → 1.9 → 1.10 → 1.12 → 1.13.
   - 1.7, 1.11 and 1.14 run in parallel where their dependencies allow.
3. **M2:** 2.1 → 2.2 → 2.3 / 2.4. Then 2.5, 2.6 and 2.9 in parallel → 2.7 / 2.8 → 2.10 → 2.11 → 2.12.
4. **M3:** 3.1 and 3.2 in parallel → 3.3 → 3.4.
5. **M4:** 4.1 and 4.2 first (spikes) → 4.3, 4.4 → 4.5 → 4.6 → 4.7.
6. **M5:** 5.1 and 5.2 in parallel → 5.3 → 5.4.

**Critical path:** 0.1 → 0.3 → 1.6 → 1.8 → 2.1 → 2.2 → 2.4 → 2.7 → 3.2 → 3.4.

## Risks and mitigations

- **SP1 fails (multiwebview unusable):** switch the default to `ChildWindowHost` (task 1.7). The trait isolates the change.
- **SP3 fails (remote IPC blocked):** stop and consult the owner. The unread design depends on it, and there is no verified alternative transport.
- **SP4 negative (WebView2 throttles anyway):** report to the owner. The Windows target is either kept as degraded (the liveness monitor makes failures visible) or dropped.
- **SP5 negative:** notifications become display-only (SPEC §10). Task 4.6 is closed.
- **Gmail feed breaks:** the title fallback is built in (task 2.7).

## Notes

- Each task is completed in one commit.
- No fallback values in configuration handling; missing settings are errors.
- Do not change working behaviour outside the task's scope.
- Ask before implementing whenever something is unclear.

## Getting started

1. Implement the tasks in the order above.
2. Mark a task in progress when starting it and completed when it is done.
3. Report any spike result that changes the plan before continuing.
