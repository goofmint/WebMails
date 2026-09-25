# SP1 — Multiwebview host spike

## Purpose

Decide the default `WebviewHost` implementation (design §2.2.4) before M1 work depends on
it: `MultiwebviewHost` (one `Window`, service webviews added as children via
`Window::add_child`, `unstable` feature) or `ChildWindowHost` (fallback, one borderless
`WebviewWindow` per service, feature `host-child-windows`).

## Question (design §6.3, SP1)

> Does `add_child` multiwebview with 4+ children, offscreen positioning and resize behave on
> macOS 14 and Windows 11?

## Environment (owner fills in per run)

| Field                              | macOS                                                 | Windows                                                          |
| ---------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------- |
| OS build                           | _(e.g. macOS 14.x, build xxXXX — fill in)_            | _(e.g. Windows 11 23H2, build xxxxx — fill in)_                  |
| CPU                                | _(e.g. Apple M-series — fill in)_                     | _(fill in)_                                                      |
| Display scale factor               | _(e.g. 2.0 — fill in)_                                | _(e.g. 1.0 / 1.25 / 1.5 — fill in)_                              |
| `tauri` version (Cargo.lock)       | 2.11.6                                                | 2.11.6                                                           |
| `wry` version (Cargo.lock)         | 0.55.1                                                | 0.55.1                                                           |
| `tao` version (Cargo.lock)         | 0.35.3                                                | 0.35.3                                                           |
| `tauri-utils` version (Cargo.lock) | 2.9.3                                                 | 2.9.3                                                            |
| WebView2 Runtime version           | n/a                                                   | _(fill in, `winver` of `msedgewebview2.exe` or Settings → Apps)_ |
| Branch / commit                    | `spike/m0-harness` @ _(fill in `git rev-parse HEAD`)_ | same                                                             |

## Implementation summary

- Mode: `ELUMA_SPIKE=sp1`.
- `src-tauri/tauri.conf.json` has no `app.windows` entry; the main window is built in
  `src-tauri/src/spike/mod.rs` / `src-tauri/src/spike/sp1.rs` with `tauri::WindowBuilder`.
- One `shell` child (`public/spike/shell.html`, fixed width = `SIDEBAR_WIDTH`,
  `src-tauri/src/spike/common.rs`) plus 4 service children `svc-0`..`svc-3`
  (`public/spike/service.html?n=0..3` via an injected `window.__ELUMA_SPIKE_SERVICE__`).
  One slot can be pointed at a real webmail URL by setting `ELUMA_SPIKE_SP1_URL` before
  launch (see `SPIKE.md`).
- The active child is placed at `x = SIDEBAR_WIDTH`; inactive children are placed at
  `x = -(content width + SIDEBAR_WIDTH)` — moved offscreen, never hidden (design §2.2.4).
- `switch_service(index)` is a `#[tauri::command]`, callable only from the `shell` webview
  (a dedicated runtime/static capability scoped to `webviews: ["shell"]` — see
  `src-tauri/capabilities/spike-local.json`).
- Relayout runs from `WindowEvent::Resized` and `WindowEvent::ScaleFactorChanged`, converting
  the physical size to logical using the window's current `scale_factor()`.

## Procedure

On each target OS, from a checkout of `spike/m0-harness`:

```sh
pnpm install
pnpm build
ELUMA_SPIKE=sp1 pnpm tauri dev
```

Run scenarios S1–S8 below. Record OS build, CPU, and display scale before starting (table
above). For each scenario, record logical position/size in the form `x,y wxh @ scale` — never
raw pixels without the scale factor, since Windows can run at non-integer scale factors.

## Scenarios and results

| Id  | Scenario                                                                                                                                               | Expected                                                                                                                                       | Actual         | Verdict |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- | -------------- | ------- |
| S1  | Startup draws shell + all 4 service webviews (only the active one visible in-bounds)                                                                   | Shell visible at x=0..SIDEBAR_WIDTH; active service fills the rest; no flash of unstyled/blank content                                         | 未実施 (owner) |         |
| S2  | Switching between the 4 children via the shell buttons, then interacting with each (click the counter area, focus)                                     | Old active child moves offscreen without being destroyed (counter keeps running); new active child appears and receives focus/input            | 未実施 (owner) |         |
| S3  | Offscreen children: invisibility, click-through, and counter liveness                                                                                  | Offscreen children are not painted on screen, cannot be clicked, and their 1s counters keep incrementing (confirmed by switching back to them) | 未実施 (owner) |         |
| S4  | Drag-resize the window, maximize, restore                                                                                                              | All visible/offscreen children relayout correctly at each step; no stale geometry; inactive children stay offscreen after resize               | 未実施 (owner) |         |
| S5  | macOS fullscreen (macOS) / Windows Snap (Windows)                                                                                                      | Layout adapts to the new content rect without leftover gaps or overlapping webviews                                                            | 未実施 (owner) |         |
| S6  | Minimize and restore the window                                                                                                                        | No crash; children resume in their prior active/inactive state after restore                                                                   | 未実施 (owner) |         |
| S7  | Change display scale factor (move window to a different-DPI display, or change OS scaling) while running                                               | `ScaleFactorChanged` relayouts all children to the new logical geometry without visual corruption                                              | 未実施 (owner) |         |
| S8  | Auxiliary: compare `.hide()`-based visibility vs. offscreen positioning, and `.auto_resize()` vs. manual relayout (toggle via env var, see `SPIKE.md`) | Documented qualitative difference, if any                                                                                                      | 未実施 (owner) |         |

Known upstream issues to watch for: tauri#10420, tauri#11376 — record reproduction steps if
hit, and whether changing child creation order or repositioning after creation works around
them.

## Decision criteria

S1, S2, S3, S4, S7 are the blocking scenarios (design's Task 0.3 decision rule):

- If **both** OSes pass all of S1/S2/S3/S4/S7, or every failure there has an acceptable
  workaround → choose **`MultiwebviewHost`**.
- If **either** OS has an unavoidable blocker in any of S1/S2/S3/S4/S7 → choose
  **`ChildWindowHost`** (fallback, `host-child-windows` feature).
- Also weigh the risk of relying on the `unstable` Tauri API surface (`WindowBuilder::build`
  outside `tauri.conf.json`, `Window::add_child`) even when functionally passing.

tauri#10420 is a Linux-only issue and is out of scope for this decision (Eluma targets macOS
and Windows only); it is tracked below under residual risks only for completeness.

## Decision

**Pending — owner to fill in after running the procedure above on both OSes.**

- Chosen host: _(MultiwebviewHost / ChildWindowHost)_
- Rationale: _(fill in, referencing the scenario table)_
- If `ChildWindowHost` was chosen, the specific Multiwebview blockers that would need to be
  fixed upstream (or worked around) before re-evaluating `MultiwebviewHost` for M1:
  _(fill in — e.g. "S3 on Windows 11 build XXXXX: offscreen svc-2 receives clicks")_

## Impact on later tasks

- Task 1.6 (`WebviewHost` trait and default host) implements whichever host is chosen here.
- Task 1.7 (fallback host) implements the other one if not already covered by this spike's
  code, gated behind `host-child-windows`.

## Residual risks

- tauri#10420 (Linux child-webview issue) — out of scope for Eluma (macOS/Windows only),
  listed here only so it is not mistaken for an open question.
- Any scenario left "未実施 (owner)" above is an open risk until run.
- This spike used `unstable` Tauri APIs; their stability contract across Tauri point releases
  is weaker than stable APIs.

## Evidence

Screenshots and recordings: `docs/spikes/sp1/` (owner to add; reference specific files here,
e.g. `![S1 startup](sp1/s1-startup-macos.png)`). Only `SP1.md` and evidence images are meant
to land on `main`; the `spike/m0-harness` branch itself is not merged, and should not be
deleted — it stays as the reference implementation this document describes.
