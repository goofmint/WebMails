/** Shared `Snapshot`/`ServiceConfig` fixtures for Task 1.10's tests. */

import type { ServiceConfig, Snapshot } from "../ipc";

export function service(overrides: Partial<ServiceConfig> = {}): ServiceConfig {
  return {
    id: "gmail",
    name: "Gmail",
    url: "https://mail.google.com/",
    profile: "default",
    notifications: true,
    icon: { source: "favicon" },
    ...overrides,
  };
}

export function snapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    settings: {
      reconcile_interval_seconds: 60,
      notifications: true,
      notification_batch_threshold: 5,
      badge_sidebar: true,
    },
    services: [service({ id: "gmail", name: "Gmail" }), service({ id: "icloud", name: "iCloud" })],
    statuses: {},
    sidebarWidth: 64,
    activeServiceId: null,
    ...overrides,
  };
}
