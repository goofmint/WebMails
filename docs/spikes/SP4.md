# SP4 — WebView2 background throttling spike (SPEC §17 Q1)

## Purpose

Determine whether a hidden/minimized/occluded webview on Windows keeps running JS timers and
DOM mutation observers when the design §2.2.4 browser-argument constant is applied to every
webview, since Eluma's whole unread-badge mechanism (design §2.2.6–§2.2.8) depends on the
in-page agent continuing to run while the window is minimized. This is SPEC §17 Q1, called out
in design §6.3 as "highest-value experiment."

## Question (design §6.3, SP4)

> Do the WebView2 throttling flags keep a hidden or minimized page's timers and
> `MutationObserver` running?

## Environment

| Field                        | Windows                                                                                                    |
| ---------------------------- | ---------------------------------------------------------------------------------------------------------- |
| OS build                     | _(fill in — Windows 10/11)_                                                                                |
| CPU                          | _(fill in)_                                                                                                |
| Display scale factor         | _(fill in)_                                                                                                |
| `tauri` version (Cargo.lock) | 2.11.6                                                                                                     |
| `wry` version (Cargo.lock)   | 0.55.1                                                                                                     |
| `tao` version (Cargo.lock)   | 0.35.3                                                                                                     |
| WebView2 Runtime version     | _(fill in — Settings → Apps → "Microsoft Edge WebView2 Runtime", or file version of `msedgewebview2.exe`)_ |
| Branch / commit              | `spike/m0-harness` @ _(fill in)_                                                                           |

## Arguments and how they were confirmed applied

Design §2.2.4 constant (applied to **every** webview on Windows, shell included, via
`WebviewBuilder::additional_browser_args` before the webview is created — see
`src-tauri/src/spike/common.rs::WEBVIEW2_ARGS`):

```
--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-background-timer-throttling --disable-renderer-backgrounding --disable-backgrounding-occluded-windows
```

`additional_browser_args` replaces wry's own default arguments rather than adding to them
(design §10); the owner should note in the results below whether any framework default that
was previously relied upon appears to be missing, and whether it should be re-added to the
constant.

To confirm the shared `msedgewebview2.exe` browser process actually launched with these
arguments: fully quit the app and any existing `msedgewebview2.exe` processes first (stale
processes are shared/reused by WebView2 and would mask the test), start the app fresh, then
inspect the process command line with Process Explorer (or `wmic process where
name='msedgewebview2.exe' get commandline` / Task Manager → Details → add "Command line"
column). Record what was found:

- Process command line observed: _(fill in — paste the relevant `--disable-...` portion)_
- All flags from the constant present: _(yes/no — fill in)_

## Test page

`public/spike/sp4.html`, loaded by the `sp4-page` child webview when `ELUMA_SPIKE=sp4`. It
runs, independently: a 5s `setInterval` heartbeat, a second 5s timer that mutates
`document.title`, a `MutationObserver` on `<title>` observing that mutation, and a
`visibilitychange` listener. Each event records `Date.now()`, `performance.now()`, the delta
from the previous event of the same kind, and `document.visibilityState`. Logs go to an
in-memory array, `localStorage`, the DevTools console, and — via the `spike_log` command — to
Rust `stderr` and a file under `{app_data_dir}/logs/spike-sp4.log`, so they can be read after
the fact without needing to have DevTools open while the window was minimized/occluded.

On macOS this same page is also loaded, but with `background_throttling(Disabled)`
(`BackgroundThrottlingPolicy::Disabled`) rather than the Windows browser-argument constant;
recording the macOS case here too is optional but useful as a cross-platform baseline.

## Procedure

Before **each** run below: fully quit the app and any `msedgewebview2.exe` processes, then
reset the log (the page's "Reset this case" button, or delete
`{app_data_dir}/logs/spike-sp4.log`).

```sh
pnpm install
pnpm build
ELUMA_SPIKE=sp4 pnpm tauri dev
```

Windows PowerShell:

```powershell
pnpm install
pnpm build
$env:ELUMA_SPIKE = "sp4"; pnpm tauri dev
```

1. **Minimized, 30 minutes.** Start the app, let the page log a few ticks normally, minimize
   the window, wait the full 30 minutes (do not shorten this), then restore and copy the log.
2. **Offscreen.** Move the window fully off any monitor's visible bounds (not minimized) for a
   sustained period, then bring it back and copy the log.
3. **Occluded.** Fully cover the window with another window (not minimized, not moved
   offscreen) for a sustained period, then uncover it and copy the log.
4. **Control run, no constant.** Temporarily remove the `additional_browser_args` call (or set
   an env var that disables it, if wired up — see `SPIKE.md`), fully quit the app and
   `msedgewebview2.exe`, relaunch, and repeat a short version of the minimized case (this run
   does not need the full 30 minutes) to see whether the constant made a difference.

For each run, save the copied log text into this document (excerpt) and keep the full copy
alongside (e.g. `docs/spikes/sp4/` — owner to create if needed).

## Judgment rule

Classify each timer/observer **independently**:

- **Continued** — steady ~5s intervals throughout.
- **Strongly throttled** — intervals settle around ~60s (the typical browser background-timer
  clamp).
- **Stopped** — the log goes silent (a gap with no further entries) before the run ends.

## Results

| Case                                  | Timer (`timer-5s`) | Title-mutation timer (drives observer) | `MutationObserver` (`title-mutation-observed`) | `visibilitychange` fired | Notes |
| ------------------------------------- | ------------------ | -------------------------------------- | ---------------------------------------------- | ------------------------ | ----- |
| Minimized, 30 min                     | 未実施 (owner)     | 未実施 (owner)                         | 未実施 (owner)                                 | 未実施 (owner)           |       |
| Offscreen                             | 未実施 (owner)     | 未実施 (owner)                         | 未実施 (owner)                                 | 未実施 (owner)           |       |
| Occluded                              | 未実施 (owner)     | 未実施 (owner)                         | 未実施 (owner)                                 | 未実施 (owner)           |       |
| Control (no constant), short minimize | 未実施 (owner)     | 未実施 (owner)                         | 未実施 (owner)                                 | 未実施 (owner)           |       |

## Log excerpts

_(Owner: paste representative excerpts per case here, e.g. the entries immediately before and
after the widest gap, plus the full copied log saved under `docs/spikes/sp4/`.)_

- Minimized, 30 min: 未実施 (owner)
- Offscreen: 未実施 (owner)
- Occluded: 未実施 (owner)
- Control: 未実施 (owner)

## Conclusion

**Pending — owner to fill in.**

- Whether Windows remains a first-class target for M3 (background survival), or the Windows
  story must be documented as degraded (per tasks.md Task 0.6 / M3 exit criterion in
  `docs/manual-checks.md`): _(fill in)_
- Any gap vs. the design §2.2.4 constant (arguments that did not have the expected effect, or
  a framework default worth re-adding): _(fill in)_
- Whether any default browser argument that was replaced (not merely added to) should be
  re-included in the constant going forward: _(fill in)_
- Risk: browser flags are an implementation detail of the WebView2/Chromium version in use and
  can change or stop working across Edge/WebView2 updates without notice. Recommendation for
  the owner: _(fill in — e.g. re-run this spike's minimized case after material WebView2
  Runtime updates, or add an automated smoke check before each release)_.
