# Eluma spike harness (`spike/m0-harness`)

Throwaway harness for tasks.md Tasks 0.3-0.6 (GitHub issues #3-#6: SP1-SP4). **This branch is
never merged to `main`.** Only `docs/spikes/SP{1,2,3,4}.md` and evidence images from a manual
run are meant to land on `main`.

This is one app, one mode at a time, selected **at runtime** by the `ELUMA_SPIKE` environment
variable — not a Cargo feature, so all four modes' code always compiles together. All spike
code lives under `src-tauri/src/spike/`; nothing here is production code.

Building/checking this harness is fine to do yourself. **Do not run `pnpm tauri dev`, `cargo
run`, or the built binary** — that's the project owner's job, on macOS 14+ and Windows 10/11,
following the procedures in `docs/spikes/SP{1,2,3,4}.md`.

## Setup (once)

```sh
pnpm install
pnpm build      # builds the agent stub + the shell's dist/, including dist/spike/*.html
```

`pnpm build` must run before `pnpm tauri dev`/`cargo build` picks up `public/spike/*.html` —
Vite copies `public/` verbatim into `dist/`, and `tauri.conf.json`'s `frontendDist` points at
`../dist`.

## Running a mode

```sh
ELUMA_SPIKE=sp1 pnpm tauri dev
ELUMA_SPIKE=sp2 pnpm tauri dev
ELUMA_SPIKE=sp3 pnpm tauri dev   # debug build only; a release build errors out on purpose
ELUMA_SPIKE=sp4 pnpm tauri dev
```

An unset or unrecognised `ELUMA_SPIKE` is a hard error at startup (`src-tauri/src/spike/mod.rs`)
— there is no default mode.

## Common structure (every mode)

- The main `Window` is built in code with `tauri::window::WindowBuilder` (`tauri.conf.json`'s
  `app.windows` is `[]`).
- A `shell` child (`public/spike/shell.html`) is always the leftmost `SIDEBAR_WIDTH` (64
  logical px, `spike::common::SIDEBAR_WIDTH`) strip, added via `Window::add_child` (the
  `unstable` Cargo feature).
- Every other child is a "service" child. The active one fills the rest of the window; every
  inactive one is moved fully outside the window's bounds (never hidden —
  `spike::common::geometry`).
- The shell's only command is `switch_service(index)`, restricted by
  `src-tauri/capabilities/spike-local.json` to the `shell` webview label only. Clicking a
  sidebar button calls it; `spike::common::relayout` re-derives every child's position/size
  from the window's current logical size and re-runs on `WindowEvent::Resized` and
  `WindowEvent::ScaleFactorChanged`.
- Every child webview (shell included) is built through `spike::common::base_child_builder`,
  which applies `background_throttling(Disabled)` on macOS and the shared Windows
  browser-argument constant (`spike::common::WEBVIEW2_ARGS`, design §2.2.4) on Windows — the
  one exception is SP4's own test page, which can opt out of the Windows constant for its
  "control run" comparison (see below).

## SP1 — multiwebview host (`docs/spikes/SP1.md`)

```sh
ELUMA_SPIKE=sp1 pnpm tauri dev
```

Shell + 4 local test children `svc-0`..`svc-3` (`public/spike/service.html?n=<i>`, injected via
`window.__ELUMA_SPIKE_SERVICE__` rather than a real query string). Each shows its index, a live
`innerWidth`/`innerHeight`/`devicePixelRatio` readout, a 1s counter, and four corner markers.

- Click the sidebar buttons to switch the active child. Watch that the previous one's counter
  keeps advancing once you switch back to it (proof it kept running offscreen, not hidden).
- Resize/maximize/restore/fullscreen/Snap the window and confirm every child's geometry stays
  correct (the always-visible one fills the content rect; the rest stay fully offscreen).
- **`ELUMA_SPIKE_SP1_URL=<https-url>`** points the last slot (`svc-3`) at a real webmail URL
  instead of the local test page, for a closer-to-real check.
- **`ELUMA_SPIKE_AUTO_RESIZE=1`** switches every service child to
  `WebviewBuilder::auto_resize()` instead of the manual `set_size` calls `common::relayout`
  otherwise makes on resize — for the S8 comparison only (`docs/spikes/SP1.md`); leave it unset
  for every other scenario/mode.
- Follow the S1-S8 scenario table and decision rule in `docs/spikes/SP1.md`.

## SP2 — data store isolation (`docs/spikes/SP2.md`)

```sh
ELUMA_SPIKE=sp2 pnpm tauri dev
```

Shell + 2 children (`isolated-a`, `isolated-b`), both loading `https://www.icloud.com/`, each
with its own UUIDv5-derived profile (`spike::data_store`, namespace UUID
`spike::data_store::SPIKE_NAMESPACE`, keys `isolated:a` / `isolated:b`):

- macOS: `WebviewBuilder::data_store_identifier(uuid.into_bytes())`.
- Windows: `WebviewBuilder::data_directory({app_data_dir}/webview/<uuid>)`.

Both UUIDs (and, on Windows, both directory paths) are printed to stdout at startup. Sign in to
two different iCloud accounts side by side, restart normally (and, as an extra observation,
after a force-quit), and confirm both sessions persist without re-authentication. Follow
`docs/spikes/SP2.md`'s procedure — including wiping any pre-existing data for both slots
first, and never committing real account details or screenshots that show them.

## SP3 — remote IPC capability (`docs/spikes/SP3.md`)

```sh
ELUMA_SPIKE=sp3 pnpm tauri dev
```

Debug builds only — a release build's `sp3::setup` returns an error instead of silently
skipping the check. Shell + 3 children probing `report_unread`'s runtime capability:

| Label                      | Loads              | Granted the `allow-report-unread` capability? | Expected `invoke` result |
| -------------------------- | ------------------ | --------------------------------------------- | ------------------------ |
| `svc-allowed`              | service origin     | Yes (label + origin both match)               | Succeeds                 |
| `svc-denied`               | service origin     | No (label never listed)                       | Denied                   |
| `svc-allowed-other-origin` | a different origin | Label yes, origin no                          | Denied                   |

- **`ELUMA_SPIKE_SP3_SERVICE_ORIGIN`** (default `https://mail.google.com`) and
  **`ELUMA_SPIKE_SP3_OTHER_ORIGIN`** (default `https://example.com`) let you point this at
  whatever real webmail origin you're validating against.
- From each window's DevTools console, run:
  ```js
  window.__TAURI_INTERNALS__.invoke("report_unread", { count: 3 });
  ```
  and also try navigating `svc-allowed` to the other origin before invoking again. Rust's
  stdout logs the capability that was registered and, on every successful call,
  `label=... url=... count=...`.
- The confirmed permission identifier is `allow-report-unread` (kebab-cased from the command
  name, no crate/plugin prefix needed for an app-defined command) — verified from the actual
  generated `src-tauri/permissions/autogenerated/report_unread.toml` and
  `src-tauri/gen/schemas/acl-manifests.json` after building, not just from reading source. See
  `docs/spikes/SP3.md` for the full citation trail.
- Follow the four-behaviour checklist and results table in `docs/spikes/SP3.md`.

## SP4 — WebView2 throttling (`docs/spikes/SP4.md`)

```sh
ELUMA_SPIKE=sp4 pnpm tauri dev
```

Shell + one `sp4-page` child (`public/spike/sp4.html`): a 5s heartbeat timer, a second 5s timer
that mutates `<title>`, a `MutationObserver` on that mutation, and a `visibilitychange`
listener. Every event is logged with `Date.now()`, `performance.now()`, the delta from the
previous same-kind event, and `document.visibilityState` — to the page (in-memory + the page's
own `localStorage`), to the DevTools console, **and**, via the `spike_log` command, to Rust
`stderr` and `{app_data_dir}/logs/spike-sp4.log`, so the owner can read what happened after
minimizing/hiding the window without DevTools open.

- **`ELUMA_SPIKE_SP4_NO_ARGS=1`** skips the Windows browser-argument constant for this one
  child only, for the "control run, no constant" comparison
  (`docs/spikes/SP4.md` procedure step 4). Every other mode's children, and this mode's
  `shell`, always get the constant regardless of this variable.
- Fully quit the app and any `msedgewebview2.exe` processes before each run (stale WebView2
  processes are shared and would mask the test).
- Follow the minimized (30 min)/offscreen/occluded/control procedure and judgment rule
  (continued / strongly throttled / stopped) in `docs/spikes/SP4.md`.

## Verified vs. guessed API

Every Tauri/wry/uuid API this harness calls was confirmed by reading the actual vendored
source under `~/.cargo/registry/src/*/` for the exact versions pinned in `Cargo.lock` — not
guessed, and not taken only from docs.rs, which can drift from what's actually vendored.
Notable citations (file paths relative to
`~/.cargo/registry/src/index.crates.io-*/<crate>-<version>/`):

- `Window::add_child`, `WindowBuilder::new`/`.build()`, `WebviewBuilder::new`,
  `.data_store_identifier`/`.data_directory`/`.background_throttling`/
  `.additional_browser_args`/`.initialization_script`, `Webview::label`/`.url`/`.set_position`/
  `.set_size`/`.set_focus`, `WindowEvent::Resized`/`ScaleFactorChanged`,
  `Window::on_window_event`, `Manager::path().app_data_dir()` — all in
  `tauri-2.11.6/src/{window,webview,app,path}/*.rs` and `tauri-2.11.6/src/lib.rs`.
- `tauri::ipc::CapabilityBuilder` and `Manager::add_capability` (behind the `dynamic-acl`
  feature, which is in `tauri`'s own `default = [...]` feature list, so no extra Cargo.toml
  change was needed) — `tauri-2.11.6/src/ipc/capability_builder.rs` and
  `tauri-2.11.6/src/lib.rs`.
- `tauri_build::Attributes`/`AppManifest`/`try_build`, and the `allow-$command` /
  `deny-$command` permission-naming rule (`command.replace('_', '-')`) — confirmed both from
  source (`tauri-build-2.6.3/src/acl.rs`,
  `tauri-utils-2.9.3/src/acl/build.rs::autogenerate_command_permissions`) **and** from this
  harness's own generated `src-tauri/permissions/autogenerated/*.toml` and
  `src-tauri/gen/schemas/acl-manifests.json` after a real `cargo build`.
- `Uuid::new_v5`, `Uuid::from_bytes` (`const fn`, used for the fixed namespace constant),
  `Uuid::get_version_num` — `uuid-1.26.1/src/{v5,builder,lib}.rs`.
- `dpi::{Position,Size,LogicalPosition,LogicalSize,PhysicalSize}` and their `Into`/`to_logical`
  conversions, re-exported through `tauri::` — `dpi-0.1.2/src/lib.rs` and
  `tauri-2.11.6/src/lib.rs`.

## Verifying it compiles (do this, don't run the app)

```sh
pnpm install        # only if node_modules is missing
pnpm build
cargo build --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
/Users/nakatsugawa/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo-fmt fmt --check
```

The `~/.cargo/bin/rustfmt` on this machine is broken; the `rustup`-toolchain `cargo-fmt` above
is the one that works.

The `x86_64-pc-windows-msvc` target **is** installed on this machine, so the `#[cfg(windows)]`
code (`additional_browser_args`, `data_directory`) was also actually cross-checked, not just
inspected against the vendored source:

```sh
PATH="/opt/homebrew/opt/llvm/bin:$PATH" \
  cargo check --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc
PATH="/opt/homebrew/opt/llvm/bin:$PATH" \
  cargo clippy --manifest-path src-tauri/Cargo.toml --target x86_64-pc-windows-msvc --all-targets
```

Both pass clean. The `PATH` prefix is required: `tauri-winres` (an icon/resource-embedding
build dependency, cross-compiling for a Windows target from macOS) shells out to `llvm-rc`,
which Homebrew's `llvm` formula installs but does not link onto `PATH` by default — it's at
`/opt/homebrew/opt/llvm/bin/llvm-rc` on this machine. Without it, the build script panics with
`NotAttempted("llvm-rc")` before any of _this_ crate's code is even type-checked, which is an
environment/toolchain issue, not a bug in the spike's Rust code.

A cross-compiled `cargo check`/`clippy` still cannot catch everything a real Windows toolchain
(`cargo build --target x86_64-pc-windows-msvc` with the MSVC linker) would — e.g. linking
against the real `WebView2Loader.dll` import library — so this is strong but not complete
evidence; the owner's actual Windows run in `docs/spikes/SP{1,2,4}.md` remains the real test.
