/**
 * Normalizes an `invoke()` rejection into the `{ kind, message }` shape
 * every `AppError` serializes as (src-tauri/src/error.rs, design.md
 * §5.2), so the settings UI can always show `.message` (Task 1.12's
 * "Errors from commands (`{kind, message}`) shown in the UI").
 */

import type { CommandError } from "./types";

/** True when `value` already has the `{ kind: string, message: string }` shape. */
function isCommandErrorShape(value: unknown): value is CommandError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return typeof record.kind === "string" && typeof record.message === "string";
}

/**
 * `caughtError` is whatever a `catch` clause around an `invoke()` call
 * received — typed `unknown` and narrowed immediately here. A command
 * failure rejects with exactly the `{ kind, message }` JSON its `AppError`
 * serialized as, so that is returned unchanged. Anything else (e.g. a
 * `TypeError` from a bug in this module, not a command failure) still
 * needs some string to display — `message` falls back to the caught
 * value's own string form, which is error-display plumbing, not a
 * substituted business value.
 */
export function toCommandError(caughtError: unknown): CommandError {
  if (isCommandErrorShape(caughtError)) {
    return caughtError;
  }
  return { kind: "unknown", message: String(caughtError) };
}
