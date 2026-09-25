import { describe, expect, it, vi } from "vitest";
import { selectorCount, watchSelector } from "./selector";

// MutationObserver callbacks run in a microtask; give them a chance to run.
async function flushMicrotasks(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("selectorCount", () => {
  describe("mode 'text'", () => {
    it("parses a plain integer", () => {
      document.body.innerHTML = '<span class="count">5</span>';
      expect(selectorCount(document, ".count", "text")).toBe(5);
    });

    it("parses an integer surrounded by other text and whitespace", () => {
      document.body.innerHTML = '<span class="count">\n  12 unread\n</span>';
      expect(selectorCount(document, ".count", "text")).toBe(12);
    });

    it("strips comma thousands separators", () => {
      document.body.innerHTML = '<span class="count">1,234</span>';
      expect(selectorCount(document, ".count", "text")).toBe(1234);
    });

    it("parses the leading digit run of a capped badge like '99+'", () => {
      document.body.innerHTML = '<span class="count">99+</span>';
      expect(selectorCount(document, ".count", "text")).toBe(99);
    });

    it("returns null for empty text", () => {
      document.body.innerHTML = '<span class="count"></span>';
      expect(selectorCount(document, ".count", "text")).toBeNull();
    });

    it("returns null when the text has no digits", () => {
      document.body.innerHTML = '<span class="count">no unread mail</span>';
      expect(selectorCount(document, ".count", "text")).toBeNull();
    });

    it("returns null when no element matches", () => {
      document.body.innerHTML = "<div></div>";
      expect(selectorCount(document, ".count", "text")).toBeNull();
    });
  });

  describe("mode 'rows'", () => {
    it("counts matching elements", () => {
      document.body.innerHTML = `
        <ul>
          <li class="row">a</li>
          <li class="row">b</li>
          <li class="row">c</li>
        </ul>
      `;
      expect(selectorCount(document, ".row", "rows")).toBe(3);
    });

    it("returns 0 when there are no matches", () => {
      document.body.innerHTML = "<ul></ul>";
      expect(selectorCount(document, ".row", "rows")).toBe(0);
    });
  });

  it("throws a clear error for an invalid selector, in both modes", () => {
    document.body.innerHTML = "<div></div>";
    expect(() => selectorCount(document, ":::not-a-selector", "text")).toThrow(/invalid selector/);
    expect(() => selectorCount(document, ":::not-a-selector", "rows")).toThrow(/invalid selector/);
  });
});

describe("watchSelector", () => {
  it("calls cb when a matching element's text changes", async () => {
    document.body.innerHTML = '<span class="count">1</span>';
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".count", cb);

    document.querySelector(".count")!.textContent = "2";
    await flushMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
    unwatch();
  });

  it("calls cb when a matching element is replaced, and again on a later change", async () => {
    document.body.innerHTML = '<div id="host"><span class="count">1</span></div>';
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".count", cb);

    const host = document.getElementById("host")!;
    const replacement = document.createElement("span");
    replacement.className = "count";
    replacement.textContent = "2";
    host.replaceChild(replacement, host.querySelector(".count")!);
    await flushMicrotasks();
    expect(cb).toHaveBeenCalledTimes(1);

    document.querySelector(".count")!.textContent = "3";
    await flushMicrotasks();
    expect(cb).toHaveBeenCalledTimes(2);

    unwatch();
  });

  it("calls cb when a matching row is added", async () => {
    document.body.innerHTML = '<ul id="list"><li class="row">a</li></ul>';
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".row", cb);

    const li = document.createElement("li");
    li.className = "row";
    li.textContent = "b";
    document.getElementById("list")!.appendChild(li);
    await flushMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
    unwatch();
  });

  it("calls cb when a matching row is removed", async () => {
    document.body.innerHTML = `
      <ul id="list">
        <li class="row">a</li>
        <li class="row">b</li>
      </ul>
    `;
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".row", cb);

    document.querySelector(".row")!.remove();
    await flushMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
    unwatch();
  });

  it("does not call cb for unrelated DOM changes", async () => {
    document.body.innerHTML = '<span class="count">1</span><div id="other"></div>';
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".count", cb);

    document.getElementById("other")!.textContent = "unrelated change";
    await flushMicrotasks();

    expect(cb).not.toHaveBeenCalled();
    unwatch();
  });

  it("calls cb at most once per mutation batch, even with multiple relevant changes", async () => {
    document.body.innerHTML = `
      <ul id="list">
        <li class="row">a</li>
      </ul>
    `;
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".row", cb);

    const list = document.getElementById("list")!;
    const b = document.createElement("li");
    b.className = "row";
    const c = document.createElement("li");
    c.className = "row";
    // Two synchronous mutations in the same task coalesce into one batch.
    list.appendChild(b);
    list.appendChild(c);
    await flushMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
    unwatch();
  });

  it("stops calling cb after unsubscribe", async () => {
    document.body.innerHTML = '<span class="count">1</span>';
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".count", cb);

    unwatch();
    document.querySelector(".count")!.textContent = "2";
    await flushMicrotasks();

    expect(cb).not.toHaveBeenCalled();
  });

  it("throws a clear error for an invalid selector", () => {
    document.body.innerHTML = "<div></div>";
    expect(() => watchSelector(document, ":::not-a-selector", vi.fn())).toThrow(/invalid selector/);
  });

  it("reflects DOM changes back through selectorCount after a watched mutation", async () => {
    document.body.innerHTML = `
      <ul id="list">
        <li class="row">a</li>
      </ul>
    `;
    const cb = vi.fn();
    const unwatch = watchSelector(document, ".row", cb);

    expect(selectorCount(document, ".row", "rows")).toBe(1);

    const li = document.createElement("li");
    li.className = "row";
    document.getElementById("list")!.appendChild(li);
    await flushMicrotasks();

    expect(cb).toHaveBeenCalledTimes(1);
    expect(selectorCount(document, ".row", "rows")).toBe(2);

    unwatch();
  });
});
