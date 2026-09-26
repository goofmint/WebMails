/**
 * `ShellIpc`: the shell UI's whole view of the backend (Task 1.10) —
 * `src/store/shellStore.ts` depends on this interface, not on
 * `@tauri-apps/api` directly, so tests can swap in
 * `src/test/mockShellIpc.ts` instead of a real Tauri runtime.
 */

import * as commands from "./commands";
import * as events from "./events";

export type {
  Settings,
  ServiceConfig,
  IconSource,
  ConfigErrorInfo,
  ServiceStatus,
  Snapshot,
  ServicePatchInput,
  CommandError,
  ServiceDiagnostic,
  Diagnostics,
} from "./types";
export { toCommandError } from "./errors";
import type { Snapshot, ServiceStatus, ServiceConfig, ServicePatchInput } from "./types";
import type { UnlistenFn } from "@tauri-apps/api/event";

export interface ShellIpc {
  getSnapshot(): Promise<Snapshot>;
  selectService(id: string): Promise<void>;
  reorderServices(ids: readonly string[]): Promise<void>;
  openSettings(): Promise<void>;
  onServicesChanged(callback: () => void): Promise<UnlistenFn>;
  onSelectService(callback: (payload: { readonly id: string }) => void): Promise<UnlistenFn>;
  onStatusChanged(
    callback: (payload: { readonly serviceId: string; readonly status: ServiceStatus }) => void,
  ): Promise<UnlistenFn>;
}

/** The real, `invoke`/`listen`-backed implementation, used by `App.tsx`. */
export const shellIpc: ShellIpc = {
  getSnapshot: commands.getSnapshot,
  selectService: commands.selectService,
  reorderServices: commands.reorderServices,
  openSettings: commands.openSettings,
  onServicesChanged: events.onServicesChanged,
  onSelectService: events.onSelectService,
  onStatusChanged: events.onStatusChanged,
};

/**
 * `SettingsIpc`: the settings window's whole view of the backend (Task
 * 1.12) — a different subset of the same command/event surface `ShellIpc`
 * wraps (design.md §2.2.12, §2.2.13): the three CRUD commands plus
 * `getSnapshot`/`selectService`/`onServicesChanged`, reused unchanged from
 * the same underlying `./commands`/`./events` wrappers `ShellIpc` uses.
 * `reorderServices`/`openSettings`/`onSelectService`/`onStatusChanged` are
 * shell-only and stay out of this interface.
 */
export interface SettingsIpc {
  getSnapshot(): Promise<Snapshot>;
  addService(name: string, url: string, profile: string): Promise<ServiceConfig>;
  updateService(id: string, patch: ServicePatchInput): Promise<ServiceConfig>;
  removeService(id: string, deleteSessionData: boolean): Promise<void>;
  selectService(id: string): Promise<void>;
  onServicesChanged(callback: () => void): Promise<UnlistenFn>;
}

/** The real, `invoke`/`listen`-backed implementation, used by `App.tsx` for `#/settings`. */
export const settingsIpc: SettingsIpc = {
  getSnapshot: commands.getSnapshot,
  addService: commands.addService,
  updateService: commands.updateService,
  removeService: commands.removeService,
  selectService: commands.selectService,
  onServicesChanged: events.onServicesChanged,
};
