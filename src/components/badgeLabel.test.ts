import { describe, expect, it } from "vitest";
import { attentionReasonLabel, badgeLabel } from "./badgeLabel";
import type { AttentionReason, ServiceStatus } from "../ipc";

describe("attentionReasonLabel", () => {
  it.each([
    ["reportedNone", "No unread count reported"],
    ["offOrigin", "Page left the service origin"],
    ["createFailed", "Service view could not be created"],
  ] as const)("returns the label for %s", (reason, expected) => {
    expect(attentionReasonLabel(reason)).toBe(expected);
  });

  it("returns null when reason is undefined", () => {
    expect(attentionReasonLabel(undefined)).toBeNull();
  });

  it("returns null for an unknown reason string — the IPC boundary carries no runtime validation", () => {
    // `ServiceStatus.reason` is typed as `AttentionReason | undefined`, but
    // nothing validates the JSON `invoke()` actually returns at runtime, so
    // an unrecognised string must degrade to `null` rather than throw.
    expect(attentionReasonLabel("somethingElse" as AttentionReason)).toBeNull();
  });
});

describe("badgeLabel for needsAttention", () => {
  it.each([
    ["reportedNone", "No unread count reported"],
    ["offOrigin", "Page left the service origin"],
    ["createFailed", "Service view could not be created"],
  ] as const)("prefixes the reason label for %s", (reason, reasonLabel) => {
    const status: ServiceStatus = { kind: "needsAttention", reason };
    expect(badgeLabel(status, true)).toBe(`Needs attention: ${reasonLabel}`);
  });

  it("falls back to plain 'Needs attention' when reason is missing", () => {
    const status: ServiceStatus = { kind: "needsAttention" };
    expect(badgeLabel(status, true)).toBe("Needs attention");
  });

  it("falls back to plain 'Needs attention' when reason is unknown", () => {
    const status: ServiceStatus = {
      kind: "needsAttention",
      reason: "somethingElse" as AttentionReason,
    };
    expect(badgeLabel(status, true)).toBe("Needs attention");
  });
});
