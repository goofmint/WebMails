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

// Builds a reporter closure bound to one service/recipe/page load.
// `iconCandidates` are computed once and attached only to the very first
// report this reporter ever sends (design.md §2.2.14: "iconCandidates are
// attached to the first report after load only"), regardless of whether
// that first invoke succeeds or fails. Every later report carries an empty
// array.
export function createReporter(options: CreateReporterOptions): Reporter {
  let hasSentFirstReport = false;

  // Serializes `invoke` calls: the loop fires reports without waiting for
  // the previous one to finish (loop.ts's `void options.report(result)`),
  // so without this a slow or failing `invoke` could let a later report
  // land at the Rust side before an earlier one. Each new invoke is
  // chained onto this tail and only starts once the previous one has
  // settled — success or failure — which also preserves the order
  // reports were produced in. The chain itself must never reject, or a
  // later report would be silently skipped.
  let invokeChain: Promise<void> = Promise.resolve();

  return function report(result: UnreadResult): Promise<void> {
    const iconCandidates = hasSentFirstReport
      ? []
      : collectIconCandidates(options.document, options.serviceUrl);
    hasSentFirstReport = true;

    const dto: UnreadReportDto =
      result.count === null
        ? {
            serviceId: options.serviceId,
            count: null,
            messages: [],
            recipeId: options.recipeId,
            observedAt: options.clock.now(),
            iconCandidates,
          }
        : {
            serviceId: options.serviceId,
            count: result.count,
            messages: result.messages,
            recipeId: options.recipeId,
            observedAt: options.clock.now(),
            iconCandidates,
          };

    const invocation = invokeChain.then(async () => {
      try {
        await options.invoke(dto);
      } catch (error) {
        // The loop continues regardless; the next report retries.
        console.error("[eluma-agent] report_unread invoke failed", error);
      }
    });
    invokeChain = invocation;
    return invocation;
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
