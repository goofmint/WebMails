/**
 * Typed wrappers over the `shell`/`settings`-webview-facing subset of
 * src-tauri/src/commands/mod.rs's `COMMAND_NAMES` these two windows need
 * (design.md §2.2.12): `get_snapshot`, `select_service`,
 * `reorder_services`, `open_settings` (Task 1.10), and `add_service`,
 * `update_service`, `remove_service` (Task 1.12, for the settings
 * window's CRUD forms). `update_settings` still belongs to Task 1.13.
 *
 * Every call sites `invoke`'s generic parameter to the exact response type
 * instead of trusting an inferred `any`, per the project's "never `any`"
 * rule.
 */

import { invoke } from "@tauri-apps/api/core";
import type {
  ServiceConfig,
  ServicePatchInput,
  Settings,
  SettingsPatchInput,
  Snapshot,
} from "./types";

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

/**
 * `add_service` — input `{ name, url, profile }`, output the new
 * `ServiceConfig` (design.md §2.2.12, §2.2.13's add form; Task 1.12). No
 * `rename_all` on the Rust command, so every argument key matches its
 * Rust parameter name verbatim.
 */
export async function addService(
  name: string,
  url: string,
  profile: string,
): Promise<ServiceConfig> {
  return invoke<ServiceConfig>("add_service", { name, url, profile });
}

/**
 * `update_service` — input `{ id, patch }`, output the updated
 * `ServiceConfig` (Task 1.12). Only the fields actually being changed
 * should be present on `patch` — an omitted key leaves that field
 * untouched server-side (`ServicePatchDto`'s `Option<T>` fields
 * deserialize to `None` when the key is absent).
 */
export async function updateService(id: string, patch: ServicePatchInput): Promise<ServiceConfig> {
  return invoke<ServiceConfig>("update_service", { id, patch });
}

/**
 * `remove_service` — input `{ id, deleteSessionData }` (Task 1.12).
 * Unlike every other command in this file, the Rust side declares
 * `#[tauri::command(rename_all = "camelCase")]`, so its
 * `delete_session_data` parameter crosses the wire as `deleteSessionData`
 * — every other argument here needs no such rename because none of them
 * have a multi-word parameter name.
 */
export async function removeService(id: string, deleteSessionData: boolean): Promise<void> {
  await invoke<void>("remove_service", { id, deleteSessionData });
}

/**
 * `update_settings` — input `{ patch }`, output the updated `Settings`
 * (Task 1.13; design.md §2.2.13's "Global settings: every `[settings]`
 * key."). Only the fields actually being changed should be present on
 * `patch` — an omitted key leaves that field untouched server-side
 * (`SettingsPatchDto`'s `Option<T>` fields deserialize to `None` when the
 * key is absent).
 */
export async function updateSettings(patch: SettingsPatchInput): Promise<Settings> {
  return invoke<Settings>("update_settings", { patch });
}
