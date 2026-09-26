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
} from "./types";
import type { Snapshot, ServiceStatus } from "./types";
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
