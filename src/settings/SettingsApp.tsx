/**
 * The `settings` window's root component (Task 1.12; design.md §2.2.13).
 * Fetches `get_snapshot` on mount and re-fetches on every `services-changed`
 * event, exactly like the shell store (Task 1.10) but without the
 * shell-only `selectedId`/`select`/`reorder`/`openSettings` concerns —
 * this window never needs them. `configError` hides the CRUD forms
 * entirely, matching the shell's own read-only `ConfigErrorScreen`.
 */

import { useEffect, useState } from "react";
import type { CommandError, SettingsIpc, Snapshot } from "../ipc";
import { toCommandError } from "../ipc";
import { ServiceList } from "./ServiceList";
import { AddServiceForm } from "./AddServiceForm";
import "./settings.css";

export interface SettingsAppProps {
  readonly ipc: SettingsIpc;
}

type SettingsState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly snapshot: Snapshot }
  | { readonly status: "error"; readonly error: CommandError };

export function SettingsApp({ ipc }: SettingsAppProps) {
  const [state, setState] = useState<SettingsState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    async function refresh(): Promise<void> {
      try {
        const snapshot = await ipc.getSnapshot();
        if (!cancelled) {
          setState({ status: "ready", snapshot });
        }
      } catch (caughtError: unknown) {
        if (!cancelled) {
          setState({ status: "error", error: toCommandError(caughtError) });
        }
      }
    }

    void refresh();
    // Registered with an explicit onFulfilled/onRejected pair (not
    // `void`/`.catch`) so a registration failure is handled here rather
    // than becoming an unhandled rejection — mirroring how
    // `createShellStore`'s `bootstrap` uses `Promise.allSettled` for the
    // same reason.
    ipc
      .onServicesChanged(() => {
        void refresh();
      })
      .then(
        (unlistenFn) => {
          if (cancelled) {
            unlistenFn();
            return;
          }
          unlisten = unlistenFn;
        },
        (caughtError: unknown) => {
          console.error("onServicesChanged failed:", caughtError);
        },
      );

    return () => {
      cancelled = true;
      if (unlisten !== null) {
        unlisten();
      }
    };
  }, [ipc]);

  if (state.status === "loading") {
    return (
      <div className="settings" data-testid="settings-loading">
        Loading…
      </div>
    );
  }

  if (state.status === "error") {
    return (
      <div className="settings settings__error" role="alert">
        {state.error.message}
      </div>
    );
  }

  const { snapshot } = state;

  if (snapshot.configError !== undefined) {
    const { configError } = snapshot;
    return (
      <div className="settings settings__error" role="alert">
        <p>{configError.file}</p>
        {configError.key !== null && <p>{configError.key}</p>}
        <p>{configError.reason}</p>
      </div>
    );
  }

  return (
    <div className="settings">
      <h1>Settings</h1>
      <ServiceList ipc={ipc} services={snapshot.services} />
      <AddServiceForm ipc={ipc} services={snapshot.services} />
    </div>
  );
}
