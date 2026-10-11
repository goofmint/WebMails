import { describe, expect, it, vi } from "vitest";
import type { RecipeContext } from "./types";
import { icloud } from "./icloud";

function flushMicrotasks(): Promise<void> {
  return Promise.resolve();
}

function makeContext(doc: Document): RecipeContext {
  return {
    serviceUrl: new URL("https://www.icloud.com/mail/"),
    document: doc,
    fetch: vi.fn(() => Promise.reject(new Error("icloud recipe must not fetch"))),
  };
}

describe("icloud recipe", () => {
  it("matches www.icloud.com/mail and nested mail paths only", () => {
    expect(icloud.matches(new URL("https://www.icloud.com/mail"))).toBe(true);
    expect(icloud.matches(new URL("https://www.icloud.com/mail/inbox"))).toBe(true);
    expect(icloud.matches(new URL("https://www.icloud.com/mailfoo"))).toBe(false);
    expect(icloud.matches(new URL("https://www.icloud.com/notes"))).toBe(false);
    expect(icloud.matches(new URL("https://icloud.com/mail"))).toBe(false);
  });

  it("describes itself as the title strategy", () => {
    expect(icloud.describe(new URL("https://www.icloud.com/mail/")).strategy).toBe("title");
  });

  it("read() extracts a parenthesized count", async () => {
    document.title = "(3) Inbox";
    const result = await icloud.read(makeContext(document));
    expect(result).toEqual({ count: 3, messages: [] });
  });

  it("read() returns 0, not null, when the title has no count", async () => {
    document.title = "iCloud Mail";
    const result = await icloud.read(makeContext(document));
    expect(result).toEqual({ count: 0, messages: [] });
  });

  it("watch() notifies on a replaced <title> element and read() then reflects the new count", async () => {
    document.head.innerHTML = "<title>Inbox</title>";
    document.title = "Inbox";
    const ctx = makeContext(document);

    const onChange = vi.fn();
    const unsubscribe = icloud.watch(ctx, onChange);

    const oldTitleEl = document.querySelector("title");
    const newTitleEl = document.createElement("title");
    newTitleEl.textContent = "(5) Inbox";
    oldTitleEl?.replaceWith(newTitleEl);
    await flushMicrotasks();

    expect(onChange).toHaveBeenCalledTimes(1);
    const result = await icloud.read(ctx);
    expect(result).toEqual({ count: 5, messages: [] });

    unsubscribe();
  });

  it("watch() stops notifying after unsubscribe", async () => {
    document.head.innerHTML = "<title>Inbox</title>";
    document.title = "Inbox";
    const ctx = makeContext(document);

    const onChange = vi.fn();
    const unsubscribe = icloud.watch(ctx, onChange);
    unsubscribe();

    document.title = "(9) Inbox";
    await flushMicrotasks();

    expect(onChange).not.toHaveBeenCalled();
  });
});
