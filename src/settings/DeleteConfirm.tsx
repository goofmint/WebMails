/**
 * Inline delete confirmation (Task 1.12; design.md §2.2.13: "Delete
 * confirmation, with the 'delete session data' option shown only for
 * isolated profiles."). `deleteSessionData` defaults to unchecked, and is
 * sent as `false` outright for any non-`"isolated"` profile — there is no
 * checkbox to even show for those.
 */

import { useState } from "react";
import type { CommandError, ServiceConfig, SettingsIpc } from "../ipc";
import { toCommandError } from "../ipc";

export interface DeleteConfirmProps {
  readonly ipc: SettingsIpc;
  readonly service: ServiceConfig;
  readonly onClose: () => void;
}

export function DeleteConfirm({ ipc, service, onClose }: DeleteConfirmProps) {
  const isIsolated = service.profile === "isolated";
  const [deleteSessionData, setDeleteSessionData] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  async function handleConfirm(): Promise<void> {
    setSubmitting(true);
    setError(null);
    try {
      await ipc.removeService(service.id, isIsolated ? deleteSessionData : false);
      onClose();
    } catch (caughtError: unknown) {
      setError(toCommandError(caughtError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="delete-confirm" role="group" aria-label={`Delete ${service.name}`}>
      <p>
        Delete &ldquo;{service.name}&rdquo;? This removes it from the sidebar and its configuration.
      </p>
      {isIsolated && (
        <label>
          <input
            type="checkbox"
            checked={deleteSessionData}
            onChange={(event) => {
              setDeleteSessionData(event.target.checked);
            }}
          />
          Delete session data
        </label>
      )}
      {error !== null && (
        <p className="settings__error" role="alert">
          {error.message}
        </p>
      )}
      <div className="delete-confirm__actions">
        <button
          type="button"
          disabled={submitting}
          onClick={() => {
            void handleConfirm();
          }}
        >
          Delete
        </button>
        <button type="button" onClick={onClose}>
          Cancel
        </button>
      </div>
    </div>
  );
}
