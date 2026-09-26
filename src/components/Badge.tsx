/**
 * Sidebar status indicator for one service (Task 2.10; design.md §2.2.13,
 * §11.3; SPEC.md §11.3). Renders nothing when `enabled` is `false`
 * (`settings.badge_sidebar`), when `status.kind` is `"loading"`, or when an
 * `"ok"` status carries a zero (or absent) count. Otherwise renders exactly
 * one of: a numeric pill capped at `"999+"`, a hollow grey dot (`"stale"`),
 * or a warning glyph (`"needsAttention"`, the same glyph for every
 * `reason`).
 *
 * Purely presentational: it takes a `ServiceStatus` and a boolean, with no
 * dependency on `ShellIpc` or the store, so it can be rendered and tested on
 * its own (`Badge.test.tsx`) and wired into `ServiceIcon` for the sidebar.
 */

import type { ServiceStatus } from "../ipc";
import { badgeLabel } from "./badgeLabel";
import "./Badge.css";

/** The single place the sidebar's unread-count pill cap is defined. */
export const BADGE_COUNT_CAP = 999;

export interface BadgeProps {
  readonly status: ServiceStatus;
  /** `settings.badge_sidebar` — `false` suppresses every indicator below. */
  readonly enabled: boolean;
}

/** `n` as its pill text, capped at `` `${BADGE_COUNT_CAP}+` ``. */
function formatPillText(count: number): string {
  return count > BADGE_COUNT_CAP ? `${BADGE_COUNT_CAP}+` : String(count);
}

export function Badge({ status, enabled }: BadgeProps) {
  const label = badgeLabel(status, enabled);
  if (label === null) {
    return null;
  }

  switch (status.kind) {
    case "ok": {
      const { count } = status;
      if (count === undefined || count <= 0) {
        // Unreachable: badgeLabel() already returned null for this case.
        return null;
      }
      return (
        <span className="badge badge--pill" aria-label={label} title={label}>
          {formatPillText(count)}
        </span>
      );
    }

    case "stale":
      return <span className="badge badge--stale" aria-label={label} title={label} />;

    case "needsAttention":
      return (
        <span className="badge badge--attention" aria-label={label} title={label}>
          ⚠
        </span>
      );

    case "loading":
      // Unreachable: badgeLabel() already returned null for this case.
      return null;
  }
}
