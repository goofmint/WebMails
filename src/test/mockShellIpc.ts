/**
 * A mock `ShellIpc` for component/store tests (Task 1.10): commands are
 * `vi.fn()`s a test can assert on or reconfigure, and each event can be
 * fired manually with `emitServicesChanged`/`emitSelectService`/
 * `emitStatusChanged`.
 */

import { vi, type Mock } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type { ServiceStatus, ShellIpc, Snapshot } from "../ipc";

export interface MockShellIpc extends ShellIpc {
  readonly getSnapshot: Mock<() => Promise<Snapshot>>;
  readonly selectService: Mock<(id: string) => Promise<void>>;
  readonly reorderServices: Mock<(ids: readonly string[]) => Promise<void>>;
  readonly openSettings: Mock<() => Promise<void>>;
  emitServicesChanged(): void;
  emitSelectService(id: string): void;
  emitStatusChanged(serviceId: string, status: ServiceStatus): void;
  emitServiceIconChanged(id: string): void;
}

/** Creates a mock `ShellIpc` whose `getSnapshot` initially resolves to `initialSnapshot`. */
export function createMockShellIpc(initialSnapshot: Snapshot): MockShellIpc {
  let servicesChangedListeners: (() => void)[] = [];
  let selectServiceListeners: ((payload: { readonly id: string }) => void)[] = [];
  let statusChangedListeners: ((payload: {
    readonly serviceId: string;
    readonly status: ServiceStatus;
  }) => void)[] = [];
  let serviceIconChangedListeners: ((payload: { readonly id: string }) => void)[] = [];

  const getSnapshot = vi.fn((): Promise<Snapshot> => Promise.resolve(initialSnapshot));
  const selectService = vi.fn((): Promise<void> => Promise.resolve());
  const reorderServices = vi.fn((): Promise<void> => Promise.resolve());
  const openSettings = vi.fn((): Promise<void> => Promise.resolve());

  function onServicesChanged(callback: () => void): Promise<UnlistenFn> {
    servicesChangedListeners.push(callback);
    return Promise.resolve(() => {
      servicesChangedListeners = servicesChangedListeners.filter(
        (listener) => listener !== callback,
      );
    });
  }

  function onSelectService(
    callback: (payload: { readonly id: string }) => void,
  ): Promise<UnlistenFn> {
    selectServiceListeners.push(callback);
    return Promise.resolve(() => {
      selectServiceListeners = selectServiceListeners.filter((listener) => listener !== callback);
    });
  }

  function onStatusChanged(
    callback: (payload: { readonly serviceId: string; readonly status: ServiceStatus }) => void,
  ): Promise<UnlistenFn> {
    statusChangedListeners.push(callback);
    return Promise.resolve(() => {
      statusChangedListeners = statusChangedListeners.filter((listener) => listener !== callback);
    });
  }

  function onServiceIconChanged(
    callback: (payload: { readonly id: string }) => void,
  ): Promise<UnlistenFn> {
    serviceIconChangedListeners.push(callback);
    return Promise.resolve(() => {
      serviceIconChangedListeners = serviceIconChangedListeners.filter(
        (listener) => listener !== callback,
      );
    });
  }

  return {
    getSnapshot,
    selectService,
    reorderServices,
    openSettings,
    onServicesChanged,
    onSelectService,
    onStatusChanged,
    onServiceIconChanged,
    emitServicesChanged() {
      for (const listener of servicesChangedListeners) {
        listener();
      }
    },
    emitSelectService(id: string) {
      for (const listener of selectServiceListeners) {
        listener({ id });
      }
    },
    emitStatusChanged(serviceId: string, status: ServiceStatus) {
      for (const listener of statusChangedListeners) {
        listener({ serviceId, status });
      }
    },
    emitServiceIconChanged(id: string) {
      for (const listener of serviceIconChangedListeners) {
        listener({ id });
      }
    },
  };
}
