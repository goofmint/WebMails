import { afterEach, describe, expect, it, vi } from "vitest";
import type { AgentWindow } from "./main";
import { bootstrapAgent } from "./main";
import type { ElumaBootstrap } from "./global";
import type { ReportInvoke } from "./core/report";
import type { Scheduler } from "./core/clock";

const VALID_BOOTSTRAP: ElumaBootstrap = {
  serviceId: "svc-1",
  serviceUrl: "https://mail.google.com/mail/u/0/",
  reportIntervalMs: 30_000,
  reconcileIntervalMs: 15_000,
};

// A scheduler that never actually fires anything, so tests that start the
// real loop don't leave real setTimeout callbacks pending after the test
// completes.
const noopScheduler: Scheduler = {
  setTimeout: () => 1,
  clearTimeout: () => {},
};

// Never expected to be called: no test in this file exercises a recipe that
// calls `ctx.fetch()` (see the comment on the "starts the loop" tests
// below), so this only needs to satisfy `AgentWindow`'s shape.
const unusedFetch: typeof fetch = () => Promise.reject(new Error("unexpected fetch call in test"));

function makeWindow(overrides: {
  origin?: string;
  bootstrap?: ElumaBootstrap;
  notTop?: boolean;
}): AgentWindow {
  const doc = new DOMParser().parseFromString(
    "<html><head></head><body></body></html>",
    "text/html",
  );
  const box: { top?: AgentWindow } = {};
  const win: AgentWindow = {
    location: { origin: overrides.origin ?? "https://mail.google.com" },
    document: doc,
    fetch: unusedFetch,
    get top(): AgentWindow {
      return box.top!;
    },
    ...(overrides.bootstrap !== undefined ? { __ELUMA__: overrides.bootstrap } : {}),
  };
  box.top = overrides.notTop ? makeDistinctFrame(doc) : win;
  return win;
}

function makeDistinctFrame(doc: Document): AgentWindow {
  const box: { top?: AgentWindow } = {};
  const frame: AgentWindow = {
    location: { origin: "https://frame.example" },
    document: doc,
    fetch: unusedFetch,
    get top(): AgentWindow {
      return box.top!;
    },
  };
  box.top = frame;
  return frame;
}

function noopInvoke(): ReturnType<typeof vi.fn<ReportInvoke>> {
  return vi.fn<ReportInvoke>().mockResolvedValue(undefined);
}

// Flushes the promise chain used by the initial `read()` -> `report()` call
// (recipe.read().then(...).catch(...).finally(...), then the reporter's own
// `await invoke(...)`) without relying on real timers.
async function flushMicrotasks(): Promise<void> {
  for (let i = 0; i < 5; i += 1) {
    await Promise.resolve();
  }
}

describe("bootstrapAgent", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("does nothing and does not delete __ELUMA__ when not the top frame", () => {
    const win = makeWindow({ bootstrap: VALID_BOOTSTRAP, notTop: true });
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(win.__ELUMA__).toBe(VALID_BOOTSTRAP);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does nothing when accessing top throws", () => {
    const win = makeWindow({ bootstrap: VALID_BOOTSTRAP });
    Object.defineProperty(win, "top", {
      get() {
        throw new Error("cross-origin frame access denied");
      },
    });
    const invoke = noopInvoke();

    expect(() => bootstrapAgent(win, { invoke, scheduler: noopScheduler })).not.toThrow();
    expect(win.__ELUMA__).toBe(VALID_BOOTSTRAP);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("deletes window.__ELUMA__ in the top frame", async () => {
    const win = makeWindow({ bootstrap: VALID_BOOTSTRAP });
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });
    await flushMicrotasks();

    expect(win.__ELUMA__).toBeUndefined();
  });

  it("logs and stops when __ELUMA__ is missing", () => {
    const win = makeWindow({});
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(errorSpy).toHaveBeenCalledWith(expect.stringContaining("[eluma-agent]"));
    expect(invoke).not.toHaveBeenCalled();
  });

  it("logs and stops when __ELUMA__ is missing required fields", () => {
    const win = makeWindow({
      bootstrap: { ...VALID_BOOTSTRAP, serviceId: "" },
    });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(errorSpy).toHaveBeenCalled();
    expect(win.__ELUMA__).toBeUndefined(); // read-then-delete happens before validation
    expect(invoke).not.toHaveBeenCalled();
  });

  it.each([
    { reportIntervalMs: 0 },
    { reportIntervalMs: -1 },
    { reconcileIntervalMs: 0 },
    { reconcileIntervalMs: -30_000 },
  ])("logs and stops when an interval is not positive: %o", (bad) => {
    const win = makeWindow({ bootstrap: { ...VALID_BOOTSTRAP, ...bad } });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(errorSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("logs and stops when serviceUrl cannot be parsed as a URL", () => {
    const win = makeWindow({
      bootstrap: { ...VALID_BOOTSTRAP, serviceUrl: "not a url" },
    });
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(errorSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("does not start the loop when location.origin differs from the service origin", () => {
    const win = makeWindow({ bootstrap: VALID_BOOTSTRAP, origin: "https://evil.example" });
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });

    expect(invoke).not.toHaveBeenCalled();
  });

  it("selects the gmail recipe for a mail.google.com service URL and starts the loop", async () => {
    const win = makeWindow({ bootstrap: VALID_BOOTSTRAP, origin: "https://mail.google.com" });
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });
    await flushMicrotasks();

    // gmail's stub read() resolves { count: null } immediately; the initial
    // report that follows proves the loop actually started.
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0]?.[0]).toMatchObject({ serviceId: "svc-1", recipeId: "gmail" });
  });

  it("falls back to the generic recipe for an unrelated service URL and starts the loop", async () => {
    const win = makeWindow({
      bootstrap: {
        ...VALID_BOOTSTRAP,
        serviceUrl: "https://webmail.example.com/",
      },
      origin: "https://webmail.example.com",
    });
    const invoke = noopInvoke();

    bootstrapAgent(win, { invoke, scheduler: noopScheduler });
    await flushMicrotasks();

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0]?.[0]).toMatchObject({ recipeId: "generic" });
  });
});
