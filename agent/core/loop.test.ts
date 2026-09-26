import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Recipe, RecipeContext, UnreadResult } from "../recipes/types";
import { computeJitteredDelay, startLoop } from "./loop";

const fakeContext: RecipeContext = {
  serviceUrl: new URL("https://mail.example.com/"),
  document: document,
  fetch: () => Promise.reject(new Error("not used in these tests")),
};

function makeRecipe(read: () => Promise<UnreadResult>): {
  recipe: Recipe;
  triggerChange: () => void;
  unsubscribeSpy: ReturnType<typeof vi.fn>;
} {
  let onChange: (() => void) | undefined;
  const unsubscribeSpy = vi.fn();
  const recipe: Recipe = {
    id: "fake",
    displayName: "Fake",
    defaultProfile: "isolated",
    matches: () => true,
    describe: () => ({ strategy: "title", reads: "n/a" }),
    read,
    watch: (_ctx, cb) => {
      onChange = cb;
      return unsubscribeSpy;
    },
  };
  return {
    recipe,
    triggerChange: () => {
      onChange?.();
    },
    unsubscribeSpy,
  };
}

describe("computeJitteredDelay", () => {
  it("returns exactly 0.8x the base at random() = 0", () => {
    expect(computeJitteredDelay(1000, () => 0)).toBe(800);
  });

  it("stays under 1.2x the base as random() approaches 1", () => {
    const delay = computeJitteredDelay(1000, () => 0.999999999);
    expect(delay).toBeGreaterThanOrEqual(800);
    expect(delay).toBeLessThan(1200);
  });
});

describe("startLoop", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it("does not read on a change until the 500ms debounce elapses", async () => {
    const read = vi.fn().mockResolvedValue({ count: 1, messages: [] } satisfies UnreadResult);
    const { recipe, triggerChange } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 60_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);
    read.mockClear(); // discard the immediate initial read

    triggerChange();
    await vi.advanceTimersByTimeAsync(499);
    expect(read).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(read).toHaveBeenCalledTimes(1);

    stop();
  });

  it("coalesces consecutive changes into a single read", async () => {
    const read = vi.fn().mockResolvedValue({ count: 1, messages: [] } satisfies UnreadResult);
    const { recipe, triggerChange } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 60_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);
    read.mockClear();

    triggerChange();
    await vi.advanceTimersByTimeAsync(200);
    triggerChange();
    await vi.advanceTimersByTimeAsync(200);
    triggerChange();
    await vi.advanceTimersByTimeAsync(500);

    expect(read).toHaveBeenCalledTimes(1);

    stop();
  });

  it("reads immediately on start", async () => {
    const read = vi.fn().mockResolvedValue({ count: 0, messages: [] } satisfies UnreadResult);
    const { recipe } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 60_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);

    expect(read).toHaveBeenCalledTimes(1);
    expect(report).toHaveBeenCalledWith({ count: 0, messages: [] });

    stop();
  });

  it("reschedules the reconcile timer with a delay within the ±20% jitter band", async () => {
    const read = vi.fn().mockResolvedValue({ count: 2, messages: [] } satisfies UnreadResult);
    const { recipe } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 10_000,
      reportIntervalMs: 300_000,
      report,
      randomSource: { random: () => 0 }, // -> exactly 0.8x = 8000ms
    });
    await vi.advanceTimersByTimeAsync(0);
    read.mockClear();

    // Just under the jittered delay: no reconcile read yet.
    await vi.advanceTimersByTimeAsync(7_999);
    expect(read).not.toHaveBeenCalled();

    // At the jittered delay: the reconcile read fires.
    await vi.advanceTimersByTimeAsync(1);
    expect(read).toHaveBeenCalledTimes(1);

    stop();
  });

  it("re-sends the last result every reportIntervalMs without reading again", async () => {
    const read = vi.fn().mockResolvedValue({ count: 3, messages: [] } satisfies UnreadResult);
    const { recipe } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 10_000_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);
    expect(report).toHaveBeenCalledTimes(1);
    read.mockClear();
    report.mockClear();

    await vi.advanceTimersByTimeAsync(30_000);
    expect(read).not.toHaveBeenCalled();
    expect(report).toHaveBeenCalledTimes(1);
    expect(report).toHaveBeenCalledWith({ count: 3, messages: [] });

    // A second interval re-sends the same result again, still without a
    // new read.
    report.mockClear();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(read).not.toHaveBeenCalled();
    expect(report).toHaveBeenCalledTimes(1);
    expect(report).toHaveBeenCalledWith({ count: 3, messages: [] });

    stop();
  });

  it("sends a null periodic report once the first read has failed", async () => {
    const read = vi.fn().mockRejectedValue(new Error("boom"));
    const { recipe } = makeRecipe(read);
    const report = vi.fn();
    vi.spyOn(console, "error").mockImplementation(() => {});

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 10_000_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);
    report.mockClear();

    await vi.advanceTimersByTimeAsync(30_000);
    expect(report).toHaveBeenCalledWith({ count: null });

    stop();
  });

  it("does not send a periodic report before the first read has completed", async () => {
    let resolveRead: ((result: UnreadResult) => void) | undefined;
    const read = vi.fn(
      () =>
        new Promise<UnreadResult>((resolve) => {
          resolveRead = resolve;
        }),
    );
    const { recipe } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 10_000_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0); // the initial read starts but never resolves
    report.mockClear();

    await vi.advanceTimersByTimeAsync(30_000);
    expect(report).not.toHaveBeenCalled();

    resolveRead?.({ count: 9, messages: [] });
    await vi.advanceTimersByTimeAsync(0);
    expect(report).toHaveBeenCalledWith({ count: 9, messages: [] });

    stop();
  });

  it("reports count: null on a read failure but keeps the loop running", async () => {
    const read = vi.fn().mockRejectedValue(new Error("network down"));
    const { recipe, triggerChange } = makeRecipe(read);
    const report = vi.fn();
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 60_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);

    expect(report).toHaveBeenCalledWith({ count: null });
    expect(errorSpy).toHaveBeenCalled();
    report.mockClear();

    // The loop keeps running: a later change still triggers a read.
    triggerChange();
    await vi.advanceTimersByTimeAsync(500);
    expect(read).toHaveBeenCalledTimes(2);
    expect(report).toHaveBeenCalledWith({ count: null });

    stop();
  });

  it("a failed read is re-reported as null until a later read succeeds", async () => {
    const read = vi
      .fn<() => Promise<UnreadResult>>()
      .mockResolvedValueOnce({ count: 5, messages: [] })
      .mockRejectedValueOnce(new Error("transient"))
      .mockResolvedValueOnce({ count: 7, messages: [] });
    const { recipe, triggerChange } = makeRecipe(read);
    const report = vi.fn();
    vi.spyOn(console, "error").mockImplementation(() => {});

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 10_000_000,
      reportIntervalMs: 30_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0); // initial read succeeds: count 5
    report.mockClear();

    triggerChange();
    await vi.advanceTimersByTimeAsync(500); // second read fails
    expect(report).toHaveBeenCalledWith({ count: null });
    report.mockClear();

    // The periodic resend also reports null now: a persistently failing
    // recipe must not flap back to the stale, pre-failure count.
    await vi.advanceTimersByTimeAsync(30_000);
    expect(report).toHaveBeenCalledWith({ count: null });
    report.mockClear();

    // Once a later read succeeds, both the immediate and the periodic
    // report reflect the new result again.
    triggerChange();
    await vi.advanceTimersByTimeAsync(500);
    expect(report).toHaveBeenCalledWith({ count: 7, messages: [] });
    report.mockClear();

    await vi.advanceTimersByTimeAsync(30_000);
    expect(report).toHaveBeenCalledWith({ count: 7, messages: [] });

    stop();
  });

  it("stops all timers and unsubscribes from watch on stop()", async () => {
    const read = vi.fn().mockResolvedValue({ count: 1, messages: [] } satisfies UnreadResult);
    const { recipe, triggerChange, unsubscribeSpy } = makeRecipe(read);
    const report = vi.fn();

    const stop = startLoop({
      recipe,
      context: fakeContext,
      reconcileIntervalMs: 1_000,
      reportIntervalMs: 2_000,
      report,
      randomSource: { random: () => 0.5 },
    });
    await vi.advanceTimersByTimeAsync(0);
    read.mockClear();
    report.mockClear();

    stop();
    expect(unsubscribeSpy).toHaveBeenCalledTimes(1);

    triggerChange();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(read).not.toHaveBeenCalled();
    expect(report).not.toHaveBeenCalled();
  });
});
