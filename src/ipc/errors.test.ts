import { describe, expect, it } from "vitest";
import { toCommandError } from "./errors";

describe("toCommandError", () => {
  it("returns a { kind, message } rejection unchanged", () => {
    const rejection = { kind: "config", message: "invalid profile name" };
    expect(toCommandError(rejection)).toEqual(rejection);
  });

  it("falls back to an 'unknown' kind for an Error instance", () => {
    const result = toCommandError(new Error("boom"));
    expect(result.kind).toBe("unknown");
    expect(result.message).toContain("boom");
  });

  it("falls back to an 'unknown' kind for a plain string", () => {
    const result = toCommandError("nope");
    expect(result).toEqual({ kind: "unknown", message: "nope" });
  });

  it("falls back to an 'unknown' kind for an object missing message", () => {
    const result = toCommandError({ kind: "config" });
    expect(result.kind).toBe("unknown");
  });

  it("falls back to an 'unknown' kind for null", () => {
    const result = toCommandError(null);
    expect(result.kind).toBe("unknown");
  });
});
