// Title strategy helpers (design.md §2.2.14). These are recipe-agnostic:
// they read and observe `document.title` only, and never mutate the page.

// Returns the first capture group of `pattern` matched against `doc.title`,
// parsed as a decimal integer, or `null` when the title has no match.
//
// `null` therefore means "no count found in the title", not "signed out" or
// any other page state: the title strategy cannot tell those apart (a
// same-origin login page cannot be distinguished from an empty inbox, per
// design.md §2.2.14's count=null rule). Callers that only use the title
// strategy (e.g. the `generic` recipe) convert this `null` into `0`
// themselves; `titleCount` never does that conversion on their behalf, so
// callers that DO have another way to detect a signed-out state (e.g. a
// specific recipe combining this with a fetch check) can still see "no
// match" as a distinct outcome.
//
// A match whose captured digits do not parse to a safe, non-negative
// integer (e.g. it overflows `Number.MAX_SAFE_INTEGER`) is treated the same
// as no match at all (`null`), since such a value cannot be trusted as a
// real unread count.
//
// `pattern` may carry the `g` flag; matching does not rely on `lastIndex`
// (a fresh, non-sticky match is taken from the start of the string each
// time), so a `g`-flagged pattern behaves the same on every call.
export function titleCount(doc: Document, pattern: RegExp): number | null {
  const match = doc.title.match(new RegExp(pattern.source, stripGlobalAndSticky(pattern.flags)));
  const captured = match?.[1];
  if (captured === undefined) {
    return null;
  }
  if (!/^\d+$/.test(captured)) {
    return null;
  }
  const value = Number(captured);
  if (!Number.isSafeInteger(value)) {
    return null;
  }
  return value;
}

function stripGlobalAndSticky(flags: string): string {
  return flags.replace(/[gy]/g, "");
}

export type Unwatch = () => void;

// Observes `doc.title` for changes and calls `cb` whenever the title text
// changes, including when the `<title>` element itself is replaced (a
// MutationObserver on `<head>` with `childList` + `subtree` catches node
// replacement; `characterData` catches in-place text edits).
//
// Falls back to `doc.documentElement` when `doc.head` is absent (e.g. a
// document mid-parse), so observation still starts rather than silently
// doing nothing.
//
// Never mutates the document; only reads `doc.title`. Not debounced -
// callers that need debouncing (e.g. the agent loop) add it themselves.
// Returns an unsubscribe function that disconnects the observer.
export function watchTitle(doc: Document, cb: () => void): Unwatch {
  const target = doc.head ?? doc.documentElement;
  let lastTitle = doc.title;

  const observer = new MutationObserver(() => {
    const currentTitle = doc.title;
    if (currentTitle === lastTitle) {
      return;
    }
    lastTitle = currentTitle;
    cb();
  });

  observer.observe(target, {
    childList: true,
    subtree: true,
    characterData: true,
  });

  return () => {
    observer.disconnect();
  };
}
