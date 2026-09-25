/**
 * Typed wrappers over the `shell`-webview-facing subset of
 * src-tauri/src/commands/mod.rs's `COMMAND_NAMES` that this task's UI
 * needs (design.md §2.2.12): `get_snapshot`, `select_service`,
 * `reorder_services`, `open_settings`. The remaining commands
 * (`add_service`, `update_service`, `remove_service`, `update_settings`)
 * belong to the settings window, Task 1.12/1.13.
 *
 * Every call sites `invoke`'s generic parameter to the exact response type
 * instead of trusting an inferred `any`, per the project's "never `any`"
 * rule.
 */

import { invoke } from "@tauri-apps/api/core";
import type { Snapshot } from "./types";

/** `get_snapshot` — no input, `Snapshot` output (design.md §2.2.12). */
export async function getSnapshot(): Promise<Snapshot> {
  return invoke<Snapshot>("get_snapshot");
}

/**
 * `select_service` — input `{ id }` (design.md §2.2.12). Activates `id`'s
 * webview and, on success, causes a `select-service` event.
 */
export async function selectService(id: string): Promise<void> {
  await invoke<void>("select_service", { id });
}

/**
 * `reorder_services` — input `{ ids }` (design.md §2.2.12). `ids` must be a
 * permutation of the current service ids; `config::apply` enforces that on
 * the Rust side.
 */
export async function reorderServices(ids: readonly string[]): Promise<void> {
  await invoke<void>("reorder_services", { ids });
}

/**
 * `open_settings` — no input (design.md §2.2.12). Opens the settings
 * window, or focuses it if one is already open.
 */
export async function openSettings(): Promise<void> {
  await invoke<void>("open_settings");
}
