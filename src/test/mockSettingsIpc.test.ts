import { describe, expect, it } from "vitest";
import { createMockSettingsIpc } from "./mockSettingsIpc";
import { snapshot } from "./fixtures";

const INITIAL_SETTINGS = {
  reconcile_interval_seconds: 60,
  notifications: true,
  notification_batch_threshold: 5,
  badge_sidebar: true,
};

describe("createMockSettingsIpc", () => {
  it("applies each updateSettings patch onto the currently stored settings, cumulatively", async () => {
    const ipc = createMockSettingsIpc(snapshot({ settings: INITIAL_SETTINGS }));

    const afterFirst = await ipc.updateSettings({ reconcile_interval_seconds: 120 });
    expect(afterFirst).toEqual({ ...INITIAL_SETTINGS, reconcile_interval_seconds: 120 });

    // The second patch lands on top of the first save's result, not the
    // original snapshot, so `reconcile_interval_seconds` from the first
    // save survives even though this patch doesn't mention it.
    const afterSecond = await ipc.updateSettings({ notification_batch_threshold: 9 });
    expect(afterSecond).toEqual({
      ...INITIAL_SETTINGS,
      reconcile_interval_seconds: 120,
      notification_batch_threshold: 9,
    });
  });

  it("returns the cumulative settings from getSnapshot after one or more updateSettings calls", async () => {
    const ipc = createMockSettingsIpc(snapshot({ settings: INITIAL_SETTINGS }));

    await ipc.updateSettings({ badge_sidebar: false });
    await ipc.updateSettings({ notifications: false });
    const afterSaves = await ipc.getSnapshot();

    expect(afterSaves.settings).toEqual({
      ...INITIAL_SETTINGS,
      badge_sidebar: false,
      notifications: false,
    });
  });
});
