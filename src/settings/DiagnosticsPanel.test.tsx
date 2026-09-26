import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { DiagnosticsPanel } from "./DiagnosticsPanel";
import type { Diagnostics } from "../ipc/types";

/**
 * `DiagnosticsPanel` calls `ipc/commands`'s `getDiagnostics()` and
 * `ipc/events`'s `onServicesChanged()` directly (module doc has why), so
 * this test mocks their shared underlying transport, `@tauri-apps/api/core`,
 * rather than the `SettingsIpc` prop the other settings components use.
 * `onServicesChanged` (`@tauri-apps/api/event`'s `listen`) itself calls
 * `invoke('plugin:event|listen', ...)` and `transformCallback(...)`
 * internally, so both are stubbed here too, even though no test below
 * exercises the event firing.
 */
const { invokeMock } = vi.hoisted(() => ({
  invokeMock: vi.fn<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: Record<string, unknown>) => invokeMock(cmd, args),
  transformCallback: () => 0,
}));

function mockGetDiagnostics(resolve: () => Promise<Diagnostics>): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "get_diagnostics") {
      return resolve();
    }
    if (cmd === "plugin:event|listen") {
      return Promise.resolve(1);
    }
    if (cmd === "plugin:event|unlisten") {
      return Promise.resolve(undefined);
    }
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
}

const THREE_SERVICE_DIAGNOSTICS: Diagnostics = {
  services: [
    {
      serviceId: "gmail",
      name: "Gmail",
      status: { kind: "ok", count: 3 },
      lastReportAgeMs: 12_000,
      staleCount: 0,
      lastStaleAt: null,
    },
    {
      serviceId: "icloud",
      name: "iCloud",
      status: { kind: "loading" },
      lastReportAgeMs: null,
      staleCount: 0,
      lastStaleAt: null,
    },
    {
      serviceId: "outlook",
      name: "Outlook",
      status: { kind: "needsAttention", reason: "reportedNone" },
      lastReportAgeMs: 5_000,
      staleCount: 1,
      lastStaleAt: 1_700_000_000_000,
    },
    {
      serviceId: "yahoo",
      name: "Yahoo",
      status: { kind: "stale" },
      lastReportAgeMs: 130_000,
      staleCount: 2,
      lastStaleAt: 1_700_000_100_000,
    },
  ],
};

describe("DiagnosticsPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("renders a row per service, including never-reported and stale services", async () => {
    mockGetDiagnostics(() => Promise.resolve(THREE_SERVICE_DIAGNOSTICS));

    render(<DiagnosticsPanel />);

    expect(await screen.findByText("Gmail")).toBeInTheDocument();
    expect(screen.getByText("12s ago")).toBeInTheDocument();

    expect(screen.getByText("iCloud")).toBeInTheDocument();
    expect(screen.getByText("Never reported")).toBeInTheDocument();

    expect(screen.getByText("Outlook")).toBeInTheDocument();
    expect(screen.getByText("Yahoo")).toBeInTheDocument();
  });

  it("gives Stale a badge distinct from NeedsAttention", async () => {
    mockGetDiagnostics(() => Promise.resolve(THREE_SERVICE_DIAGNOSTICS));

    render(<DiagnosticsPanel />);
    await screen.findByText("Yahoo");

    const staleBadge = screen.getByText("Stale");
    const needsAttentionBadge = screen.getByText("Needs attention");

    expect(staleBadge.className).toContain("diagnostics__badge--stale");
    expect(needsAttentionBadge.className).toContain("diagnostics__badge--needsAttention");
    expect(staleBadge.className).not.toEqual(needsAttentionBadge.className);
  });

  it("shows null last-report age as never reported, and a null last-stale-at as Never", async () => {
    mockGetDiagnostics(() => Promise.resolve(THREE_SERVICE_DIAGNOSTICS));

    render(<DiagnosticsPanel />);
    await screen.findByText("iCloud");

    // "iCloud" has never reported and never gone stale.
    const row = screen.getByText("iCloud").closest("tr");
    expect(row).not.toBeNull();
    expect(row?.textContent).toContain("Never reported");
    expect(row?.textContent).toContain("Never");
  });

  it("refetches when the refresh button is clicked", async () => {
    mockGetDiagnostics(() => Promise.resolve(THREE_SERVICE_DIAGNOSTICS));

    render(<DiagnosticsPanel />);
    await screen.findByText("Gmail");

    const callsAfterMount = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "get_diagnostics",
    ).length;
    expect(callsAfterMount).toBeGreaterThanOrEqual(1);

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

    await new Promise((resolve) => setTimeout(resolve, 0));
    const callsAfterRefresh = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === "get_diagnostics",
    ).length;
    expect(callsAfterRefresh).toBeGreaterThan(callsAfterMount);
  });

  it("shows the command error when get_diagnostics fails", async () => {
    // `toCommandError` only inspects `.kind`/`.message`, not `instanceof
    // Error`, but the rejection reason itself must still be an `Error`
    // (lint rule `prefer-promise-reject-errors`).
    const commandError = Object.assign(new Error("state.json is corrupt"), { kind: "state" });
    mockGetDiagnostics(() => Promise.reject(commandError));

    render(<DiagnosticsPanel />);

    expect(await screen.findByRole("alert")).toHaveTextContent("state.json is corrupt");
  });
});
