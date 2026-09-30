import { describe, expect, it } from "vitest";
import { sidebarMetrics } from "./sidebarMetrics";

describe("sidebarMetrics", () => {
  it("shrinks the icon below sidebarWidth, leaving positive list padding, at 64", () => {
    const metrics = sidebarMetrics(64);
    expect(metrics.iconSize).toBe(24);
    expect(metrics.iconSize).toBeLessThan(64);
    expect(metrics.listPadding).toBeGreaterThan(0);
    expect(metrics.listPadding).toBe(12);
    expect(metrics.listGap).toBe(6);
    expect(metrics.actionButtonSize).toBe(40);
    expect(metrics.actionGlyphSize).toBe(20);
  });

  it("scales proportionally at a different sidebarWidth", () => {
    const metrics = sidebarMetrics(80);
    expect(metrics.iconSize).toBe(30);
    expect(metrics.listPadding).toBe(15);
    expect(metrics.listGap).toBe(8);
    expect(metrics.actionButtonSize).toBe(50);
    expect(metrics.actionGlyphSize).toBe(25);
  });

  it("keeps the action buttons larger than the service icons", () => {
    for (const width of [64, 80]) {
      const metrics = sidebarMetrics(width);
      expect(metrics.actionButtonSize).toBeGreaterThan(metrics.iconSize);
    }
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
