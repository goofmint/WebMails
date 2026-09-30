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

import type { AttentionReason, ServiceStatus } from "../ipc";

/**
 * The human-readable label for each `AttentionReason` (Rust
 * `src-tauri/src/unread/status.rs`) — the only place these strings are
 * defined. Both `badgeLabel` (below) and `DiagnosticsPanel`'s
 * `StatusBadge` read this via `attentionReasonLabel` rather than each
 * keeping their own copy.
 */
const ATTENTION_REASON_LABEL: Readonly<Record<AttentionReason, string>> = {
  reportedNone: "No unread count reported",
  offOrigin: "Page left the service origin",
  createFailed: "Service view could not be created",
};

/**
 * `reason`'s label, or `null` when it's `undefined` or not one of the
 * known `AttentionReason` values. `ServiceStatus.reason` crosses the IPC
 * boundary with no runtime validation, so an unrecognised string must
 * degrade to `null` rather than throw or render `undefined`.
 */
export function attentionReasonLabel(reason: AttentionReason | undefined): string | null {
  if (reason === undefined) {
    return null;
  }
  return Object.prototype.hasOwnProperty.call(ATTENTION_REASON_LABEL, reason)
    ? ATTENTION_REASON_LABEL[reason]
    : null;
}

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

    case "needsAttention": {
      const reasonLabel = attentionReasonLabel(status.reason);
      return reasonLabel === null ? "Needs attention" : `Needs attention: ${reasonLabel}`;
    }
  }
}
