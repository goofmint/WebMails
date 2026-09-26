/**
 * Global settings form (Task 1.13; design.md §2.2.13: "Global settings:
 * every `[settings]` key."). Edits `reconcile_interval_seconds`,
 * `notifications`, `notification_batch_threshold` and `badge_sidebar`
 * (SPEC §6) and submits only the fields the user actually changed, as
 * `update_settings`'s `patch` — the same "diff against the saved value"
 * shape `EditServiceForm` uses for services.
 *
 * The two numeric fields are kept as strings while being edited (so a
 * user can clear the field or type a partial number without it snapping
 * back), validated with `parseU32` (0..=4294967295, matching the Rust
 * `u32` field) before being included in the patch — an invalid value blocks
 * submission and shows an inline error instead of being sent.
 */

import { useState, type FormEvent } from "react";
import type { CommandError, Settings, SettingsIpc, SettingsPatchInput } from "../ipc";
import { toCommandError } from "../ipc";
import { parseU32 } from "./helpers";

export interface GlobalSettingsFormProps {
  readonly ipc: SettingsIpc;
  readonly settings: Settings;
}

const NUMBER_RANGE_MESSAGE = "Enter a whole number between 0 and 4294967295.";

export function GlobalSettingsForm({ ipc, settings }: GlobalSettingsFormProps) {
  // `saved` is the last value known to match `config.toml` — the baseline
  // the in-progress edits are diffed against. It starts as the snapshot's
  // `settings` and is only ever replaced by `update_settings`'s own return
  // value on a successful save (never by a guess), exactly like
  // `EditServiceForm`'s `service` prop plays that role for one service.
  const [saved, setSaved] = useState(settings);
  const [reconcileIntervalSeconds, setReconcileIntervalSeconds] = useState(
    String(settings.reconcile_interval_seconds),
  );
  const [notifications, setNotifications] = useState(settings.notifications);
  const [notificationBatchThreshold, setNotificationBatchThreshold] = useState(
    String(settings.notification_batch_threshold),
  );
  const [badgeSidebar, setBadgeSidebar] = useState(settings.badge_sidebar);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [justSaved, setJustSaved] = useState(false);

  const parsedReconcileIntervalSeconds = parseU32(reconcileIntervalSeconds);
  const parsedNotificationBatchThreshold = parseU32(notificationBatchThreshold);
  const reconcileIntervalSecondsIsValid = parsedReconcileIntervalSeconds !== null;
  const notificationBatchThresholdIsValid = parsedNotificationBatchThreshold !== null;

  // `SettingsPatchInput`'s fields are `readonly`, so — like
  // `EditServiceForm`'s `patch` — this is assembled by conditional
  // spreading (each key set once, at construction) rather than by
  // mutating a `{}` literal after the fact.
  const patch: SettingsPatchInput = {
    ...(reconcileIntervalSecondsIsValid &&
    parsedReconcileIntervalSeconds !== saved.reconcile_interval_seconds
      ? { reconcile_interval_seconds: parsedReconcileIntervalSeconds }
      : {}),
    ...(notifications !== saved.notifications ? { notifications } : {}),
    ...(notificationBatchThresholdIsValid &&
    parsedNotificationBatchThreshold !== saved.notification_batch_threshold
      ? { notification_batch_threshold: parsedNotificationBatchThreshold }
      : {}),
    ...(badgeSidebar !== saved.badge_sidebar ? { badge_sidebar: badgeSidebar } : {}),
  };
  const hasChanges = Object.keys(patch).length > 0;
  const canSubmit =
    hasChanges &&
    reconcileIntervalSecondsIsValid &&
    notificationBatchThresholdIsValid &&
    !submitting;

  async function handleSubmit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    setSubmitting(true);
    setError(null);
    setJustSaved(false);
    try {
      const updated = await ipc.updateSettings(patch);
      setSaved(updated);
      setReconcileIntervalSeconds(String(updated.reconcile_interval_seconds));
      setNotifications(updated.notifications);
      setNotificationBatchThreshold(String(updated.notification_batch_threshold));
      setBadgeSidebar(updated.badge_sidebar);
      setJustSaved(true);
    } catch (caughtError: unknown) {
      // The in-progress values are deliberately left untouched here, so
      // the user's edit is not lost on a failed save (matching
      // `EditServiceForm`'s behaviour).
      setError(toCommandError(caughtError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="global-settings-form"
      aria-label="Global settings"
      onSubmit={(event) => {
        void handleSubmit(event);
      }}
    >
      <h2>Global settings</h2>

      <label htmlFor="global-settings-reconcile-interval-seconds">
        Reconcile interval (seconds)
      </label>
      <input
        id="global-settings-reconcile-interval-seconds"
        type="text"
        inputMode="numeric"
        value={reconcileIntervalSeconds}
        onChange={(event) => {
          setReconcileIntervalSeconds(event.target.value);
          setJustSaved(false);
        }}
      />
      {!reconcileIntervalSecondsIsValid && (
        <p className="global-settings-form__error" role="alert">
          {NUMBER_RANGE_MESSAGE}
        </p>
      )}

      <label>
        <input
          type="checkbox"
          checked={notifications}
          onChange={(event) => {
            setNotifications(event.target.checked);
            setJustSaved(false);
          }}
        />
        Notifications
      </label>

      <label htmlFor="global-settings-notification-batch-threshold">
        Notification batch threshold
      </label>
      <input
        id="global-settings-notification-batch-threshold"
        type="text"
        inputMode="numeric"
        value={notificationBatchThreshold}
        onChange={(event) => {
          setNotificationBatchThreshold(event.target.value);
          setJustSaved(false);
        }}
      />
      {!notificationBatchThresholdIsValid && (
        <p className="global-settings-form__error" role="alert">
          {NUMBER_RANGE_MESSAGE}
        </p>
      )}

      <label>
        <input
          type="checkbox"
          checked={badgeSidebar}
          onChange={(event) => {
            setBadgeSidebar(event.target.checked);
            setJustSaved(false);
          }}
        />
        Badge sidebar
      </label>

      {error !== null && (
        <p className="settings__error" role="alert">
          {error.message}
        </p>
      )}

      {justSaved && error === null && <p className="global-settings-form__success">Saved.</p>}

      <button type="submit" disabled={!canSubmit}>
        Save
      </button>
    </form>
  );
}
