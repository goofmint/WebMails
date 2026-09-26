/**
 * Shared default/isolated/named profile picker (Task 1.12; design.md
 * §2.2.13: "It shows `default`, `isolated`, or a named profile chosen
 * from the existing names or entered as a new one."). Used by both the
 * add form and the per-service edit form.
 */

import { isValidProfileName, type ProfileKind, type ProfileSelection } from "./helpers";

export interface ProfileFieldProps {
  /** Unique per field instance, so radio `name`s/ids don't collide across forms (e.g. one per service row). */
  readonly idPrefix: string;
  readonly selection: ProfileSelection;
  readonly existingNamedProfiles: readonly string[];
  readonly onChange: (next: ProfileSelection) => void;
}

const KIND_ORDER: readonly ProfileKind[] = ["default", "isolated", "named"];

const KIND_LABELS: Readonly<Record<ProfileKind, string>> = {
  default: "Default (shared)",
  isolated: "Isolated (this service only)",
  named: "Named",
};

export function ProfileField({
  idPrefix,
  selection,
  existingNamedProfiles,
  onChange,
}: ProfileFieldProps) {
  const namedListId = `${idPrefix}-named-profiles`;
  const namedInputId = `${idPrefix}-named-value`;

  function handleKindChange(kind: ProfileKind): void {
    onChange({ kind, namedValue: kind === "named" ? selection.namedValue : "" });
  }

  const namedIsInvalid =
    selection.kind === "named" &&
    selection.namedValue.length > 0 &&
    !isValidProfileName(selection.namedValue);

  return (
    <fieldset className="profile-field">
      <legend>Profile</legend>
      {KIND_ORDER.map((kind) => (
        <label key={kind} className="profile-field__option">
          <input
            type="radio"
            name={`${idPrefix}-profile-kind`}
            value={kind}
            checked={selection.kind === kind}
            onChange={() => {
              handleKindChange(kind);
            }}
          />
          {KIND_LABELS[kind]}
        </label>
      ))}
      {selection.kind === "named" && (
        <div className="profile-field__named">
          <label htmlFor={namedInputId}>Profile name</label>
          <input
            id={namedInputId}
            type="text"
            list={namedListId}
            value={selection.namedValue}
            onChange={(event) => {
              onChange({ kind: "named", namedValue: event.target.value });
            }}
          />
          <datalist id={namedListId}>
            {existingNamedProfiles.map((name) => (
              <option key={name} value={name} />
            ))}
          </datalist>
          {namedIsInvalid && (
            <p className="profile-field__error" role="alert">
              Must be 1–48 lowercase letters, digits or hyphens.
            </p>
          )}
        </div>
      )}
    </fieldset>
  );
}
