import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsApp } from "./SettingsApp";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { service, snapshot } from "../test/fixtures";

describe("SettingsApp", () => {
  it("fetches the snapshot on mount and renders the service list", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    render(<SettingsApp ipc={ipc} />);

    expect(await screen.findByText("Gmail")).toBeInTheDocument();
    expect(screen.getByText("iCloud")).toBeInTheDocument();
    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
  });

  it("re-fetches the snapshot when a services-changed event fires", async () => {
    const ipc = createMockSettingsIpc(snapshot());
    render(<SettingsApp ipc={ipc} />);
    await screen.findByText("Gmail");

    ipc.getSnapshot.mockResolvedValueOnce(
      snapshot({ services: [service({ id: "outlook", name: "Outlook" })] }),
    );
    ipc.emitServicesChanged();

    await waitFor(() => {
      expect(screen.getByText("Outlook")).toBeInTheDocument();
    });
    expect(screen.queryByText("Gmail")).not.toBeInTheDocument();
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
    await screen.findByText("Gmail");

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
