# Manual verification checklist

This checklist maps each milestone exit criterion in `SPEC.md` §16 to manual checks against real services. The owner runs the app (`pnpm tauri dev`) and ticks each item after verifying it. Record the date, platform and result under each item.

Platforms: macOS 14+ and Windows 11.

## M1 — Shell

Exit criterion (SPEC §16): two iCloud accounts signed in side by side; a URL becomes a usable mail tab.

- [ ] Two iCloud accounts are signed in side by side (macOS)
- [ ] Two iCloud accounts are signed in side by side (Windows 11)
- [ ] Adding a URL in the settings window produces a usable mail tab

## M2 — Unread

Exit criterion (SPEC §16): N Gmail accounts plus iCloud show live counts while the window is in the background.

- [ ] With N Gmail accounts and iCloud configured at the same time, every service shows a live count while the window is in the background

## M3 — Background survival

Exit criterion (SPEC §16): badges still correct after 2 hours minimised, on both platforms — or the Windows story is honestly documented as degraded.

- [ ] Badges match the real inboxes after 2 hours minimised (macOS)
- [ ] Badges match the real inboxes after 2 hours minimised (Windows 11), or the degraded state is documented
- [ ] Memory use per service recorded (R7)

## M4 — Notifications

Exit criterion (SPEC §16): iCloud fires a native notification with Eluma in the background.

- [ ] iCloud fires a native notification with Eluma in the background

## M5 — Recipes

Exit criterion (SPEC §16): a contributor adds Fastmail by adding one recipe module, without touching Rust.

- [ ] Fastmail is added by adding one recipe module, without touching Rust, and shows a live count
