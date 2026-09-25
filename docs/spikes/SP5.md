# SP5 — Notification click crate spike (design §6.3, SPEC §10)

## Purpose

Determine which crate, if any, can deliver a **notification click** event on both
macOS 14+ and Windows 10/11, so design.md §2.2.9's `sink.rs` can add a click-capable
`NotificationSink` (focus the window, `host.activate(service)`, navigate to the
link) instead of leaving Eluma's notifications display-only. This is SPEC §10 /
design §6.3's SP5, gating Task 4.6 (tasks.md).

`tauri-plugin-notification`, the plugin Eluma already plans to use as the baseline
sink, is confirmed (design §2.2.9, SPEC §10, R8) to deliver **no click event at all**
on desktop — tracked upstream as
[plugins-workspace#2150](https://github.com/tauri-apps/plugins-workspace/issues/2150),
still open. That issue names `user-notify` as a possible replacement; this spike
validates that claim against the crate's actual source rather than the issue
thread's assumption, and evaluates `notify-rust` as a second candidate.

## Question (design §6.3, SP5)

> Which crate delivers notification clicks on macOS and Windows (`user-notify`
> first)?

## Environment

| Field                               | macOS                                                                                                                                           | Windows                               |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| OS version                          | 未実施 (owner) — target macOS 14+                                                                                                               | 未実施 (owner) — target Windows 10/11 |
| CPU / architecture                  | 未実施 (owner)                                                                                                                                  | 未実施 (owner)                        |
| Display scale factor                | 未実施 (owner)                                                                                                                                  | 未実施 (owner)                        |
| `tauri` version (Cargo.lock)        | 2.11.6                                                                                                                                          | 2.11.6                                |
| `tauri-build` version (Cargo.lock)  | 2.6.3                                                                                                                                           | 2.6.3                                 |
| `wry` version (Cargo.lock)          | 0.55.1                                                                                                                                          | 0.55.1                                |
| `tao` version (Cargo.lock)          | 0.35.3                                                                                                                                          | 0.35.3                                |
| `user-notify` version               | 0.4.2 (feature `sp5-user-notify`)                                                                                                               | 0.4.2 (feature `sp5-user-notify`)     |
| `notify-rust` version               | 4.18.0, `preview-macos-un` (feature `sp5-notify-rust`)                                                                                          | 4.18.0 (feature `sp5-notify-rust`)    |
| `tauri-plugin-notification` version | 2.4.0 (feature `sp5-baseline`)                                                                                                                  | 2.4.0 (feature `sp5-baseline`)        |
| WebView2 Runtime version            | n/a                                                                                                                                             | 未実施 (owner)                        |
| App bundle id / AUMID used          | 未実施 (owner) — see SPIKE-SP5.md                                                                                                               | 未実施 (owner) — see SPIKE-SP5.md     |
| Signing used                        | 未実施 (owner) — unsigned / ad-hoc / Developer ID, see SPIKE-SP5.md                                                                             | n/a                                   |
| Branch / commit                     | `spike/sp5-notifications` @ `ae7b128` (harness, `SPIKE-SP5.md` and this document are committed there; check out that commit to reproduce S1–S7) | same                                  |

## Candidates: what the primary sources say

Read directly from each crate's published source (not the crates.io description
alone) on 2026-09-25. Versions are each crate's current `max_stable_version` on
crates.io as of that date.

### `user-notify` 0.4.2 (github.com/Simon-Laux/user-notify)

- **License/maintainer:** LGPL-3.0-or-later, single maintainer (Simon Laux) plus one
  other contributor. Built specifically to replace Tauri's own notification API for
  the Delta Chat desktop (Tauri) port — its README explicitly cites
  plugins-workspace#2150 as the gap it fills ("Basic features like reacting to
  clicks on notifications were missing in the rust api").
- **macOS** (`src/platform_impl/mac_os/`): uses `UNUserNotificationCenter`
  (`objc2-user-notifications`). `delegate.rs` implements
  `UNUserNotificationCenterDelegate::didReceiveNotificationResponse`, which the
  delegate type declares `#[thread_kind = MainThreadOnly]` — so the OS callback
  itself fires on the **main thread**. That delegate method sends the response over
  a `tokio::sync::mpsc` channel to a dedicated `listener_loop` background thread
  (`manager.rs`) spawned by the manager; **that** thread is what calls the
  `handler_callback` a consumer registers via `NotificationManager::register`. Net
  effect: the click callback this harness receives runs on a crate-owned background
  thread, not the OS's own main-thread delegate call and not Tauri's main thread.
  Click support: **implemented**.
- **Windows** (`src/platform_impl/windows.rs`): uses
  `Windows.UI.Notifications.ToastNotification`
  (`windows`/`windows-collections` crates, WinRT). `register_event_listeners`
  registers `toast.Activated(&activation_handler)` — a `TypedEventHandler` that
  calls the same `handler_callback` directly from whatever thread WinRT invokes the
  COM event callback on. The crate does **no thread marshalling of its own** on
  Windows (contrast with macOS's `listener_loop`). Click support: **implemented**,
  contrary to what this task's brief flagged as a thing to double check ("record
  the alternate evaluation conditions if `user-notify`'s Windows is confirmed
  unimplemented") — reading the source directly shows Windows click activation
  _is_ implemented; the crate's own README's Windows section just says "TODO:
  instructions for windows", which is a documentation gap, not a missing feature.
  The owner's manual run is still what actually confirms this in practice.
- **Platform prerequisites** (`get_notification_manager` in `src/lib.rs`):
  - macOS: falls back to a log-only mock manager if
    `NSBundle.mainBundle().bundleIdentifier()` is `None` (true for an unbundled
    `cargo run`/`tauri dev` binary). A signed `.app` bundle with a bundle id is
    required for real notifications — the crate's own README says as much
    ("on macOS this only works inside an app package with a 'Bundle ID', also you
    need an Apple developer account to sign it").
  - Windows: falls back to the same mock if
    `ToastNotificationManager::CreateToastNotifierWithId(app_id)` fails, which
    happens when `app_id` (an AUMID) isn't registered by an installed Start Menu
    shortcut.
- **Payload/identifying data:** `NotificationBuilder::set_user_info(HashMap<String, String>)`
  round-trips through the OS notification store on both platforms and is handed
  back in `NotificationResponse.user_info` on click — used directly by this
  harness to carry `SpikePayload`'s JSON.

### `notify-rust` 4.18.0 (github.com/hoodie/notify-rust)

- **License/maintainer:** MIT OR Apache-2.0, established crate (14.8M+ total
  downloads on crates.io), maintained by Hendrik Sollich since long before this
  spike. Its own README (linked from `user-notify`'s) explicitly says: "If this
  crate is not for you, then you may like user-notify... but has less feature
  support on macOS" — i.e. the two crates' own authors already frame `notify-rust`
  as the more conservative, `user-notify` as the more featureful, option.
- **macOS** (`src/macos/`): **two** backends selected by a Cargo feature:
  - Default (no `preview-macos-un`): `mac-notification-sys` wrapping the legacy
    `NSUserNotificationCenter` API. Its own doc comment in
    `src/macos/nsusernotifications.rs` says: _"This stack is deprecated on macOS
    14+, but still works."_ `NotificationHandle::wait_for_response` **is**
    implemented here too, but its doc comment adds: _"Requires the main run loop
    to be running. `NSUserNotificationCenter` delivers delegate callbacks on the
    main thread; if nothing is pumping the main run loop this call will block
    indefinitely."_
  - `preview-macos-un` (opt-in, what this harness's `sp5-notify-rust` Cargo
    feature enables): `mac-usernotifications` wrapping `UNUserNotificationCenter` —
    the same modern API family `user-notify` uses. `wait_for_response` here calls
    `mac_usernotifications::block_on_current(self.inner.response())`, i.e. also
    blocks the calling thread, but through a different (non-run-loop-dependent)
    mechanism.
  - This harness uses `preview-macos-un` (per the CodeRabbit plan for this task)
    since it's the non-deprecated path and shares click-delivery infrastructure
    with `user-notify`, giving a cleaner macOS-14+-forward comparison. The default
    legacy backend was not separately built/tested here; if the owner wants that
    data point too (e.g. because `preview-macos-un` is labelled "preview"/unstable
    upstream), it needs a separate build with the feature disabled — note that as
    a gap rather than assuming it behaves like the modern backend.
  - Click support (with `preview-macos-un`): **implemented**.
- **Windows** (`src/windows.rs`): unconditionally depends on the
  `tauri-winrt-notification` crate (`winrt-notification` in `Cargo.toml`, resolved
  to `tauri-winrt-notification` 0.7.3), not the raw `windows` crate directly, and
  not affected by the `preview-macos-un` feature. `show_notification` calls
  `toast.on_activated(...)` / `.on_dismissed(...)`, both forwarding into a
  `std::sync::mpsc` channel returned as part of the `NotificationHandle`. Click
  support: **implemented**.
- **API shape — the key practical difference from `user-notify`:** both platforms
  expose the _same_ blocking model: `Notification::show()` returns a
  `NotificationHandle`, and `handle.wait_for_response(handler)` **blocks the
  calling thread** until the user acts (or the process's callback delivers a
  result). There is no "register a persistent handler once" API like
  `user-notify`'s `NotificationManager::register` — each notification needs its
  own thread blocked in `wait_for_response` (or `wait_for_action`) to observe its
  click. This harness spawns one dedicated OS thread per notification to do that.
- **Platform prerequisites:** the same macOS bundle-id/signing requirement applies
  to the `preview-macos-un` backend (it also drives `UNUserNotificationCenter`).
  On Windows, `tauri-winrt-notification`'s `Toast::new(app_id)` needs a real AUMID
  the same way `user-notify` does; unlike `user-notify`, `notify-rust` does **not**
  fall back to a mock — it defaults to `Toast::POWERSHELL_APP_ID` when no
  `app_id` is set on the `Notification` (this harness does not currently call
  `.app_id(...)`; see SPIKE-SP5.md's Windows section), which means an unconfigured
  run will visibly show under PowerShell's name/icon rather than failing silently.

### `tauri-plugin-notification` 2.4.0 (baseline, for comparison)

- Its own `Cargo.toml` metadata: `windows = { level = "full", notes = "Only works
for installed apps. Shows powershell name & icon in development." }`,
  `macos = { level = "full", notes = "" }` — "full" here means notification
  _display_, not click delivery; design.md §2.2.9 / SPEC.md §10 already established
  (via plugins-workspace#2150) that no click event reaches Rust code on desktop with
  this plugin. This spike does not re-litigate that; it's included in the harness
  purely so the owner can see all three notifications side by side under the same
  conditions.
- On Windows, checking its dependency tree during this spike's `cargo check`
  confirms it uses `tauri-winrt-notification` internally too (the same crate
  `notify-rust` wraps) — so the baseline and the `notify-rust` candidate share the
  same underlying Windows toast mechanism; the difference S1–S7 is actually testing
  for `notify-rust` vs. baseline is purely "does this harness's own `on_activated`
  wiring work", not a difference in the underlying Windows API being used.

## Decision criteria

Per design §6.3 / SPEC §10 / tasks.md's M4 risk note ("SP5 negative: notifications
become display-only"):

- **Click-capable**, and a candidate is adopted for Task 4.2/4.6, only if a single
  crate delivers a click event with the correct payload **on both** macOS 14+ and
  Windows 10/11, in the scenarios marked **required** below (S2, S3, S6, S7) — not
  merely display, not merely a mocked/dev-mode success log line.
- **Display-only**: if no single crate clears the required scenarios on **both**
  OSes, notifications stay display-only per SPEC §10, and Task 4.6 is closed as not
  applicable (tasks.md).
- **Mixed candidates** (e.g. `user-notify` works on macOS but only `notify-rust`
  works on Windows, or vice versa): this does **not** meet the click-capable bar as
  written (a _single_ crate's condition is not satisfied) — flag this explicitly and
  ask the owner rather than deciding unilaterally to run two different sink
  implementations per OS. `docs/spikes/SP5.md`'s Decision section below is where that
  question, if it comes up, gets recorded.

## Implementation summary (the harness)

Full detail in `SPIKE-SP5.md` at the worktree root (`spike/sp5-notifications`,
uncommitted — see Environment table above). Summary:

- Three Cargo features on `eluma` (`src-tauri/Cargo.toml`), each optional and off
  by default, independently buildable: `sp5-user-notify`, `sp5-notify-rust`,
  `sp5-baseline`.
- `src-tauri/src/spike/common.rs`: a `SpikePayload` (JSON: `candidate`, `service`,
  `message_id`, `sent_at_ms`) standing in for design §2.2.9's
  `OutgoingNotification`; `spike_log` (stderr + `{app_data_dir}/logs/spike-sp5.log`,
  same pattern as the SP4 harness's `spike_log`); `activate_main_window`, which
  logs the callback's OS thread name/id, then calls `AppHandle::run_on_main_thread`
  and, on the main thread, `show()`, `unminimize()`, `set_focus()` on the `"main"`
  window.
- `src-tauri/src/spike/sp5_{user_notify,notify_rust,baseline}.rs`: one module per
  candidate, each with a `setup(&App)` called from `eluma_lib::run`'s `.setup()`
  closure (only when its feature is enabled). Each candidate, 5 seconds after
  launch, sends one notification carrying a `SpikePayload`; the two click-capable
  ones call `activate_main_window` when clicked.
- Verified so far (this agent, compile-only, macOS host):
  `cargo clippy --all-targets` clean (zero warnings; the workspace denies
  `warnings` and `clippy::all`) for the base app and for every one of the 3
  features individually, every pair, and all three together;
  `cargo check --target x86_64-pc-windows-msvc` clean for the same 5 combinations;
  `cargo test` 45/45 passing per feature; `pnpm build:agent` unaffected (TS-only).
  **No app or bundle was run** — that's this spike's remaining, owner-only step.

## Procedure

Scenarios S1–S7, run **per candidate** (`sp5-user-notify`, then `sp5-notify-rust`;
`sp5-baseline` is display-only and only needs S1) and **per OS**, using the
packaged-build instructions in `SPIKE-SP5.md` (dev mode is explicitly _not_
sufficient for macOS or Windows — see that file's caveats on bundle id / AUMID
fallbacks to a mock notifier). Do not generalize a result from one OS to the other;
if only one OS was run, say so explicitly in each Actual cell.

- **S1 — Notification appears at all** _(reference)_. Confirm the notification
  shows with the expected app name/icon (not PowerShell's, on Windows — see
  `SPIKE-SP5.md`'s AUMID note) and body text containing the `SpikePayload` JSON.
- **S2 — Click while app is foregrounded and window visible** _(required)_. Click
  the notification; confirm `spike-sp5.log` shows the click received, the correct
  payload, and the activation sequence (`show`/`unminimize`/`set_focus`) running on
  the main thread.
- **S3 — Click while window is minimized** _(required)_. Minimize the main window,
  wait for the notification, click it; confirm the window un-minimizes and
  focuses. This is the actual use case design §2.2.9 exists for.
- **S4 — Click after the app has been backgrounded for a while (e.g. an hour)**
  _(reference)_. Checks whether a long-lived registration (macOS's `listener_loop`
  thread; Windows' registered `Activated` handler) survives, vs. e.g. being torn
  down by app-nap-style suspension. Related to SP4's Windows throttling findings —
  if SP4 is inconclusive on whether Windows keeps background work alive, note that
  dependency explicitly rather than treating S4 as an independent result.
  On Windows specifically: confirm which thread `wait_for_response`
  (`notify-rust`) is still blocked on, and whether the handle/registration
  outlives whatever moved the notification into the Action Center.
  On macOS: also note SP1's webview-host/focus observations if anything about
  multiwebview host configuration seems to interact with which window receives
  focus after activation.
- **S5 — Cold activation: click a notification from a previous session after the
  app was fully quit** _(reference)_. Record **only** whether cold activation
  happens at all (app launches / a running instance receives the click) — not a
  full pass/fail, since this exercises a different code path (`user-notify`'s
  `notification_protocol` deep-link parameter, currently `None` in this harness,
  and Windows' persisted-toast-history replay in `NotificationManagerWindows::register`)
  that isn't fully wired up here. If cold activation doesn't work with
  `notification_protocol: None`, that's expected, not a failure of the candidate.
- **S6 — Correct payload on click** _(required)_. Confirm the exact `SpikePayload`
  JSON sent is the one logged on click (not a stale/previous notification's
  payload, not empty/missing `user_info`).
- **S7 — `show`/`unminimize`/`set_focus` all actually take effect** _(required)_.
  From a minimized (or other-space/virtual-desktop, on macOS) state, confirm the
  window becomes visible, un-minimized, and focused/frontmost — not just that the
  calls returned `Ok(())`.

Build/run commands (POSIX and PowerShell), packaging, signing, and AUMID setup are
all in `SPIKE-SP5.md` — this file only defines the scenarios, not the mechanics of
running them.

## Judgment rule

Per candidate, per OS: **Click-capable** if S2, S3, S6, S7 all pass; **Display-only
for that OS** if any of those four fail or could not be produced (e.g. falls back to
the crate's mock manager). S1, S4, S5 are recorded but do not by themselves flip the
verdict.

## Results

| Candidate       | OS      | S1 (ref)       | S2 (req)             | S3 (req)       | S4 (ref)       | S5 (ref)       | S6 (req)       | S7 (req)       | Verdict                                              |
| --------------- | ------- | -------------- | -------------------- | -------------- | -------------- | -------------- | -------------- | -------------- | ---------------------------------------------------- |
| `user-notify`   | macOS   | 未実施 (owner) | 未実施 (owner)       | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner)                                       |
| `user-notify`   | Windows | 未実施 (owner) | 未実施 (owner)       | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner)                                       |
| `notify-rust`   | macOS   | 未実施 (owner) | 未実施 (owner)       | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner)                                       |
| `notify-rust`   | Windows | 未実施 (owner) | 未実施 (owner)       | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner) | 未実施 (owner)                                       |
| baseline plugin | macOS   | 未実施 (owner) | n/a — no click event | n/a            | n/a            | n/a            | n/a            | n/a            | display-only (by design, per plugins-workspace#2150) |
| baseline plugin | Windows | 未実施 (owner) | n/a — no click event | n/a            | n/a            | n/a            | n/a            | n/a            | display-only (by design, per plugins-workspace#2150) |

ACTUAL/VERDICT cells above are for the owner to fill in from real runs — nothing in
this table is inferred or guessed from source-reading alone; the "what the primary
sources say" section is what's confirmed without running anything, and it is not a
substitute for these results.

## Log excerpts

_(Owner: paste representative `spike-sp5.log` excerpts per candidate/OS/scenario
here — the click-received line with thread info, the main-thread activation line,
and anything unexpected, e.g. a fallback-to-mock message.)_

- `user-notify` / macOS: 未実施 (owner)
- `user-notify` / Windows: 未実施 (owner)
- `notify-rust` / macOS: 未実施 (owner)
- `notify-rust` / Windows: 未実施 (owner)

## Impact on Task 4.2 / Task 4.6

- **If click-capable** (a single crate clears S2/S3/S6/S7 on both OSes): Task 4.2's
  `NotificationSink` trait (design §2.2.9) gets a second implementation using that
  crate and its confirmed callback→main-thread handoff pattern (documented above
  and in the relevant `src-tauri/src/spike/sp5_*.rs` module); Task 4.6 implements
  the real activation sequence (focus, `host.activate(service)`, emit
  `select-service`, navigate to the link) on top of that sink, reusing this
  spike's `run_on_main_thread` pattern rather than re-deriving it.
- **If display-only**: Task 4.6 is closed as not applicable (tasks.md's existing
  wording already covers this — "If SP5 is negative, this task is closed as not
  applicable"). Record here, per OS, exactly which required scenario(s) failed and
  for which candidate(s), so a future revisit (e.g. after a crate update) knows
  what specifically needs to change rather than re-running everything from
  scratch.
- **If mixed** (different crates needed per OS): per the decision criteria above,
  this is not treated as click-capable without the owner's explicit sign-off, since
  it doubles the sink implementations Task 4.2 would need to maintain. Ask before
  Task 4.2's sink configuration is finalized.
- M4's exit condition (tasks.md, Task 4.7 — "iCloud fires a native notification
  with Eluma in the background") is unaffected either way; it exercises the
  baseline display path regardless of this decision.

## Decision

**Pending — owner to fill in**, based on the Results table above:

- Click-capable or display-only (SPEC §10): _(fill in)_
- If click-capable: adopted crate + version, Cargo feature to carry forward into
  Task 4.2 (`user-notify` vs. `notify-rust`; if `notify-rust`, confirm whether
  `preview-macos-un` should ship given it's an upstream "preview" feature — flag
  that stability question to the owner rather than assuming it's fine for
  production): _(fill in)_
- macOS: bundle id / signing approach to carry forward for Task 4.2's real
  packaging (unsigned won't work per `user-notify`'s README — see SPIKE-SP5.md):
  _(fill in)_
- Windows: AUMID strategy (this harness hardcodes `com.goofmint.eluma`; confirm
  whether Tauri's NSIS/MSI bundler's auto-registered AUMID matches, or whether an
  explicit AUMID + Start Menu shortcut registration step needs to be added to the
  installer config): _(fill in)_
- Thread-handoff pattern to reuse in Task 4.2/4.6's real sink
  (`run_on_main_thread`, as implemented in `common::activate_main_window`), or
  a different approach if the owner found problems with it during S2/S3/S7:
  _(fill in)_
- Any follow-up needed for S4/S5 (reference scenarios) that turned up something
  worth investigating further: _(fill in)_

## Constraints and notes for the owner

- Dev mode (`pnpm tauri dev`) cannot produce a valid result for the required
  scenarios S2, S3, S6 and S7 (S1 is reference only). The two crates degrade
  differently: `user-notify` falls back to a mock/no-op manager without a
  bundle id (macOS) or a registered AUMID (Windows), so nothing is shown;
  `notify-rust` on Windows shows the toast under `Toast::POWERSHELL_APP_ID`, so
  a notification appears but it is attributed to PowerShell and does not
  exercise the packaged app's own AUMID or its click activation. Validate
  S2/S3/S6/S7 only with the packaged builds described in `SPIKE-SP5.md`.
- This spike does not implement `NotificationManager::first_time_ask_for_notification_permission`
  (`user-notify`) or any equivalent onboarding prompt — if macOS's permission
  prompt doesn't appear on first launch of a signed build, notifications will
  silently not show; record that as a distinct finding, not conflated with a
  candidate's click-delivery capability.
- `notify-rust`'s Windows path does not currently get `.app_id(...)` set by this
  harness, so it will show under PowerShell's identity unless the owner adds that
  call before testing S1 on Windows (noted in SPIKE-SP5.md).
- Running all three `sp5-*` features in one build (verified to compile) is
  possible but not recommended for the manual scenarios themselves, since three
  notifications firing 5 seconds apart makes it easy to click/attribute the wrong
  one — run one candidate feature per install for S2–S7.
- If a candidate's debug/packaged build could not even be produced (e.g. signing
  failed, installer failed), record that here as a blocker with symptom,
  reproduction steps, and next steps, instead of leaving Results blank with no
  explanation.

## Risks and evidence

- Both candidate crates' Windows click support was confirmed by reading
  `src/platform_impl/windows.rs` (`user-notify`) and `src/windows.rs`
  (`notify-rust`) directly — not from either crate's README, which for
  `user-notify` is explicitly incomplete on Windows ("TODO: instructions for
  windows"). Source can drift from what's actually exercised in CI; neither
  crate's repository showed a Windows CI job in its workflow file at the versions
  pinned here (only worth flagging, not itself disqualifying — the manual run is
  what actually matters).
- `notify-rust`'s `preview-macos-un` feature is explicitly named "preview" by its
  own maintainer and is a relatively recent addition (see its Cargo feature name
  and the commented-out `// "preview-macos-un"` line in `notify-rust`'s own
  `Cargo.toml` default-features list) — treat any macOS `notify-rust` result here
  as validating a not-yet-fully-stabilized upstream API surface, which is a
  maintenance risk distinct from whether it technically works today.
- `user-notify` is a young, small-team crate (LGPL-3.0-or-later — note the license
  differs from `notify-rust`'s MIT/Apache-2.0 dual license and from this project's
  own licensing; confirm that's acceptable before adopting it, since it wasn't
  checked as part of this spike).
- Both crates' Windows AUMID/mock-fallback behavior and macOS bundle-id/mock-
  fallback behavior mean a naive dev-mode smoke test can silently "pass" (log
  lines look fine) while never having touched the real OS notification API at
  all — this is the single most likely way to produce a false-positive result
  here; the packaged-build requirement in SPIKE-SP5.md exists specifically to
  avoid that.
