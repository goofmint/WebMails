/**
 * Pure helpers for the generated letter icon fallback (Task 1.14; design.md
 * §11.1's step 5: "first letter of the service name on a colour derived
 * deterministically from the service id"). Kept separate from any
 * component so both are trivially unit-testable.
 */

/**
 * 32-bit FNV-1a hash of `value`, always a non-negative integer
 * (`>>> 0` forces the result into the unsigned 32-bit range).
 */
export function fnv1aHash(value: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    // 32-bit multiply by the FNV prime, done with `Math.imul` so the
    // result wraps the same way a fixed-width integer multiply would
    // (plain `*` would lose precision once the product exceeds 2^53).
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/** Fixed saturation/lightness (design.md §11.1): dark enough, at every
 * hue, for white icon text to stay readable on top of it. */
const SATURATION_PERCENT = 65;
const LIGHTNESS_PERCENT = 38;

/**
 * A CSS `hsl()` colour deterministically derived from `serviceId`: the
 * hue comes from {@link fnv1aHash} modulo 360, at a fixed
 * saturation/lightness chosen for contrast with white text.
 */
export function colorForServiceId(serviceId: string): string {
  const hue = fnv1aHash(serviceId) % 360;
  return `hsl(${hue}, ${SATURATION_PERCENT}%, ${LIGHTNESS_PERCENT}%)`;
}

/**
 * The first character of `name` (or `serviceId` if `name` is empty after
 * trimming), upper-cased. Iterates by Unicode code point (`Array.from`),
 * so a name starting with a surrogate-pair character (e.g. an emoji) still
 * yields one whole character rather than half of one.
 */
export function initialForService(name: string, serviceId: string): string {
  const trimmedName = name.trim();
  const source = trimmedName.length > 0 ? trimmedName : serviceId.trim();
  const firstChar = Array.from(source)[0];
  return firstChar === undefined ? "?" : firstChar.toUpperCase();
}
