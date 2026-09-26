/**
 * Whether `hash` (a `window.location.hash` value) names the settings
 * route (`#/settings`, matching `commands/mod.rs`'s
 * `SETTINGS_WINDOW_PATH` constant `"index.html#/settings"`). A pure
 * function so route selection is testable without a DOM `location`
 * object — `App.tsx` is the only caller that reads
 * `window.location.hash` itself.
 */
export function isSettingsRoute(hash: string): boolean {
  return hash.startsWith("#/settings");
}
