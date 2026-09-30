/**
 * Pure sizing helper for the sidebar (Task #88; design.md §2.2.13). Every
 * pixel size the sidebar and its action buttons use is derived here from
 * `Snapshot.sidebarWidth` (never hard-coded in a component or stylesheet),
 * so the whole rail scales consistently if `sidebarWidth` ever changes.
 *
 * Every derived size is a ratio of the reference sidebar width (Task #98:
 * 52px) to its target size at that width: the service icons to 24px (Task
 * #93's shrink), the "+"/gear action buttons to 40px (larger than the
 * icons, per #88), and the gap between icons to 12px (Task #98). At 52px
 * that works out to 24 / 40 / 20 / 12 / 6 — icon / action / glyph / gap /
 * padding — with the rail's inner padding falling out of the action-button
 * size and the leftover space, and the glyph out of the action size.
 */

import type { CSSProperties } from "react";

/** Sidebar width the target sizes below are defined at (Task #98: 52px). */
const REFERENCE_WIDTH = 52;

/** Service icon size at `REFERENCE_WIDTH` — 24px. */
const ICON_SIZE_AT_REFERENCE = 24;

/** "+"/gear button size at `REFERENCE_WIDTH` — 40px. */
const ACTION_SIZE_AT_REFERENCE = 40;

/** Gap between sidebar icons at `REFERENCE_WIDTH` — 12px. */
const GAP_AT_REFERENCE = 12;

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
 * whole pixels. `sidebarWidth` is always 52 today (Rust's `SIDEBAR_WIDTH`),
 * but nothing here assumes that. Every ratio multiplies before it divides,
 * so rounding only ever happens once per value.
 */
export function sidebarMetrics(sidebarWidth: number): SidebarMetrics {
  const iconSize = Math.round((sidebarWidth * ICON_SIZE_AT_REFERENCE) / REFERENCE_WIDTH);
  const actionButtonSize = Math.round((sidebarWidth * ACTION_SIZE_AT_REFERENCE) / REFERENCE_WIDTH);
  // Half the space the (larger) action buttons leave inside `sidebarWidth`:
  // the rail's inner padding, unchanged from #88 so only the icons shrink.
  const listPadding = Math.round((sidebarWidth - actionButtonSize) / 2);
  // Independent of `listPadding` (Task #98) — the gap between icons, not
  // derived from the leftover action-button space.
  const listGap = Math.round((sidebarWidth * GAP_AT_REFERENCE) / REFERENCE_WIDTH);

  return {
    iconSize,
    listPadding,
    listGap,
    actionButtonSize,
    actionGlyphSize: Math.round(actionButtonSize / 2),
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
