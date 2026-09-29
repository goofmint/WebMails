/**
 * Pure sizing helper for the sidebar (Task #88; design.md §2.2.13). Every
 * pixel size the sidebar and its action buttons use is derived here from
 * `Snapshot.sidebarWidth` (never hard-coded in a component or stylesheet),
 * so the whole rail scales consistently if `sidebarWidth` ever changes.
 *
 * `ICON_SIZE_RATIO` is the single named ratio constant every other metric
 * is derived from: the icon shrinks to a fraction of the sidebar's width
 * (leaving visible padding around it, unlike the old edge-to-edge sizing),
 * and the remaining metrics fall out of that icon size and the leftover
 * space, rather than each introducing its own arbitrary ratio.
 */

import type { CSSProperties } from "react";

/** Icon size as a fraction of `sidebarWidth` — 0.625 × 64 = 40px. */
const ICON_SIZE_RATIO = 0.625;

export interface SidebarMetrics {
  /** `ServiceIcon`'s `size` prop (its button and glyph both scale from this). */
  readonly iconSize: number;
  /** Vertical padding around `.sidebar__list` and bottom padding under `.sidebar__actions`. */
  readonly listPadding: number;
  /** Gap between sidebar list items and between the two action buttons. */
  readonly listGap: number;
  /** Width/height of the "+" and gear buttons. */
  readonly actionButtonSize: number;
  /** Font size of the "+" and gear glyphs. */
  readonly actionGlyphSize: number;
}

/** `sidebarMetrics`'s result as inline CSS custom properties, ready to spread onto a `style` prop. */
export type SidebarMetricsVars = CSSProperties & {
  readonly "--sidebar-icon-size": string;
  readonly "--sidebar-list-padding": string;
  readonly "--sidebar-list-gap": string;
  readonly "--sidebar-action-size": string;
  readonly "--sidebar-action-glyph-size": string;
};

/**
 * Derives every sidebar sizing metric from `sidebarWidth`, all rounded to
 * whole pixels. `sidebarWidth` is always 64 today (Rust's `SIDEBAR_WIDTH`),
 * but nothing here assumes that.
 */
export function sidebarMetrics(sidebarWidth: number): SidebarMetrics {
  const iconSize = Math.round(sidebarWidth * ICON_SIZE_RATIO);
  // Half the space `iconSize` leaves inside `sidebarWidth`, so the icon
  // reads as inset from the rail's edges rather than filling them.
  const listPadding = Math.round((sidebarWidth - iconSize) / 2);
  const listGap = Math.round(listPadding / 2);

  return {
    iconSize,
    listPadding,
    listGap,
    actionButtonSize: iconSize,
    actionGlyphSize: Math.round(iconSize / 2),
  };
}

/** `metrics` as a `--sidebar-*` custom-properties object, each value a `px` string. */
export function sidebarMetricsVars(metrics: SidebarMetrics): SidebarMetricsVars {
  return {
    "--sidebar-icon-size": `${metrics.iconSize}px`,
    "--sidebar-list-padding": `${metrics.listPadding}px`,
    "--sidebar-list-gap": `${metrics.listGap}px`,
    "--sidebar-action-size": `${metrics.actionButtonSize}px`,
    "--sidebar-action-glyph-size": `${metrics.actionGlyphSize}px`,
  };
}
