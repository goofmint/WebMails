import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { titleCount, watchTitle } from "./title";

const PATTERN = /\((\d+)\)/;

// jsdom's MutationObserver delivers records as a microtask, so tests that
// trigger a mutation must flush microtasks before asserting the callback
// ran. A resolved promise `await` is enough to let a queued microtask run.
function flushMicrotasks(): Promise<void> {
  return Promise.resolve();
}

describe("titleCount", () => {
  beforeEach(() => {
    document.head.innerHTML = "";
    document.title = "";
  });

  it("extracts the count from a matching title", () => {
    document.title = "(3) Inbox";
    expect(titleCount(document, PATTERN)).toBe(3);
  });

  it("returns null when the title has no count", () => {
    document.title = "Inbox";
    expect(titleCount(document, PATTERN)).toBeNull();
  });

  it("returns null for an empty title", () => {
    document.title = "";
    expect(titleCount(document, PATTERN)).toBeNull();
  });

  it("extracts a multi-digit count", () => {
    document.title = "(42) Inbox - Example";
    expect(titleCount(document, PATTERN)).toBe(42);
  });

  it("does not depend on lastIndex when the pattern carries the g flag", () => {
    const globalPattern = /\((\d+)\)/g;
    document.title = "(7) Inbox";
    expect(titleCount(document, globalPattern)).toBe(7);
    // A second call with the same (stateful, if lastIndex were relied on)
    // pattern object must still match from the start.
    expect(titleCount(document, globalPattern)).toBe(7);
  });

  it("treats an unsafe integer capture as no count", () => {
    document.title = "(99999999999999999999) Inbox";
    expect(titleCount(document, PATTERN)).toBeNull();
  });
});

describe("watchTitle", () => {
  beforeEach(() => {
    document.head.innerHTML = "<title></title>";
    document.title = "";
  });

  afterEach(() => {
    document.head.innerHTML = "<title></title>";
    document.title = "";
  });

  it("notifies when an existing title element's text changes", async () => {
    document.title = "Inbox";
    const calls: number[] = [];
    const unsubscribe = watchTitle(document, () => {
      calls.push(document.title.length);
    });

    document.title = "(3) Inbox";
    await flushMicrotasks();

    expect(calls).toEqual([document.title.length]);
    unsubscribe();
  });

  it("notifies when the <title> element itself is replaced", async () => {
    document.title = "Inbox";
    let notified = 0;
    const unsubscribe = watchTitle(document, () => {
      notified += 1;
    });

    const oldTitleEl = document.querySelector("title");
    const newTitleEl = document.createElement("title");
    newTitleEl.textContent = "(5) Inbox";
    oldTitleEl?.replaceWith(newTitleEl);
    await flushMicrotasks();

    expect(notified).toBe(1);
    expect(document.title).toBe("(5) Inbox");
    unsubscribe();
  });

  it("does not notify for an unrelated <head> mutation that leaves the title unchanged", async () => {
    document.title = "Inbox";
    let notified = 0;
    const unsubscribe = watchTitle(document, () => {
      notified += 1;
    });

    const meta = document.createElement("meta");
    meta.setAttribute("name", "unrelated");
    document.head.appendChild(meta);
    await flushMicrotasks();

    expect(notified).toBe(0);
    unsubscribe();
  });

  it("stops notifying after unsubscribe", async () => {
    document.title = "Inbox";
    let notified = 0;
    const unsubscribe = watchTitle(document, () => {
      notified += 1;
    });

    unsubscribe();

    document.title = "(9) Inbox";
    await flushMicrotasks();

    expect(notified).toBe(0);
  });

  it("does not mutate the document itself", async () => {
    document.title = "Inbox";
    const unsubscribe = watchTitle(document, () => {});
    const headHtmlBefore = document.head.innerHTML;

    await flushMicrotasks();

    expect(document.head.innerHTML).toBe(headHtmlBefore);
    unsubscribe();
  });
});
