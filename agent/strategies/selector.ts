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

// True for a `characterData` mutation whose text node lives inside (or as)
// an element matching `selector`. Text nodes have no `matches`/`closest` of
// their own, so relevance is judged via their element parent (an
// ancestor-or-self match on the parent): a text change deep inside a
// matching element (e.g. a badge's own text) can change what
// `selectorCount(document, selector, ...)` returns.
function isCharacterDataRelevant(target: Node, selector: string): boolean {
  const parent = target.parentElement;
  return parent !== null && (parent.matches(selector) || parent.closest(selector) !== null);
}

// True when `node` is an element that itself matches `selector`, or
// contains a descendant matching it. Used for `childList` mutations, where
// only the changed nodes themselves (not their ancestors) can tell us
// whether the set of elements matching `selector` changed.
function matchesOrContainsMatch(node: Node, selector: string): boolean {
  if (node.nodeType !== Node.ELEMENT_NODE) {
    return false;
  }
  const element = node as Element;
  return element.matches(selector) || element.querySelector(selector) !== null;
}

// True for a `childList` mutation that added or removed a node matching
// `selector` (or containing a match), or whose target itself matches (or
// contains a match). Deliberately does not consult ancestors via
// `closest`: an ancestor's descendants changing doesn't by itself tell us
// whether an element matching `selector` was added or removed anywhere in
// the tree, only the changed nodes and target can.
function isChildListRelevant(mutation: MutationRecord, selector: string): boolean {
  if (matchesOrContainsMatch(mutation.target, selector)) {
    return true;
  }
  for (const node of mutation.addedNodes) {
    if (matchesOrContainsMatch(node, selector)) {
      return true;
    }
  }
  for (const node of mutation.removedNodes) {
    if (matchesOrContainsMatch(node, selector)) {
      return true;
    }
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
      if (mutation.type === "attributes") {
        // An attribute change (e.g. a class toggle) can make an element
        // start or stop matching `selector`; matches()/closest() only see
        // the post-mutation DOM, so there is no reliable "was this
        // relevant" check here. Always notify, still at most once per
        // batch.
        cb();
        return;
      }
      if (mutation.type === "childList") {
        if (isChildListRelevant(mutation, selector)) {
          cb();
          return;
        }
        continue;
      }
      if (isCharacterDataRelevant(mutation.target, selector)) {
        cb();
        return;
      }
    }
  });

  observer.observe(root, {
    attributes: true,
    childList: true,
    subtree: true,
    characterData: true,
  });

  return (): void => {
    observer.disconnect();
  };
}
