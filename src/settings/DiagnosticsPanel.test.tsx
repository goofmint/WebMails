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
    if (row === null) {
      throw new Error("expected the iCloud row to exist");
    }
    const cells = Array.from(row.querySelectorAll("td"));
    expect(cells).toHaveLength(5);
    const [, , lastReportCell, , lastStaleCell] = cells;
    if (lastReportCell === undefined || lastStaleCell === undefined) {
      throw new Error("expected 5 cells in the iCloud row");
    }
    expect(lastReportCell.textContent).toBe("Never reported");
    // Index 4 is the last-stale cell; it must be exactly "Never" (not merely
    // containing it) when `lastStaleAt` is null.
    expect(lastStaleCell.textContent).toBe("Never");
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

  it("keeps the newer result when two overlapping refreshes resolve out of order", async () => {
    // Mount issues the first `get_diagnostics` call; clicking Refresh while
    // it is still pending issues a second, newer one. Resolving the newer
    // (second) request before the older (first) one must still leave the
    // newer result on screen — the stale, out-of-order response from the
    // first request must be ignored even though it settles last.
    type Deferred = {
      readonly promise: Promise<Diagnostics>;
      readonly resolve: (value: Diagnostics) => void;
    };
    function createDeferred(): Deferred {
      let resolve!: (value: Diagnostics) => void;
      const promise = new Promise<Diagnostics>((res) => {
        resolve = res;
      });
      return { promise, resolve };
    }

    const deferreds: Deferred[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_diagnostics") {
        const deferred = createDeferred();
        deferreds.push(deferred);
        return deferred.promise;
      }
      if (cmd === "plugin:event|listen") {
        return Promise.resolve(1);
      }
      if (cmd === "plugin:event|unlisten") {
        return Promise.resolve(undefined);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });

    const OLDER_RESULT: Diagnostics = {
      services: [
        {
          serviceId: "gmail",
          name: "Older result",
          status: { kind: "ok", count: 1 },
          lastReportAgeMs: 1_000,
          staleCount: 0,
          lastStaleAt: null,
        },
      ],
    };
    const NEWER_RESULT: Diagnostics = {
      services: [
        {
          serviceId: "gmail",
          name: "Newer result",
          status: { kind: "ok", count: 2 },
          lastReportAgeMs: 2_000,
          staleCount: 0,
          lastStaleAt: null,
        },
      ],
    };

    render(<DiagnosticsPanel />);

    // Wait for the mount-triggered (first, older) request to be issued.
    await vi.waitFor(() => expect(deferreds).toHaveLength(1));

    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));

    // Wait for the click-triggered (second, newer) request to be issued.
    await vi.waitFor(() => expect(deferreds).toHaveLength(2));

    const [olderRequest, newerRequest] = deferreds;
    if (olderRequest === undefined || newerRequest === undefined) {
      throw new Error("expected two get_diagnostics requests to have been issued");
    }

    // Resolve out of order: the newer request settles first.
    newerRequest.resolve(NEWER_RESULT);
    await screen.findByText("Newer result");

    // The older request settles after — it must not overwrite the newer
    // result that's already on screen.
    olderRequest.resolve(OLDER_RESULT);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(screen.getByText("Newer result")).toBeInTheDocument();
    expect(screen.queryByText("Older result")).not.toBeInTheDocument();
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
