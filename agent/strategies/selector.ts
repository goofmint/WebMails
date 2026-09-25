// Selector strategy helpers (design.md §2.2.14):
//   selectorCount(document, selector, mode: "text" | "rows") -> number | null
//   watchSelector(document, selector, cb) -> unsubscribe
//
// Both functions validate the selector eagerly and throw a clear error at
// call time when it is invalid, rather than swallowing the failure into a
// `null`/no-op result (no fallback defaults: an invalid selector is a
// programming error in a recipe, not a "no data" signal).

// Throws (with a recipe-identifiable message) if `selector` cannot be
// parsed as a CSS selector by `document`.
function assertValidSelector(document: Document, selector: string): void {
  try {
    document.querySelector(selector);
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`selector strategy: invalid selector "${selector}": ${reason}`, {
      cause: error,
    });
  }
}

// Parses the first run of digits in `text` into a non-negative integer.
// Digits may contain "," thousands separators (e.g. "1,234"); anything
// after the digit run is ignored, so "99+" parses as 99. Returns `null`
// when there is no digit run, or when the parsed value is not a safe
// integer (no upper-bound clamping: an out-of-range count is reported as
// "unparseable", not silently truncated).
function parseCount(text: string): number | null {
  const match = /\d[\d,]*/.exec(text);
  const digits = match?.[0];
  if (digits === undefined) {
    return null;
  }
  const value = Number.parseInt(digits.replace(/,/g, ""), 10);
  return Number.isSafeInteger(value) ? value : null;
}

// Reads a count from the page via `selector`.
//
// mode "text": reads the first matching element's `textContent` and parses
// an integer out of it (see parseCount). Returns `null` when there is no
// matching element, the text is empty, or it has no parseable digit run —
// all three are "no reliable count", not "zero" (design.md's count=null
// rule: a missing/unparseable signal must not be reported as 0 unread).
//
// mode "rows": returns the number of matching elements. `0` here is a real
// signal (no matching rows), unlike the `null` cases in "text" mode above.
export function selectorCount(
  document: Document,
  selector: string,
  mode: "text" | "rows",
): number | null {
  assertValidSelector(document, selector);

  if (mode === "rows") {
    return document.querySelectorAll(selector).length;
  }

  const element = document.querySelector(selector);
  if (!element) {
    return null;
  }
  return parseCount(element.textContent ?? "");
}

// True when `node` is, contains, or (for elements) is contained by an
// element matching `selector`. Used to decide whether a mutation could
// change what `selectorCount(document, selector, ...)` would return.
function isRelevant(node: Node, selector: string): boolean {
  if (node.nodeType === Node.ELEMENT_NODE) {
    const element = node as Element;
    return (
      element.matches(selector) ||
      element.closest(selector) !== null ||
      element.querySelector(selector) !== null
    );
  }
  if (node.nodeType === Node.TEXT_NODE || node.nodeType === Node.CDATA_SECTION_NODE) {
    // Text nodes have no `matches`/`closest` of their own; judge relevance
    // by their element parent (an ancestor-or-self match on the parent).
    const parent = node.parentElement;
    return parent !== null && (parent.matches(selector) || parent.closest(selector) !== null);
  }
  return false;
}

// Watches `document` for DOM changes that could change what
// `selectorCount(document, selector, ...)` returns, and calls `cb` (at
// most once per MutationObserver batch) when one is observed. Returns an
// unsubscribe function that disconnects the observer.
//
// Debouncing/coalescing repeated calls to `cb` across batches is the
// caller's (the loop's) job, not this helper's.
export function watchSelector(document: Document, selector: string, cb: () => void): () => void {
  assertValidSelector(document, selector);

  const root = document.documentElement;
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      if (isRelevant(mutation.target, selector)) {
        cb();
        return;
      }
      for (const node of mutation.addedNodes) {
        if (isRelevant(node, selector)) {
          cb();
          return;
        }
      }
      for (const node of mutation.removedNodes) {
        if (isRelevant(node, selector)) {
          cb();
          return;
        }
      }
    }
  });

  observer.observe(root, {
    childList: true,
    subtree: true,
    characterData: true,
  });

  return (): void => {
    observer.disconnect();
  };
}
