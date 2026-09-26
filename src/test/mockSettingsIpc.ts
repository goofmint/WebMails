/**
 * A mock `SettingsIpc` for the settings screen's component tests (Task
 * 1.12), mirroring `mockShellIpc.ts`'s shape: commands are `vi.fn()`s a
 * test can assert on or reconfigure (e.g. `mockRejectedValueOnce({ kind,
 * message })` to simulate a command failure), and `emitServicesChanged`
 * fires the one event `SettingsIpc` subscribes to.
 */

import { vi, type Mock } from "vitest";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  ServiceConfig,
  ServicePatchInput,
  Settings,
  SettingsIpc,
  SettingsPatchInput,
  Snapshot,
} from "../ipc";

export interface MockSettingsIpc extends SettingsIpc {
  readonly getSnapshot: Mock<() => Promise<Snapshot>>;
  readonly addService: Mock<(name: string, url: string, profile: string) => Promise<ServiceConfig>>;
  readonly updateService: Mock<(id: string, patch: ServicePatchInput) => Promise<ServiceConfig>>;
  readonly removeService: Mock<(id: string, deleteSessionData: boolean) => Promise<void>>;
  readonly updateSettings: Mock<(patch: SettingsPatchInput) => Promise<Settings>>;
  readonly selectService: Mock<(id: string) => Promise<void>>;
  emitServicesChanged(): void;
}

/** Creates a mock `SettingsIpc` whose `getSnapshot` initially resolves to `initialSnapshot`. */
export function createMockSettingsIpc(initialSnapshot: Snapshot): MockSettingsIpc {
  let servicesChangedListeners: (() => void)[] = [];

  const getSnapshot = vi.fn((): Promise<Snapshot> => Promise.resolve(initialSnapshot));

  const addService = vi.fn((name: string, url: string, profile: string): Promise<ServiceConfig> =>
    Promise.resolve({
      id: "new-service",
      name,
      url,
      profile,
      notifications: true,
      icon: { source: "favicon" },
    }),
  );

  const updateService = vi.fn((id: string, patch: ServicePatchInput): Promise<ServiceConfig> =>
    Promise.resolve({
      id,
      name: patch.name ?? "Service",
      url: patch.url ?? "https://example.com/",
      profile: patch.profile ?? "default",
      notifications: patch.notifications ?? true,
      icon: { source: "favicon" },
    }),
  );

  const removeService = vi.fn((): Promise<void> => Promise.resolve());

  const updateSettings = vi.fn((patch: SettingsPatchInput): Promise<Settings> => {
    const current = initialSnapshot.settings;
    return Promise.resolve({
      reconcile_interval_seconds:
        patch.reconcile_interval_seconds ?? current?.reconcile_interval_seconds ?? 60,
      notifications: patch.notifications ?? current?.notifications ?? true,
      notification_batch_threshold:
        patch.notification_batch_threshold ?? current?.notification_batch_threshold ?? 5,
      badge_sidebar: patch.badge_sidebar ?? current?.badge_sidebar ?? true,
    });
  });

  const selectService = vi.fn((): Promise<void> => Promise.resolve());

  function onServicesChanged(callback: () => void): Promise<UnlistenFn> {
    servicesChangedListeners.push(callback);
    return Promise.resolve(() => {
      servicesChangedListeners = servicesChangedListeners.filter(
        (listener) => listener !== callback,
      );
    });
  }

  return {
    getSnapshot,
    addService,
    updateService,
    removeService,
    updateSettings,
    selectService,
    onServicesChanged,
    emitServicesChanged() {
      for (const listener of servicesChangedListeners) {
        listener();
      }
    },
  };
}
