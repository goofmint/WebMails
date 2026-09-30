/**
 * Pure sizing helper for the sidebar (Task #88; design.md §2.2.13). Every
 * pixel size the sidebar and its action buttons use is derived here from
 * `Snapshot.sidebarWidth` (never hard-coded in a component or stylesheet),
 * so the whole rail scales consistently if `sidebarWidth` ever changes.
 *
 * Two named ratios drive everything: `ICON_SIZE_RATIO` sizes the service
 * icons (Task #93 shrank them to 60% of #88's 40px, per the owner), and
 * `ACTION_SIZE_RATIO` sizes the "+"/gear buttons independently, since the
 * owner asked for those to be *larger* in #88. The rail's inner padding and
 * gap fall out of the action-button size and the leftover space.
 */

import type { CSSProperties } from "react";

/** Service icon size as a fraction of `sidebarWidth` — 0.375 × 64 = 24px. */
const ICON_SIZE_RATIO = 0.375;

/** "+"/gear button size as a fraction of `sidebarWidth` — 0.625 × 64 = 40px. */
const ACTION_SIZE_RATIO = 0.625;

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
  const actionButtonSize = Math.round(sidebarWidth * ACTION_SIZE_RATIO);
  // Half the space the (larger) action buttons leave inside `sidebarWidth`:
  // the rail's inner padding, unchanged from #88 so only the icons shrink.
  const listPadding = Math.round((sidebarWidth - actionButtonSize) / 2);
  const listGap = Math.round(listPadding / 2);

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
