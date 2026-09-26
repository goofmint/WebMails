/**
 * The settings screen's Diagnostics panel (Task 3.3; design.md §2.2.13,
 * SPEC.md §9.4): fetches `get_diagnostics` on mount, on every
 * `services-changed` event, and on a manual "Refresh" click, and renders
 * one row per configured service — status, last report age, stale count
 * and last stale time. Self-contained (it calls the `ipc/commands` and
 * `ipc/events` wrappers directly, unlike `ServiceList`/`AddServiceForm`'s
 * `ipc` prop) since it needs no CRUD dependency injection of its own.
 */

import { useCallback, useEffect, useState } from "react";
import { getDiagnostics } from "../ipc/commands";
import { onServicesChanged } from "../ipc/events";
import { toCommandError } from "../ipc/errors";
import type { CommandError, Diagnostics, ServiceDiagnostic, ServiceStatus } from "../ipc/types";

type DiagnosticsState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly diagnostics: Diagnostics }
  | { readonly status: "error"; readonly error: CommandError };

/**
 * `Xs ago` for a last-report age already given in milliseconds (design.md
 * §2.2.12's `lastReportAgeMs`, computed server-side against its own
 * injected clock — this only formats it, never computes an age from
 * `Date.now()` itself, so it stays deterministic in tests with no fake
 * clock needed).
 */
function formatAge(ms: number): string {
  const seconds = Math.round(ms / 1000);
  return `${seconds}s ago`;
}

/** An absolute, deterministic rendering of a last-stale epoch-ms timestamp. */
function formatTimestamp(ms: number): string {
  return new Date(ms).toISOString();
}

const STATUS_LABEL: Readonly<Record<ServiceStatus["kind"], string>> = {
  loading: "Loading",
  ok: "OK",
  needsAttention: "Needs attention",
  stale: "Stale",
};

function StatusBadge({ status }: { readonly status: ServiceStatus }) {
  const label =
    status.kind === "ok" && status.count !== undefined
      ? `${STATUS_LABEL.ok} (${status.count})`
      : STATUS_LABEL[status.kind];
  return <span className={`diagnostics__badge diagnostics__badge--${status.kind}`}>{label}</span>;
}

function DiagnosticsRow({ row }: { readonly row: ServiceDiagnostic }) {
  return (
    <tr>
      <td>{row.name}</td>
      <td>
        <StatusBadge status={row.status} />
      </td>
      <td>{row.lastReportAgeMs === null ? "Never reported" : formatAge(row.lastReportAgeMs)}</td>
      <td>{row.staleCount}</td>
      <td>{row.lastStaleAt === null ? "Never" : formatTimestamp(row.lastStaleAt)}</td>
    </tr>
  );
}

export function DiagnosticsPanel() {
  const [state, setState] = useState<DiagnosticsState>({ status: "loading" });

  const refresh = useCallback((): Promise<void> => {
    return getDiagnostics().then(
      (diagnostics) => {
        setState({ status: "ready", diagnostics });
      },
      (caughtError: unknown) => {
        setState({ status: "error", error: toCommandError(caughtError) });
      },
    );
  }, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    void refresh();

    onServicesChanged(() => {
      void refresh();
    }).then(
      (unlistenFn) => {
        if (cancelled) {
          unlistenFn();
          return;
        }
        unlisten = unlistenFn;
      },
      (caughtError: unknown) => {
        console.error("DiagnosticsPanel: onServicesChanged failed:", caughtError);
      },
    );

    return () => {
      cancelled = true;
      if (unlisten !== null) {
        unlisten();
      }
    };
  }, [refresh]);

  return (
    <section className="diagnostics">
      <h2>Diagnostics</h2>
      <button type="button" onClick={() => void refresh()}>
        Refresh
      </button>
      {state.status === "loading" && <p data-testid="diagnostics-loading">Loading…</p>}
      {state.status === "error" && (
        <p className="diagnostics__error" role="alert">
          {state.error.message}
        </p>
      )}
      {state.status === "ready" && (
        <table className="diagnostics__table">
          <thead>
            <tr>
              <th>Service</th>
              <th>Status</th>
              <th>Last report</th>
              <th>Stale count</th>
              <th>Last stale</th>
            </tr>
          </thead>
          <tbody>
            {state.diagnostics.services.map((row) => (
              <DiagnosticsRow key={row.serviceId} row={row} />
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
