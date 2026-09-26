/**
 * Per-service edit form (Task 1.12; design.md §2.2.13: "Per-service edit:
 * name, URL, profile, notifications, icon override." — icon override is
 * Task 1.14 and is skipped here). Submits only the fields the user
 * actually changed, as `update_service`'s `patch`.
 */

import { useState, type FormEvent } from "react";
import type { CommandError, ServiceConfig, ServicePatchInput, SettingsIpc } from "../ipc";
import { toCommandError } from "../ipc";
import {
  isProfileSelectionValid,
  parseServiceUrl,
  profileSelectionToString,
  profileStringToSelection,
  type ProfileSelection,
} from "./helpers";
import { ProfileField } from "./ProfileField";

export interface EditServiceFormProps {
  readonly ipc: SettingsIpc;
  readonly service: ServiceConfig;
  readonly existingNamedProfiles: readonly string[];
  readonly onClose: () => void;
}

export function EditServiceForm({
  ipc,
  service,
  existingNamedProfiles,
  onClose,
}: EditServiceFormProps) {
  const [name, setName] = useState(service.name);
  const [url, setUrl] = useState(service.url);
  const [profile, setProfile] = useState<ProfileSelection>(
    profileStringToSelection(service.profile),
  );
  const [notifications, setNotifications] = useState(service.notifications);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const trimmedName = name.trim();
  const parsedUrl = parseServiceUrl(url);
  const profileString = profileSelectionToString(profile);

  // `ServicePatchInput`'s fields are `readonly`, so the patch is assembled
  // by conditional spreading (each key set once, at construction) rather
  // than by mutating a `{}` literal after the fact.
  const patch: ServicePatchInput = {
    ...(trimmedName.length > 0 && trimmedName !== service.name ? { name: trimmedName } : {}),
    ...(parsedUrl !== null && parsedUrl.toString() !== service.url
      ? { url: parsedUrl.toString() }
      : {}),
    ...(isProfileSelectionValid(profile) && profileString !== service.profile
      ? { profile: profileString }
      : {}),
    ...(notifications !== service.notifications ? { notifications } : {}),
  };
  const hasChanges = Object.keys(patch).length > 0;
  const urlIsValid = url.trim().length > 0 && parsedUrl !== null;
  const canSubmit =
    hasChanges &&
    trimmedName.length > 0 &&
    urlIsValid &&
    isProfileSelectionValid(profile) &&
    !submitting;
  const recreatesPage = patch.url !== undefined || patch.profile !== undefined;

  async function handleSubmit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    setSubmitting(true);
    setError(null);
    try {
      await ipc.updateService(service.id, patch);
      onClose();
    } catch (caughtError: unknown) {
      setError(toCommandError(caughtError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="edit-service-form"
      aria-label={`Edit ${service.name}`}
      onSubmit={(event) => {
        void handleSubmit(event);
      }}
    >
      <label htmlFor={`edit-${service.id}-name`}>Name</label>
      <input
        id={`edit-${service.id}-name`}
        type="text"
        value={name}
        onChange={(event) => {
          setName(event.target.value);
        }}
      />

      <label htmlFor={`edit-${service.id}-url`}>URL</label>
      <input
        id={`edit-${service.id}-url`}
        type="text"
        value={url}
        onChange={(event) => {
          setUrl(event.target.value);
        }}
      />

      <ProfileField
        idPrefix={`edit-${service.id}`}
        selection={profile}
        existingNamedProfiles={existingNamedProfiles}
        onChange={setProfile}
      />

      <label>
        <input
          type="checkbox"
          checked={notifications}
          onChange={(event) => {
            setNotifications(event.target.checked);
          }}
        />
        Notifications
      </label>

      {recreatesPage && (
        <p className="edit-service-form__notice">
          Changing the URL or profile recreates this service&rsquo;s page.
        </p>
      )}

      {error !== null && (
        <p className="settings__error" role="alert">
          {error.message}
        </p>
      )}

      <div className="edit-service-form__actions">
        <button type="submit" disabled={!canSubmit}>
          Save
        </button>
        <button type="button" onClick={onClose}>
          Cancel
        </button>
      </div>
    </form>
  );
}
