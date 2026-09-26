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

import { useId, useState, type FormEvent } from "react";
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
  // `settings`, is kept in sync with the `settings` prop (the effect
  // below), and is also replaced by `update_settings`'s own return value
  // on a successful save, exactly like `EditServiceForm`'s `service` prop
  // plays that role for one service.
  const [saved, setSaved] = useState(settings);
  const [reconcileIntervalSeconds, setReconcileIntervalSeconds] = useState(
    String(settings.reconcile_interval_seconds),
  );
  const [reconcileIntervalSecondsDirty, setReconcileIntervalSecondsDirty] = useState(false);
  const [notifications, setNotifications] = useState(settings.notifications);
  const [notificationsDirty, setNotificationsDirty] = useState(false);
  const [notificationBatchThreshold, setNotificationBatchThreshold] = useState(
    String(settings.notification_batch_threshold),
  );
  const [notificationBatchThresholdDirty, setNotificationBatchThresholdDirty] = useState(false);
  const [badgeSidebar, setBadgeSidebar] = useState(settings.badge_sidebar);
  const [badgeSidebarDirty, setBadgeSidebarDirty] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [justSaved, setJustSaved] = useState(false);

  const reconcileIntervalSecondsErrorId = useId();
  const notificationBatchThresholdErrorId = useId();

  // When the `settings` prop changes (e.g. a snapshot re-fetched after
  // another window edited the same config), re-baseline `saved` against it
  // and pull the new value into any field the user hasn't touched yet. A
  // field the user has already edited (its `*Dirty` flag) keeps the user's
  // in-progress value. This adjusts state during render (React's pattern
  // for deriving state from a changed prop) instead of in an effect, which
  // would render twice per change.
  const [previousSettings, setPreviousSettings] = useState(settings);
  if (settings !== previousSettings) {
    setPreviousSettings(settings);
    setSaved(settings);
    if (!reconcileIntervalSecondsDirty) {
      setReconcileIntervalSeconds(String(settings.reconcile_interval_seconds));
    }
    if (!notificationsDirty) {
      setNotifications(settings.notifications);
    }
    if (!notificationBatchThresholdDirty) {
      setNotificationBatchThreshold(String(settings.notification_batch_threshold));
    }
    if (!badgeSidebarDirty) {
      setBadgeSidebar(settings.badge_sidebar);
    }
  }

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
      setReconcileIntervalSecondsDirty(false);
      setNotifications(updated.notifications);
      setNotificationsDirty(false);
      setNotificationBatchThreshold(String(updated.notification_batch_threshold));
      setNotificationBatchThresholdDirty(false);
      setBadgeSidebar(updated.badge_sidebar);
      setBadgeSidebarDirty(false);
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
        // Disabled while a save is in flight: otherwise a keystroke made
        // before `updateSettings` resolves would be clobbered when the
        // success handler resets every field to its returned value.
        disabled={submitting}
        aria-required="true"
        aria-invalid={!reconcileIntervalSecondsIsValid}
        aria-describedby={
          reconcileIntervalSecondsIsValid ? undefined : reconcileIntervalSecondsErrorId
        }
        onChange={(event) => {
          setReconcileIntervalSeconds(event.target.value);
          setReconcileIntervalSecondsDirty(
            parseU32(event.target.value) !== saved.reconcile_interval_seconds,
          );
          setJustSaved(false);
          setError(null);
        }}
      />
      {!reconcileIntervalSecondsIsValid && (
        <p
          id={reconcileIntervalSecondsErrorId}
          className="global-settings-form__error"
          role="alert"
        >
          {NUMBER_RANGE_MESSAGE}
        </p>
      )}

      <label>
        <input
          type="checkbox"
          checked={notifications}
          disabled={submitting}
          onChange={(event) => {
            setNotifications(event.target.checked);
            setNotificationsDirty(event.target.checked !== saved.notifications);
            setJustSaved(false);
            setError(null);
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
        disabled={submitting}
        aria-required="true"
        aria-invalid={!notificationBatchThresholdIsValid}
        aria-describedby={
          notificationBatchThresholdIsValid ? undefined : notificationBatchThresholdErrorId
        }
        onChange={(event) => {
          setNotificationBatchThreshold(event.target.value);
          setNotificationBatchThresholdDirty(
            parseU32(event.target.value) !== saved.notification_batch_threshold,
          );
          setJustSaved(false);
          setError(null);
        }}
      />
      {!notificationBatchThresholdIsValid && (
        <p
          id={notificationBatchThresholdErrorId}
          className="global-settings-form__error"
          role="alert"
        >
          {NUMBER_RANGE_MESSAGE}
        </p>
      )}

      <label>
        <input
          type="checkbox"
          checked={badgeSidebar}
          disabled={submitting}
          onChange={(event) => {
            setBadgeSidebar(event.target.checked);
            setBadgeSidebarDirty(event.target.checked !== saved.badge_sidebar);
            setJustSaved(false);
            setError(null);
          }}
        />
        Badge sidebar
      </label>

      {error !== null && (
        <p className="settings__error" role="alert">
          {error.message}
        </p>
      )}

      {/* Always mounted — rather than only while `justSaved` — so
          assistive tech has a stable `role="status"` region to watch;
          only the message text inside it is conditional. */}
      <p className="global-settings-form__success" role="status">
        {justSaved && error === null ? "Saved." : ""}
      </p>

      <button type="submit" disabled={!canSubmit}>
        Save
      </button>
    </form>
  );
}
