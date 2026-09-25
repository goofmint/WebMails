import { afterEach, describe, expect, it, vi } from "vitest";

describe("agent entry point", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.resetModules();
  });

  it("logs an identifying message when loaded", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});

    await import("./main");

    expect(logSpy).toHaveBeenCalledWith(expect.stringContaining("[eluma-agent]"));
  });
});
