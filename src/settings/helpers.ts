/**
 * Pure helpers for the settings screen's CRUD forms (Task 1.12): URL
 * parsing/validation, recipe-driven name/profile suggestion, and the
 * default/isolated/named profile representation the add and edit forms
 * share via `ProfileField`.
 */

import { matchRecipe } from "../../agent/recipes/registry";
import type { ProfileDefault } from "../../agent/recipes/types";

/**
 * Mirrors `ID_PATTERN_DESCRIPTION` (`"[a-z0-9-]{1,48}"`) in
 * src-tauri/src/config/model.rs, which `ProfileName::new` validates every
 * profile name against. There is no IPC-exposed way to ask Rust for this
 * pattern, so the settings UI has to know it independently to give inline
 * feedback before a doomed `add_service`/`update_service` round-trip —
 * `ProfileName::new` on the Rust side remains the authoritative check.
 */
const PROFILE_NAME_PATTERN = /^[a-z0-9-]{1,48}$/;

/** Whether `value` is a syntactically valid profile name (design.md §2.2.3, §5). */
export function isValidProfileName(value: string): boolean {
  return PROFILE_NAME_PATTERN.test(value);
}

/** The three ways the add/edit forms can present a service's `profile` (design.md §2.2.13). */
export type ProfileKind = "default" | "isolated" | "named";

/**
 * A profile field's in-progress value: `namedValue` is only meaningful
 * (and only shown) when `kind` is `"named"`, but is kept even while
 * `kind` is `"default"`/`"isolated"` so switching back to `"named"`
 * restores whatever the user had typed.
 */
export interface ProfileSelection {
  readonly kind: ProfileKind;
  readonly namedValue: string;
}

/** Converts an existing `ServiceConfig.profile` string into its `ProfileSelection`. */
export function profileStringToSelection(profile: string): ProfileSelection {
  if (profile === "default" || profile === "isolated") {
    return { kind: profile, namedValue: "" };
  }
  return { kind: "named", namedValue: profile };
}

/** The `profile` string a `ProfileSelection` would submit as (may be invalid; see `isProfileSelectionValid`). */
export function profileSelectionToString(selection: ProfileSelection): string {
  return selection.kind === "named" ? selection.namedValue : selection.kind;
}

/** Whether `selection` is submittable: always true for `default`/`isolated`, else a valid profile name. */
export function isProfileSelectionValid(selection: ProfileSelection): boolean {
  if (selection.kind !== "named") {
    return true;
  }
  return isValidProfileName(selection.namedValue);
}

/** The `ProfileSelection` for a recipe's `defaultProfile` (always `"default"` or `"isolated"`). */
export function defaultProfileSelection(defaultProfile: ProfileDefault): ProfileSelection {
  return { kind: defaultProfile, namedValue: "" };
}

/**
 * Parses `value` as an absolute URL, returning `null` for anything that
 * fails to parse or whose scheme is not `http`/`https` (every service and
 * icon URL in the config model is constrained to http(s); design.md
 * §2.2.1).
 */
export function parseServiceUrl(value: string): URL | null {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    return null;
  }
  return url;
}

export interface ServiceSuggestion {
  readonly name: string;
  readonly profile: ProfileSelection;
}

/**
 * Suggests a starting name and profile for a freshly parsed service URL,
 * using the same recipe registry the agent matches at runtime (design.md
 * §2.2.13: "profile is pre-filled from `matchRecipe(url).defaultProfile`")
 * — so the suggestion always matches what `defaultProfile` the resulting
 * service would actually get once the agent runs against it.
 */
export function suggestServiceDetails(url: URL): ServiceSuggestion {
  const recipe = matchRecipe(url);
  return { name: url.hostname, profile: defaultProfileSelection(recipe.defaultProfile) };
}

/** The distinct named (non-`default`/`isolated`) profiles already used by `services`, sorted. */
export function distinctNamedProfiles(
  services: readonly { readonly profile: string }[],
): readonly string[] {
  const names = new Set<string>();
  for (const service of services) {
    if (service.profile !== "default" && service.profile !== "isolated") {
      names.add(service.profile);
    }
  }
  return Array.from(names).sort();
}
