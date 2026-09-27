import { describe, expect, it } from "vitest";
import { act } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { SettingsApp } from "./SettingsApp";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { service, snapshot } from "../test/fixtures";
import type { Snapshot } from "../ipc";

// Task 5.1's `RecipePanel` also renders each service's name (and, for
// Gmail, a recipe display name that happens to read "Gmail" too), so an
// unscoped `getByText`/`findByText` for a service's name would now match
// more than one element. These tests are about `ServiceList`
// (Task 1.12), so they scope every such query to that list specifically.
// `SettingsApp` never unmounts `ServiceList` once loaded (its `status`
// only ever moves from `"loading"`/`"error"` to `"ready"`, never back), so
// the `<ul class="service-list">` node found here stays valid across a
// test's later re-renders.
async function findServiceList(): Promise<HTMLElement> {
  return waitFor(() => {
    const list = document.querySelector<HTMLElement>(".service-list");
    if (list === null) {
      throw new Error("expected a .service-list element to be rendered");
    }
    return list;
  });
}

describe("SettingsApp", () => {
  it("fetches the snapshot on mount and renders the service list", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    render(<SettingsApp ipc={ipc} />);

    const list = await findServiceList();
    expect(within(list).getByText("Gmail")).toBeInTheDocument();
    expect(within(list).getByText("iCloud")).toBeInTheDocument();
    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
  });

  it("re-fetches the snapshot when a services-changed event fires", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    render(<SettingsApp ipc={ipc} />);
    const list = await findServiceList();
    await within(list).findByText("Gmail");

    ipc.getSnapshot.mockResolvedValueOnce(
      snapshot({ services: [service({ id: "outlook", name: "Outlook" })] }),
    );
    ipc.emitServicesChanged();

    await waitFor(() => {
      expect(within(list).getByText("Outlook")).toBeInTheDocument();
    });
    expect(within(list).queryByText("Gmail")).not.toBeInTheDocument();
  });

  it("ignores a stale getSnapshot response that resolves after a newer request", async () => {
    const ipc = createMockSettingsIpc(snapshot());

    let resolveFirst: ((value: Snapshot) => void) | undefined;
    ipc.getSnapshot.mockReturnValueOnce(
      new Promise<Snapshot>((resolve) => {
        resolveFirst = resolve;
      }),
    );

    render(<SettingsApp ipc={ipc} />);
    // The mount's initial refresh is now pending on `resolveFirst`.

    ipc.getSnapshot.mockResolvedValueOnce(
      snapshot({ services: [service({ id: "outlook", name: "Outlook" })] }),
    );
    ipc.emitServicesChanged();

    const list = await findServiceList();
    await waitFor(() => {
      expect(within(list).getByText("Outlook")).toBeInTheDocument();
    });

    // The stale first request now resolves with an older snapshot; being
    // the older (no longer latest) request, it must not overwrite the
    // newer state already rendered above.
    await act(async () => {
      resolveFirst?.(snapshot({ services: [service({ id: "gmail", name: "Gmail" })] }));
      await Promise.resolve();
    });

    expect(within(list).getByText("Outlook")).toBeInTheDocument();
    expect(within(list).queryByText("Gmail")).not.toBeInTheDocument();
  });

  it("shows the configError and hides the CRUD forms", async () => {
    const ipc = createMockSettingsIpc(
      snapshot({
        services: [],
        settings: null,
        configError: {
          file: "/tmp/eluma/config.toml",
          key: "services[0].url",
          reason: "invalid scheme",
        },
      }),
    );
    render(<SettingsApp ipc={ipc} />);

    expect(await screen.findByText("/tmp/eluma/config.toml")).toBeInTheDocument();
    expect(screen.getByText("services[0].url")).toBeInTheDocument();
    expect(screen.getByText("invalid scheme")).toBeInTheDocument();
    expect(screen.queryByRole("form", { name: "Add service" })).not.toBeInTheDocument();
  });

  it("shows a getSnapshot failure as a command error", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    ipc.getSnapshot.mockRejectedValueOnce({ kind: "config", message: "cannot read config" });
    render(<SettingsApp ipc={ipc} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("cannot read config");
  });

  it("closes the edit form when its target service disappears from the list", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    render(<SettingsApp ipc={ipc} />);
    const list = await findServiceList();
    await within(list).findByText("Gmail");

    fireEvent.click(screen.getAllByRole("button", { name: "Edit" })[0] as HTMLElement);
    expect(screen.getByRole("form", { name: "Edit Gmail" })).toBeInTheDocument();

    ipc.getSnapshot.mockResolvedValueOnce(
      snapshot({ services: [service({ id: "icloud", name: "iCloud" })] }),
    );
    ipc.emitServicesChanged();

    await waitFor(() => {
      expect(screen.queryByRole("form", { name: "Edit Gmail" })).not.toBeInTheDocument();
    });
  });
});
