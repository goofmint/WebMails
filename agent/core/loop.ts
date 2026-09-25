// The agent's watch/reconcile/report loop (design.md §2.2.14, §8.2).
import type { Recipe, RecipeContext, UnreadResult } from "../recipes/types";
import type { RandomSource, Scheduler } from "./clock";
import { systemRandom, systemScheduler } from "./clock";

const DEBOUNCE_MS = 500;

// Reconcile jitter: base * (0.8 + 0.4 * random()), i.e. within [0.8, 1.2)
// of the base interval, so services do not poll in lockstep (§8.2).
const JITTER_BASE_FACTOR = 0.8;
const JITTER_RANGE_FACTOR = 0.4;

export function computeJitteredDelay(baseMs: number, random: () => number): number {
  return baseMs * (JITTER_BASE_FACTOR + JITTER_RANGE_FACTOR * random());
}

export interface StartLoopOptions {
  readonly recipe: Recipe;
  readonly context: RecipeContext;
  // Reconcile timer base interval, jittered by ±20% on every reschedule.
  readonly reconcileIntervalMs: number;
  // Fixed interval at which the last known result is re-sent (no re-read).
  readonly reportIntervalMs: number;
  readonly report: (result: UnreadResult) => Promise<void> | void;
  readonly scheduler?: Scheduler;
  readonly randomSource?: RandomSource;
}

export type StopLoop = () => void;

// Starts the watch/reconcile/report loop for one recipe and returns a stop
// function that clears every timer and unsubscribes from `recipe.watch`.
export function startLoop(options: StartLoopOptions): StopLoop {
  const scheduler = options.scheduler ?? systemScheduler;
  const randomSource = options.randomSource ?? systemRandom;

  let stopped = false;
  let debounceTimer: number | undefined;
  let reconcileTimer: number | undefined;
  let reportTimer: number | undefined;

  // The last read result, used by the report timer. A read failure updates
  // this to `{ count: null }` too (design.md §5.1: failure -> count null),
  // so the periodic re-report keeps sending null until a later read
  // succeeds, instead of flapping between a stale count and null.
  let lastResult: UnreadResult | undefined;

  // Reads are serialized: a change observed while a read is already in
  // flight is queued and coalesced into a single follow-up read once the
  // in-flight one finishes.
  let readInFlight = false;
  let readQueued = false;

  const runRead = (): void => {
    if (stopped || readInFlight) {
      if (!stopped) {
        readQueued = true;
      }
      return;
    }
    readInFlight = true;
    void options.recipe
      .read(options.context)
      .then((result) => {
        if (stopped) {
          return;
        }
        lastResult = result;
        void options.report(result);
      })
      .catch((error) => {
        console.error("[eluma-agent] recipe read failed", error);
        if (!stopped) {
          lastResult = { count: null };
          void options.report(lastResult);
        }
      })
      .finally(() => {
        readInFlight = false;
        if (readQueued && !stopped) {
          readQueued = false;
          runRead();
        }
      });
  };

  const scheduleReconcile = (): void => {
    if (stopped) {
      return;
    }
    const delay = computeJitteredDelay(options.reconcileIntervalMs, () => randomSource.random());
    reconcileTimer = scheduler.setTimeout(() => {
      runRead();
      scheduleReconcile();
    }, delay);
  };

  const scheduleReportResend = (): void => {
    if (stopped) {
      return;
    }
    reportTimer = scheduler.setTimeout(() => {
      if (lastResult !== undefined) {
        void options.report(lastResult);
      }
      scheduleReportResend();
    }, options.reportIntervalMs);
  };

  const unsubscribeWatch = options.recipe.watch(options.context, () => {
    if (stopped) {
      return;
    }
    if (debounceTimer !== undefined) {
      scheduler.clearTimeout(debounceTimer);
    }
    debounceTimer = scheduler.setTimeout(() => {
      debounceTimer = undefined;
      runRead();
    }, DEBOUNCE_MS);
  });

  // Read once immediately so a first result exists as soon as possible,
  // rather than waiting for the first change or reconcile tick.
  runRead();
  scheduleReconcile();
  scheduleReportResend();

  return (): void => {
    if (stopped) {
      return;
    }
    stopped = true;
    if (debounceTimer !== undefined) {
      scheduler.clearTimeout(debounceTimer);
    }
    if (reconcileTimer !== undefined) {
      scheduler.clearTimeout(reconcileTimer);
    }
    if (reportTimer !== undefined) {
      scheduler.clearTimeout(reportTimer);
    }
    unsubscribeWatch();
  };
}
