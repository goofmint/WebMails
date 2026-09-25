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
- [ ] The hard-coded test service (`https://example.com`) is visible and interactive in the content area on launch (macOS)
- [ ] The hard-coded test service (`https://example.com`) is visible and interactive in the content area on launch (Windows 11)
- [ ] Drag-resizing the window relays out the shell and the test service with no stale geometry, gap or overlap (macOS)
- [ ] Drag-resizing the window relays out the shell and the test service with no stale geometry, gap or overlap (Windows 11)
- [ ] Changing the display scale factor (moving the window to a different-DPI display, or changing OS scaling) relays out both webviews correctly (macOS)
- [ ] Changing the display scale factor relays out both webviews correctly (Windows 11)
- [ ] Minimising and restoring the window leaves the shell and the test service in their prior layout, with no crash (macOS)
- [ ] Minimising and restoring the window leaves the shell and the test service in their prior layout, with no crash (Windows 11)

### Task 1.7 — Fallback host (`ChildWindowHost`, `cargo tauri dev --features host-child-windows`)

For each item, record date, platform (macOS 14+ or Windows 11) and result (pass/fail, with notes) before checking it off.

- [ ] Built with `--features host-child-windows`: the shell and the hard-coded test service render correctly, switching (would-be) services relays out and focuses correctly, and drag-resizing, drag-moving, minimising/restoring and closing the main window all keep the service window(s) in sync (matching geometry offscreen/onscreen, closed on quit) with no gap, overlap, stale geometry or crash (macOS)
- [ ] Same, on Windows 11

## M2 — Unread

Exit criterion (SPEC §16): N Gmail accounts plus iCloud show live counts while the window is in the background.

- [ ] With N Gmail accounts and iCloud configured at the same time, every service shows a live count while the window is in the background

## M3 — Background survival

Exit criterion (SPEC §16): badges still correct after 2 hours minimised, on both platforms — or the Windows story is honestly documented as degraded.

- [ ] Badges match the real inboxes after 2 hours minimised (macOS)
- [ ] Badges match the real inboxes after 2 hours minimised (Windows 11), or the degraded state is documented
- [ ] Badges match the real inboxes after 2 hours minimised (Windows 10), or the degraded state is documented
- [ ] Memory use per service recorded (R7)

## M4 — Notifications

Exit criterion (SPEC §16): iCloud fires a native notification with Eluma in the background.

- [ ] iCloud fires a native notification with Eluma in the background

## M5 — Recipes

Exit criterion (SPEC §16): a contributor adds Fastmail by adding one recipe module, without touching Rust.

- [ ] Fastmail is added by adding one recipe module, without touching Rust, and shows a live count
