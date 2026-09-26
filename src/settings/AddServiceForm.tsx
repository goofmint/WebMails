/**
 * The settings screen's add-service form (Task 1.12; design.md §2.2.13:
 * "Add form: URL, then name. The profile is pre-filled from
 * `matchRecipe(url).defaultProfile`."). On success, the form resets to
 * blank immediately (the service now exists, so it must not stay primed
 * to create a duplicate), then `select_service` is called with the new
 * id so the newly added service becomes the active one; a failure from
 * that follow-up call is reported on its own, separately from an
 * `add_service` failure.
 */

import { useState, type FormEvent } from "react";
import type { CommandError, ServiceConfig, SettingsIpc } from "../ipc";
import { toCommandError } from "../ipc";
import {
  distinctNamedProfiles,
  isProfileSelectionValid,
  parseServiceUrl,
  profileSelectionToString,
  suggestServiceDetails,
  type ProfileSelection,
} from "./helpers";
import { ProfileField } from "./ProfileField";

export interface AddServiceFormProps {
  readonly ipc: SettingsIpc;
  readonly services: readonly ServiceConfig[];
}

const BLANK_PROFILE: ProfileSelection = { kind: "default", namedValue: "" };

export function AddServiceForm({ ipc, services }: AddServiceFormProps) {
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [profile, setProfile] = useState<ProfileSelection>(BLANK_PROFILE);
  const [profileTouched, setProfileTouched] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const parsedUrl = parseServiceUrl(url);
  const trimmedName = name.trim();
  const canSubmit =
    parsedUrl !== null && trimmedName.length > 0 && isProfileSelectionValid(profile) && !submitting;

  function handleUrlChange(nextUrl: string): void {
    setUrl(nextUrl);
    const parsed = parseServiceUrl(nextUrl);
    if (parsed === null) {
      return;
    }
    const suggestion = suggestServiceDetails(parsed);
    if (!nameTouched) {
      setName(suggestion.name);
    }
    if (!profileTouched) {
      setProfile(suggestion.profile);
    }
  }

  function resetForm(): void {
    setUrl("");
    setName("");
    setNameTouched(false);
    setProfile(BLANK_PROFILE);
    setProfileTouched(false);
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>): Promise<void> {
    event.preventDefault();
    if (!canSubmit || parsedUrl === null) {
      return;
    }
    setSubmitting(true);
    setError(null);
    let created: ServiceConfig;
    try {
      created = await ipc.addService(
        trimmedName,
        parsedUrl.toString(),
        profileSelectionToString(profile),
      );
    } catch (caughtError: unknown) {
      setError(toCommandError(caughtError));
      setSubmitting(false);
      return;
    }
    // addService succeeded — the service now exists, so the form must not
    // stay primed to create a duplicate even if the follow-up
    // selectService below fails. Reset it first, then handle that failure
    // on its own.
    resetForm();
    try {
      await ipc.selectService(created.id);
    } catch (caughtError: unknown) {
      setError(toCommandError(caughtError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form
      className="add-service-form"
      aria-label="Add service"
      onSubmit={(event) => {
        void handleSubmit(event);
      }}
    >
      <h2>Add service</h2>

      <label htmlFor="add-service-url">URL</label>
      <input
        id="add-service-url"
        type="text"
        value={url}
        onChange={(event) => {
          handleUrlChange(event.target.value);
        }}
      />

      <label htmlFor="add-service-name">Name</label>
      <input
        id="add-service-name"
        type="text"
        value={name}
        onChange={(event) => {
          setName(event.target.value);
          setNameTouched(true);
        }}
      />

      <ProfileField
        idPrefix="add-service"
        selection={profile}
        existingNamedProfiles={distinctNamedProfiles(services)}
        onChange={(next) => {
          setProfile(next);
          setProfileTouched(true);
        }}
      />

      {error !== null && (
        <p className="settings__error" role="alert">
          {error.message}
        </p>
      )}

      <button type="submit" disabled={!canSubmit}>
        Add
      </button>
    </form>
  );
}
