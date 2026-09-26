import { describe, expect, it } from "vitest";
import { isSettingsRoute } from "./route";

describe("isSettingsRoute", () => {
  it("is true for the bare #/settings hash", () => {
    expect(isSettingsRoute("#/settings")).toBe(true);
  });

  it("is true for a #/settings hash with a sub-path", () => {
    expect(isSettingsRoute("#/settings/general")).toBe(true);
  });

  it("is false for the empty hash (shell route)", () => {
    expect(isSettingsRoute("")).toBe(false);
  });

  it("is false for an unrelated hash", () => {
    expect(isSettingsRoute("#/other")).toBe(false);
  });
});
