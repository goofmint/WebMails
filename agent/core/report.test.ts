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
});
