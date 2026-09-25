# SP5 spike harness — build/run guide

Branch: `spike/sp5-notifications` (based on `main`). **Throwaway**: this file, the
Cargo features it describes, and everything under `src-tauri/src/spike/` exist only
here, to let the owner run the manual click-scenario checks (S1–S7) in
`docs/spikes/SP5.md`. None of it is wired into the production app, and this branch is
never merged (tasks.md Task 4.1). The permanent record of the decision goes in
`docs/spikes/SP5.md` on `main`.

This agent (working only inside this worktree) verified **compilation only** —
`cargo clippy --all-targets` on macOS and `cargo check --target x86_64-pc-windows-msvc`
for every feature combination below all pass with zero warnings (workspace lints deny
all warnings). It did **not** run `pnpm tauri dev`, `tauri build`, or the app itself —
that's the owner's job on real macOS 14+ and Windows 10/11 hardware, per
`docs/spikes/SP5.md`'s procedure.

## What's in the harness

`src-tauri/src/spike/`:

- `common.rs` — `SpikePayload` (a fake `{candidate, service, message_id, sent_at_ms}`
  JSON payload standing in for design.md §2.2.9's `OutgoingNotification`), `spike_log`
  (stderr + `{app_data_dir}/logs/spike-sp5.log`, mirroring the SP4 harness's
  `spike_log`), and `activate_main_window` (logs which thread the click callback fired
  on, then explicitly hands off to Tauri's main thread via
  `AppHandle::run_on_main_thread` before calling `show()`, `unminimize()`,
  `set_focus()` on the `"main"` window).
- `sp5_baseline.rs` — `tauri-plugin-notification` 2.4.0. Display-only; never calls
  `activate_main_window` (nothing to react to — see plugins-workspace#2150).
- `sp5_notify_rust.rs` — `notify-rust` 4.18.0 (macOS built with its
  `preview-macos-un` feature; Windows unaffected by that feature — see the module's
  doc comment for what was read from the crate's own source).
- `sp5_user_notify.rs` — `user-notify` 0.4.2 (see its module doc comment for the
  exact source lines read to confirm macOS/Windows click support and how each
  platform hands the click to a callback).

Each candidate: from `setup`, waits 5s, sends one notification carrying a
`SpikePayload`, and on click logs the payload + timestamp and activates the main
window. `docs/spikes/SP5.md` records what "activates" should look like per S1–S7.

## Cargo features

Declared in `src-tauri/Cargo.toml`, all optional, all off by default:

| Feature           | Dependency                                                                             | Notes                                                                                                  |
| ----------------- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `sp5-user-notify` | `user-notify = "0.4.2"`                                                                | Async API; `send_notification` runs on a spike-spawned OS thread via `tauri::async_runtime::block_on`. |
| `sp5-notify-rust` | `notify-rust = "4.18.0"` (`default-features = false, features = ["preview-macos-un"]`) | Blocking API; a spike-spawned OS thread blocks in `wait_for_response`.                                 |
| `sp5-baseline`    | `tauri-plugin-notification = "2.4.0"`                                                  | Registered as a Tauri plugin in `eluma_lib::run` only when this feature is on.                         |

They can be combined (`--features sp5-user-notify,sp5-notify-rust,sp5-baseline`) —
all three will send their own notification 5s after launch, each logging under its
own `[candidate]` prefix. This agent verified `cargo check`/`clippy` for every
feature, every pair, and all three together; it did not verify that running all
three simultaneously is a _good idea_ for the manual scenarios (SP5.md recommends
running one candidate per launch so a click activation cannot be misattributed to
the wrong candidate).

## Compile-only verification (what this agent ran)

macOS (native):

```sh
cd src-tauri
cargo clippy --all-targets --no-default-features                                             # base app, unaffected
cargo clippy --all-targets --no-default-features --features sp5-baseline
cargo clippy --all-targets --no-default-features --features sp5-notify-rust
cargo clippy --all-targets --no-default-features --features sp5-user-notify
cargo clippy --all-targets --no-default-features --features sp5-baseline,sp5-notify-rust,sp5-user-notify
```

Windows target, cross-checked from macOS (`llvm-rc` needed for the `.rc`
resource-embedding step in `tauri-build`/`embed-resource`):

```sh
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
cd src-tauri
cargo check --target x86_64-pc-windows-msvc --no-default-features
cargo check --target x86_64-pc-windows-msvc --no-default-features --features sp5-baseline
cargo check --target x86_64-pc-windows-msvc --no-default-features --features sp5-notify-rust
cargo check --target x86_64-pc-windows-msvc --no-default-features --features sp5-user-notify
cargo check --target x86_64-pc-windows-msvc --no-default-features --features sp5-baseline,sp5-notify-rust,sp5-user-notify
```

All ten passed clean (no warnings; the workspace denies `warnings` and `clippy::all`).
`cargo test` (all features, one at a time) passes 45/45 including the one new test in
`spike/common.rs`. `pnpm build:agent` is unaffected by any of this (TypeScript-only).

## Running each variant (owner, on real hardware)

Do this from the worktree root (`WebMails-wt/sp5`), one candidate feature at a time
so the click can't be misattributed:

### macOS 14+ — dev mode

```sh
pnpm install
pnpm build
pnpm tauri dev --features sp5-user-notify
# or: --features sp5-notify-rust
# or: --features sp5-baseline
```

**Dev-mode caveat (read before concluding anything from a dev run):**
`user-notify`'s `get_notification_manager` falls back to a log-only mock manager
whenever `NSBundle.mainBundle().bundleIdentifier()` is `None` — true for a plain
`cargo run`/`tauri dev` binary, which isn't inside a `.app` bundle. In that case you
will see `[user-notify] send_notification ok` in the log but **no real macOS
notification appears** — this is expected, not a failure, and it means dev mode
cannot be used to evaluate `user-notify` on macOS. `notify-rust`'s
`preview-macos-un` backend (`mac-usernotifications`) has the same bundle
requirement per its own docs. Use the packaged build below for any macOS S1–S7
result you intend to record in `docs/spikes/SP5.md`.

### macOS 14+ — packaged build (required for real results)

```sh
pnpm install
pnpm build
pnpm tauri build --features sp5-user-notify --bundles app
open "src-tauri/target/release/bundle/macos/Eluma.app"
```

`--bundles app` skips the slower `.dmg` step, which this spike doesn't need.

Repeat with `--features sp5-notify-rust` and `--features sp5-baseline` (separate
runs — `tauri build` overwrites the same bundle path each time, so move or rename
each `.app` before building the next candidate if you want to keep them side by
side).

**Signing** (design.md doesn't specify a signing identity for this spike; record
whichever the owner actually used, in `docs/spikes/SP5.md`'s environment table,
since `user-notify`'s own README says macOS notifications require a signed bundle
with a Developer ID / Apple developer account — an unsigned or ad-hoc-signed
build may behave differently and that difference is itself part of what S1–S7
should surface):

- Unsigned (`tauri.conf.json`'s current state — no `bundle.macOS.signingIdentity`
  set): fastest, but per `user-notify`'s README ("macOS 10.14 or above... this only
  works inside an app package with a 'Bundle ID', also you need an Apple developer
  account to sign it") this may not deliver real notifications at all. Try this
  first only to see whether it even gets past the bundle-id check into a real
  `NotificationManagerMacOS` (vs. falling back to the mock — check the log for
  which one ran).
- Ad-hoc signed: `codesign --force --deep --sign - "Eluma.app"` after building.
  Cheapest way to get a real signature without a paid account; may still be
  rejected/behave differently than Developer ID for notification delivery —
  record what actually happens.
- Developer ID signed: `security find-identity -v -p codesigning` to list
  identities, then `APPLE_SIGNING_IDENTITY=<hash> ...` — closest to how the app
  will ship. If the owner has one, prefer this for the S2/S3/S6/S7 required
  scenarios.

`tauri.conf.json`'s `identifier` is `com.goofmint.eluma` — that's the bundle id the
harness (`sp5_user_notify.rs`'s `APP_ID` constant) also uses. The **first** launch of
a signed build should trigger macOS's native "Eluma Would Like to Send You
Notifications" permission prompt; if it doesn't appear, notifications will silently
not show and that's worth recording (design.md doesn't yet have a path for the app
to ask for permission itself, since `NotificationManager::first_time_ask_for_notification_permission`
in `user-notify` is not called anywhere in this harness — this spike doesn't
implement the real onboarding flow, only the click path).

### Windows 10/11 — NSIS or MSI install (required — dev mode won't show real toasts)

PowerShell:

```powershell
pnpm install
pnpm build
pnpm tauri build --features sp5-user-notify --bundles nsis
# or: --bundles msi
```

Then run the generated installer from
`src-tauri\target\release\bundle\nsis\Eluma_0.1.0_x64-setup.exe` (or
`bundle\msi\Eluma_0.1.0_x64_en-US.msi`), launch **Eluma from its Start Menu
shortcut** (not by double-clicking the `.exe` in `target\release\` directly — see
AUMID note below), then repeat with `--features sp5-notify-rust` and
`--features sp5-baseline`, reinstalling between each so only one candidate's
notification fires per run.

**AUMID (Application User Model ID)** — both click-capable candidates need one for
real Windows toast delivery, confirmed from source:

- `user-notify`'s `get_notification_manager` calls
  `ToastNotificationManager::CreateToastNotifierWithId(app_id)`; if that call fails
  (no matching registered AUMID) it **silently falls back to the log-only mock
  manager** — same failure mode as the macOS bundle-id case above. The `app_id`
  passed is `sp5_user_notify.rs`'s `APP_ID` constant, currently hardcoded to
  `com.goofmint.eluma` to match `tauri.conf.json`'s `identifier`; if the
  owner's install pipeline registers a different AUMID (Tauri's NSIS/MSI bundler
  can set its own), change that constant to match and rebuild.
- `notify-rust`'s Windows backend (`tauri-winrt-notification`) reads
  `notification.app_id`, defaulting to `Toast::POWERSHELL_APP_ID` when unset — this
  harness does not currently call `.app_id(...)` on the `Notification` builder in
  `sp5_notify_rust.rs`, so on Windows it will show under PowerShell's identity/icon
  unless the owner adds that call. This is a real, observable difference worth
  recording under S1 (does the toast show Eluma's name/icon, or PowerShell's?).
- Tauri's NSIS/MSI bundlers register a Start Menu shortcut with an AUMID derived
  from `tauri.conf.json` automatically as part of a normal install — this is _why_
  a packaged install (not a bare `.exe` copy) matters for this spike. Record in
  `docs/spikes/SP5.md` what AUMID actually got registered (Start Menu shortcut →
  right-click → Properties, or `Get-StartApps` in PowerShell) versus what the
  harness's constants assume.
- `user-notify`'s Windows module also has an (unused, commented-out) fallback to a
  hardcoded PowerShell AUMID for cases where the app's own toast notifier can't be
  created — this harness does not exercise that path since it's commented out in
  the crate itself; note it if the owner sees behavior suggesting it's relevant.

## Logs

All three candidates and `common::spike_log` write to the **same** file, so a run
with multiple features enabled interleaves them (each line is prefixed
`[candidate]`):

- stderr of the running process (visible in `pnpm tauri dev`'s terminal; not
  visible for a packaged/installed app unless launched from a terminal — on
  Windows, launch the installed `.exe` from `cmd`/PowerShell rather than the Start
  Menu if you want to see stderr live)
- `{app_data_dir}/logs/spike-sp5.log`, where `app_data_dir` (Tauri's resolver,
  `data_dir/{identifier}`) is:
  - macOS: `~/Library/Application Support/com.goofmint.eluma/logs/spike-sp5.log`
  - Windows: `%APPDATA%\com.goofmint.eluma\logs\spike-sp5.log` (i.e.
    `C:\Users\<user>\AppData\Roaming\com.goofmint.eluma\logs\spike-sp5.log`)

This is why the log also goes to a file, not just stderr: S4/S5 in
`docs/spikes/SP5.md` specifically involve the window being minimized or the app in
the background when the click happens, so a terminal may not be visible to copy
from.

## What this harness deliberately does not do

Per the CodeRabbit plan for this task: no real `NotificationSink` trait, no
dispatcher, no dedupe/seen-ring, no settings integration, no `select-service`
event, no deep-link navigation into a service webview. It only proves (or
disproves) that a click callback reaches Rust code with the right payload, on the
right thread, and that `show`/`unminimize`/`set_focus` can be driven from that
callback via `run_on_main_thread`. The real `sink.rs` click path is Task 4.2/4.6's
job, once `docs/spikes/SP5.md`'s decision is made.
