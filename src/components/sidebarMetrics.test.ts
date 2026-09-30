import { describe, expect, it } from "vitest";
import { sidebarMetrics } from "./sidebarMetrics";

describe("sidebarMetrics", () => {
  it("produces the reference sizes at the default 52px sidebarWidth", () => {
    const metrics = sidebarMetrics(52);
    expect(metrics.iconSize).toBe(24);
    expect(metrics.iconSize).toBeLessThan(52);
    expect(metrics.listPadding).toBeGreaterThan(0);
    expect(metrics.listPadding).toBe(6);
    expect(metrics.listGap).toBe(12);
    expect(metrics.actionButtonSize).toBe(40);
    expect(metrics.actionGlyphSize).toBe(20);
  });

  it("scales proportionally at a different sidebarWidth (64)", () => {
    const metrics = sidebarMetrics(64);
    expect(metrics.iconSize).toBe(30);
    expect(metrics.listPadding).toBe(8);
    expect(metrics.listGap).toBe(15);
    expect(metrics.actionButtonSize).toBe(49);
    expect(metrics.actionGlyphSize).toBe(25);
  });

  it("scales proportionally at a different sidebarWidth (80)", () => {
    const metrics = sidebarMetrics(80);
    expect(metrics.iconSize).toBe(37);
    expect(metrics.listPadding).toBe(9);
    expect(metrics.listGap).toBe(18);
    expect(metrics.actionButtonSize).toBe(62);
    expect(metrics.actionGlyphSize).toBe(31);
  });

  it("keeps the action buttons larger than the service icons", () => {
    for (const width of [52, 64, 80]) {
      const metrics = sidebarMetrics(width);
      expect(metrics.actionButtonSize).toBeGreaterThan(metrics.iconSize);
    }
  });

  it("returns only integer px values, at every width", () => {
    for (const width of [52, 64, 80]) {
      const metrics = sidebarMetrics(width);
      for (const value of Object.values(metrics)) {
        expect(Number.isInteger(value)).toBe(true);
      }
    }
  });

  it("keeps list padding positive and at least 2px, at every width", () => {
    for (const width of [52, 64, 80]) {
      const metrics = sidebarMetrics(width);
      expect(metrics.listPadding).toBeGreaterThan(0);
      expect(metrics.listPadding).toBeGreaterThanOrEqual(2);
    }
  });
});
