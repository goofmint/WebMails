// Ambient globals installed by the host around the agent bundle.
import type { UnreadReportDto } from "./core/report";

// Injected by Task 2.2 before the agent bundle runs; read once by main.ts
// and deleted immediately after (design.md §2.2.14).
export interface ElumaBootstrap {
  readonly serviceId: string;
  readonly serviceUrl: string;
  readonly reportIntervalMs: number;
  readonly reconcileIntervalMs: number;
}

// Provided by the Tauri runtime in every real webview.
export interface TauriInternals {
  invoke(command: "report_unread", args: { report: UnreadReportDto }): Promise<void>;
}

declare global {
  interface Window {
    __ELUMA__?: ElumaBootstrap;
    __TAURI_INTERNALS__?: TauriInternals;
  }
}
