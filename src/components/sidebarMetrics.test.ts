import { describe, expect, it } from "vitest";
import { sidebarMetrics } from "./sidebarMetrics";

describe("sidebarMetrics", () => {
  it("shrinks the icon below sidebarWidth, leaving positive list padding, at 64", () => {
    const metrics = sidebarMetrics(64);
    expect(metrics.iconSize).toBe(40);
    expect(metrics.iconSize).toBeLessThan(64);
    expect(metrics.listPadding).toBeGreaterThan(0);
  });

  it("scales proportionally at a different sidebarWidth", () => {
    const metrics = sidebarMetrics(80);
    expect(metrics.iconSize).toBe(50);
    expect(metrics.listPadding).toBe(15);
    expect(metrics.actionButtonSize).toBe(50);
    expect(metrics.actionGlyphSize).toBe(25);
  });

  it("returns only integer px values, at every width", () => {
    for (const width of [64, 80]) {
      const metrics = sidebarMetrics(width);
      for (const value of Object.values(metrics)) {
        expect(Number.isInteger(value)).toBe(true);
      }
    }
  });
});
