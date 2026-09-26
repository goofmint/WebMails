import { describe, expect, it } from "vitest";
import { colorForServiceId, fnv1aHash, initialForService } from "./color";

describe("fnv1aHash", () => {
  it("is deterministic for the same input", () => {
    expect(fnv1aHash("gmail-personal")).toBe(fnv1aHash("gmail-personal"));
  });

  it("differs for different inputs (no collision for these fixtures)", () => {
    expect(fnv1aHash("gmail-personal")).not.toBe(fnv1aHash("gmail-work"));
    expect(fnv1aHash("icloud")).not.toBe(fnv1aHash("outlook"));
  });

  it("always returns a non-negative 32-bit integer", () => {
    for (const value of ["", "a", "gmail-personal", "x".repeat(100)]) {
      const hash = fnv1aHash(value);
      expect(hash).toBeGreaterThanOrEqual(0);
      expect(hash).toBeLessThanOrEqual(0xffffffff);
      expect(Number.isInteger(hash)).toBe(true);
    }
  });

  it("matches the known FNV-1a 32-bit hash of the empty string", () => {
    // The FNV-1a offset basis is the hash of the empty string, by
    // definition of the algorithm (no bytes are ever folded in).
    expect(fnv1aHash("")).toBe(0x811c9dc5);
  });
});

describe("colorForServiceId", () => {
  it("is deterministic for the same id", () => {
    expect(colorForServiceId("gmail-personal")).toBe(colorForServiceId("gmail-personal"));
  });

  it("differs for different ids (no collision for these fixtures)", () => {
    expect(colorForServiceId("gmail-personal")).not.toBe(colorForServiceId("gmail-work"));
  });

  it("uses a fixed saturation and lightness, varying only the hue", () => {
    const a = colorForServiceId("alpha");
    const b = colorForServiceId("beta");
    expect(a).toMatch(/^hsl\(\d+, 65%, 38%\)$/);
    expect(b).toMatch(/^hsl\(\d+, 65%, 38%\)$/);
  });

  it("produces a hue within 0-359", () => {
    const match = /^hsl\((\d+), /.exec(colorForServiceId("gmail-personal"));
    expect(match).not.toBeNull();
    const hue = Number(match?.[1]);
    expect(hue).toBeGreaterThanOrEqual(0);
    expect(hue).toBeLessThan(360);
  });
});

describe("initialForService", () => {
  it("returns the upper-cased first character of the name", () => {
    expect(initialForService("Gmail", "gmail-personal")).toBe("G");
  });

  it("falls back to the service id when the name is empty", () => {
    expect(initialForService("", "icloud")).toBe("I");
  });

  it("falls back to the service id when the name is only whitespace", () => {
    expect(initialForService("   ", "outlook")).toBe("O");
  });

  it("handles a name starting with a surrogate-pair character as one character", () => {
    expect(initialForService("😀mail", "gmail")).toBe("😀");
  });

  it("returns a placeholder when both name and id are empty", () => {
    expect(initialForService("", "")).toBe("?");
  });
});
