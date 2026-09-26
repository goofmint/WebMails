/**
 * `Badge`'s own rendering rule (Task 2.10; design.md §11.3), factored out
 * as a pure function so `ServiceIcon`'s `aria-label` can share it instead
 * of re-deriving its own copy of "does Badge render, and what does it
 * say" — which could otherwise drift from `Badge`'s actual behavior.
 *
 * Kept in its own module (rather than exported from `Badge.tsx` itself)
 * so this file only exports the one component, satisfying
 * `react-refresh/only-export-components`.
 */

import type { ServiceStatus } from "../ipc";

/**
 * The label `Badge` renders for `status` when `enabled` — its `aria-label`
 * and `title` — or `null` when `Badge` renders nothing at all: `enabled`
 * is `false`, `status.kind` is `"loading"`, or an `"ok"` status carries a
 * zero (or absent) count.
 */
export function badgeLabel(status: ServiceStatus, enabled: boolean): string | null {
  if (!enabled) {
    return null;
  }

  switch (status.kind) {
    case "loading":
      return null;

    case "ok":
      return status.count === undefined || status.count <= 0 ? null : `${status.count} unread`;

    case "stale":
      return "Not updating";

    case "needsAttention":
      return "Needs attention";
  }
}
