import { describe, expect, it, vi } from "vitest";
import type { RecipeContext } from "./types";
import { generic } from "./generic";

function flushMicrotasks(): Promise<void> {
  return Promise.resolve();
}

function makeContext(doc: Document): RecipeContext {
  return {
    serviceUrl: new URL("https://mail.example.com/"),
    document: doc,
    fetch: vi.fn(() => Promise.reject(new Error("generic recipe must not fetch"))),
  };
}

describe("generic recipe", () => {
  it("always matches", () => {
    expect(generic.matches(new URL("https://anything.example/"))).toBe(true);
  });

  it("describes itself as the title strategy", () => {
    expect(generic.describe(new URL("https://anything.example/")).strategy).toBe("title");
  });

  it("read() extracts the count from a title like (3) Inbox", async () => {
    document.title = "(3) Inbox";
    const result = await generic.read(makeContext(document));
    expect(result).toEqual({ count: 3, messages: [] });
  });

  it("read() returns 0, not null, when the title has no count", async () => {
    document.title = "Inbox";
    const result = await generic.read(makeContext(document));
    expect(result).toEqual({ count: 0, messages: [] });
  });

  it("watch() notifies on a replaced <title> element and read() then reflects the new count", async () => {
    document.head.innerHTML = "<title>Inbox</title>";
    document.title = "Inbox";
    const ctx = makeContext(document);

    const onChange = vi.fn();
    const unsubscribe = generic.watch(ctx, onChange);

    const oldTitleEl = document.querySelector("title");
    const newTitleEl = document.createElement("title");
    newTitleEl.textContent = "(5) Inbox";
    oldTitleEl?.replaceWith(newTitleEl);
    await flushMicrotasks();

    expect(onChange).toHaveBeenCalledTimes(1);
    const result = await generic.read(ctx);
    expect(result).toEqual({ count: 5, messages: [] });

    unsubscribe();
  });

  it("watch() stops notifying after unsubscribe", async () => {
    document.head.innerHTML = "<title>Inbox</title>";
    document.title = "Inbox";
    const ctx = makeContext(document);

    const onChange = vi.fn();
    const unsubscribe = generic.watch(ctx, onChange);
    unsubscribe();

    document.title = "(9) Inbox";
    await flushMicrotasks();

    expect(onChange).not.toHaveBeenCalled();
  });
});
