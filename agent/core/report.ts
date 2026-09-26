// Builds the report_unread DTO and sends it to Rust. See design.md
// §2.2.14 ("Loop") and the Task 2.1 DTO shape.
import type { MessageRef, UnreadResult } from "../recipes/types";
import type { Clock } from "./clock";
import { collectIconCandidates } from "./icons";

// The DTO sent through `report_unread` (Task 2.1's Rust-side shape). Every
// key is required; nullable keys are always present, never omitted.
export interface UnreadReportDto {
  readonly serviceId: string;
  readonly count: number | null;
  readonly messages: readonly MessageRef[];
  readonly recipeId: string;
  readonly observedAt: number;
  readonly iconCandidates: readonly string[];
}

export type ReportInvoke = (report: UnreadReportDto) => Promise<void>;

export interface CreateReporterOptions {
  readonly serviceId: string;
  readonly recipeId: string;
  readonly serviceUrl: URL;
  readonly document: Document;
  readonly clock: Clock;
  readonly invoke: ReportInvoke;
}

export type Reporter = (result: UnreadResult) => Promise<void>;

// One report snapshot waiting for its turn at `invoke`, plus the resolver
// for the promise `report()` returned when it was queued.
interface PendingReport {
  readonly result: UnreadResult;
  readonly observedAt: number;
  readonly resolve: () => void;
}

// Builds a reporter closure bound to one service/recipe/page load.
// `iconCandidates` are computed once and attached only to the very first
// report this reporter actually sends to `invoke` (design.md §2.2.14:
// "iconCandidates are attached to the first report after load only"),
// regardless of whether that first invoke succeeds or fails. Every later
// report carries an empty array.
export function createReporter(options: CreateReporterOptions): Reporter {
  let firstReportSent = false;
  let invokeInFlight = false;

  // The loop fires reports without waiting for the previous one to finish
  // (loop.ts's `void options.report(result)`), and a slow `invoke` can fall
  // behind a service that keeps changing. Rather than let an unbounded
  // backlog build up, at most one report waits behind the in-flight
  // invoke: a newer report replaces it outright (latest wins). Each report
  // is a full snapshot of the recipe's current state, not a delta, so an
  // older queued report carries no information the newer one that just
  // replaced it doesn't already make obsolete — dropping it loses nothing.
  //
  // There is deliberately no timeout on `invoke` itself: a stalled invoke
  // means no reports are reaching Rust at all, which the liveness monitor
  // (design.md's Stale handling) already detects independently — it marks
  // the service Stale and reloads its page, tearing down and recreating
  // this whole agent. A second, local timeout here would be redundant and
  // could only race that reload.
  let pending: PendingReport | undefined;

  const buildDto = (result: UnreadResult, observedAt: number): UnreadReportDto => {
    // Decided here, at the moment a report is actually handed to
    // `invoke` — never when `report()` is called — so that if the report
    // that would have been first is itself replaced before ever being
    // sent, whichever report replaces it (and so becomes the first one
    // this reporter actually sends) carries the icon candidates instead.
    const iconCandidates = firstReportSent
      ? []
      : collectIconCandidates(options.document, options.serviceUrl);
    firstReportSent = true;

    return result.count === null
      ? {
          serviceId: options.serviceId,
          count: null,
          messages: [],
          recipeId: options.recipeId,
          observedAt,
          iconCandidates,
        }
      : {
          serviceId: options.serviceId,
          count: result.count,
          messages: result.messages,
          recipeId: options.recipeId,
          observedAt,
          iconCandidates,
        };
  };

  // Sends whatever is currently pending, if nothing else is in flight, and
  // loops back for anything that arrived while this invoke was running.
  const drain = (): void => {
    if (invokeInFlight || pending === undefined) {
      return;
    }
    const { result, observedAt, resolve } = pending;
    pending = undefined;
    invokeInFlight = true;
    const dto = buildDto(result, observedAt);
    void options
      .invoke(dto)
      .catch((error) => {
        // The queue continues regardless; the next report retries.
        console.error("[eluma-agent] report_unread invoke failed", error);
      })
      .then(() => {
        invokeInFlight = false;
        resolve();
        drain();
      });
  };

  return function report(result: UnreadResult): Promise<void> {
    const observedAt = options.clock.now();
    return new Promise<void>((resolve) => {
      // Latest wins: a report still waiting in the pending slot is
      // superseded right here, before it was ever sent, so its promise
      // resolves now instead of waiting for a send that will never
      // happen.
      pending?.resolve();
      pending = { result, observedAt, resolve };
      void Promise.resolve().then(drain);
    });
  };
}

// Default invoke: calls into the real Tauri bridge. main.ts uses this
// unless a test injects its own `invoke`. The argument key must stay named
// `report` to match the Rust command's parameter name (Task 2.1).
export const defaultInvoke: ReportInvoke = async (report) => {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals) {
    throw new Error("[eluma-agent] window.__TAURI_INTERNALS__ is not available");
  }
  await internals.invoke("report_unread", { report });
};
