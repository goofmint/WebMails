import { afterEach, describe, expect, it, vi } from "vitest";
import type { UnreadResult } from "../recipes/types";
import type { ReportInvoke } from "./report";
import { createReporter } from "./report";

const SERVICE_URL = new URL("https://mail.example.com/inbox");

function makeDoc(html: string): Document {
  return new DOMParser().parseFromString(html, "text/html");
}

describe("createReporter", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("builds the DTO with every required key present", async () => {
    const invoke = vi.fn<ReportInvoke>().mockResolvedValue(undefined);
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "gmail",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 1_700_000_000_000 },
      invoke,
    });

    const result: UnreadResult = {
      count: 3,
      messages: [{ id: "m1", from: "a@example.com", subject: "hi", link: null }],
    };
    await report(result);

    expect(invoke).toHaveBeenCalledTimes(1);
    const dto = invoke.mock.calls[0]?.[0];
    expect(dto).toEqual({
      serviceId: "svc-1",
      count: 3,
      messages: result.messages,
      recipeId: "gmail",
      observedAt: 1_700_000_000_000,
      iconCandidates: ["https://mail.example.com/favicon.ico"],
    });
  });

  it("uses an empty messages array when count is null", async () => {
    const invoke = vi.fn<ReportInvoke>().mockResolvedValue(undefined);
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 0 },
      invoke,
    });

    await report({ count: null });

    const dto = invoke.mock.calls[0]?.[0];
    expect(dto).toMatchObject({ count: null, messages: [] });
  });

  it("attaches iconCandidates only to the first report, then empty arrays", async () => {
    const invoke = vi.fn<ReportInvoke>().mockResolvedValue(undefined);
    const doc = makeDoc(
      '<html><head><link rel="icon" href="/icon.png" sizes="32x32"></head><body></body></html>',
    );
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: doc,
      clock: { now: () => 0 },
      invoke,
    });

    await report({ count: 1, messages: [] });
    await report({ count: 2, messages: [] });

    const firstDto = invoke.mock.calls[0]?.[0];
    const secondDto = invoke.mock.calls[1]?.[0];
    expect(firstDto?.iconCandidates).toEqual([
      "https://mail.example.com/icon.png",
      "https://mail.example.com/favicon.ico",
    ]);
    expect(secondDto?.iconCandidates).toEqual([]);
  });

  it("still marks the first report as sent even when invoke fails", async () => {
    const invoke = vi.fn<ReportInvoke>().mockRejectedValue(new Error("bridge unavailable"));
    vi.spyOn(console, "error").mockImplementation(() => {});
    const doc = makeDoc(
      '<html><head><link rel="icon" href="/icon.png"></head><body></body></html>',
    );
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: doc,
      clock: { now: () => 0 },
      invoke,
    });

    await report({ count: 1, messages: [] });
    await report({ count: 2, messages: [] });

    const firstDto = invoke.mock.calls[0]?.[0];
    const secondDto = invoke.mock.calls[1]?.[0];
    expect(firstDto?.iconCandidates).toEqual([
      "https://mail.example.com/icon.png",
      "https://mail.example.com/favicon.ico",
    ]);
    expect(secondDto?.iconCandidates).toEqual([]);
  });

  it("logs and does not throw when invoke fails", async () => {
    const invoke = vi.fn<ReportInvoke>().mockRejectedValue(new Error("bridge unavailable"));
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 0 },
      invoke,
    });

    await expect(report({ count: 1, messages: [] })).resolves.toBeUndefined();
    expect(errorSpy).toHaveBeenCalled();
  });

  // `report()` always defers the actual `invoke` call by a microtask (see
  // report.ts's `drain`), so a report only becomes genuinely "in flight"
  // once that tick has run. These tests wait for that before firing more
  // reports, the way a real in-flight invoke would be observed.
  const tick = (): Promise<void> => Promise.resolve();

  it("does not start the second invoke until the first one settles, and preserves order", async () => {
    let resolveFirstInvoke: (() => void) | undefined;
    const invoke = vi
      .fn<ReportInvoke>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveFirstInvoke = resolve;
          }),
      )
      .mockResolvedValueOnce(undefined);
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 0 },
      invoke,
    });

    const firstPromise = report({ count: 1, messages: [] });

    // Let the first invoke actually start before firing the second
    // report, so it is genuinely in flight rather than still sitting in
    // the pending slot itself.
    await tick();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0]?.[0]).toMatchObject({ count: 1 });

    const secondPromise = report({ count: 2, messages: [] });

    // The second report must not have started yet since the first is
    // still pending.
    expect(invoke).toHaveBeenCalledTimes(1);

    resolveFirstInvoke?.();
    await firstPromise;
    await secondPromise;

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls[1]?.[0]).toMatchObject({ count: 2 });
  });

  it("does not let a failed invoke block a later one", async () => {
    let rejectFirstInvoke: ((error: Error) => void) | undefined;
    const invoke = vi
      .fn<ReportInvoke>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((_resolve, reject) => {
            rejectFirstInvoke = reject;
          }),
      )
      .mockResolvedValueOnce(undefined);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 0 },
      invoke,
    });

    const firstPromise = report({ count: 1, messages: [] });
    await tick();
    expect(invoke).toHaveBeenCalledTimes(1);

    const secondPromise = report({ count: 2, messages: [] });

    rejectFirstInvoke?.(new Error("boom"));
    await expect(firstPromise).resolves.toBeUndefined();
    await expect(secondPromise).resolves.toBeUndefined();

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls[0]?.[0]).toMatchObject({ count: 1 });
    expect(invoke.mock.calls[1]?.[0]).toMatchObject({ count: 2 });
    expect(errorSpy).toHaveBeenCalledTimes(1);
  });

  it("bounds the queue to one pending report: three more reports coalesce to the latest", async () => {
    let resolveFirstInvoke: (() => void) | undefined;
    const invoke = vi
      .fn<ReportInvoke>()
      .mockImplementationOnce(
        () =>
          new Promise<void>((resolve) => {
            resolveFirstInvoke = resolve;
          }),
      )
      .mockResolvedValueOnce(undefined);
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: makeDoc("<html><head></head><body></body></html>"),
      clock: { now: () => 0 },
      invoke,
    });

    const firstPromise = report({ count: 1, messages: [] });
    await tick();
    expect(invoke).toHaveBeenCalledTimes(1);

    // Three more reports arrive while the first invoke is still pending;
    // each newer one replaces the last, so only the final one is ever
    // sent.
    const secondPromise = report({ count: 2, messages: [] });
    const thirdPromise = report({ count: 3, messages: [] });
    const fourthPromise = report({ count: 4, messages: [] });

    resolveFirstInvoke?.();
    await firstPromise;
    await Promise.all([secondPromise, thirdPromise, fourthPromise]);

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(invoke.mock.calls[0]?.[0]).toMatchObject({ count: 1 });
    expect(invoke.mock.calls[1]?.[0]).toMatchObject({ count: 4 });
  });

  it("carries the icon candidates on the replacement when the report that would have been first is coalesced away", async () => {
    const invoke = vi.fn<ReportInvoke>().mockResolvedValue(undefined);
    const doc = makeDoc(
      '<html><head><link rel="icon" href="/icon.png"></head><body></body></html>',
    );
    const report = createReporter({
      serviceId: "svc-1",
      recipeId: "generic",
      serviceUrl: SERVICE_URL,
      document: doc,
      clock: { now: () => 0 },
      invoke,
    });

    // Fired back-to-back in the same tick, before the first report's
    // drain microtask has run: the first is superseded and never sent at
    // all, so it must never receive the icon candidates it would
    // otherwise have carried.
    const firstPromise = report({ count: 1, messages: [] });
    const secondPromise = report({ count: 2, messages: [] });

    await Promise.all([firstPromise, secondPromise]);

    expect(invoke).toHaveBeenCalledTimes(1);
    const dto = invoke.mock.calls[0]?.[0];
    expect(dto).toMatchObject({ count: 2 });
    expect(dto?.iconCandidates).toEqual([
      "https://mail.example.com/icon.png",
      "https://mail.example.com/favicon.ico",
    ]);
  });
});
