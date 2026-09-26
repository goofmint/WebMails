import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { DeleteConfirm } from "./DeleteConfirm";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { service } from "../test/fixtures";

function renderConfirm(profile: string, onClose: () => void = () => {}) {
  const target = service({ profile });
  const ipc = createMockSettingsIpc({
    settings: null,
    services: [target],
    statuses: {},
    sidebarWidth: 64,
    activeServiceId: null,
  });
  render(<DeleteConfirm ipc={ipc} service={target} onClose={onClose} />);
  return { ipc, target };
}

describe("DeleteConfirm", () => {
  it("requires an explicit Delete click before removeService is called", () => {
    const { ipc } = renderConfirm("default");
    expect(ipc.removeService).not.toHaveBeenCalled();
  });

  it("shows the delete-session-data checkbox, unchecked by default, only for an isolated profile", () => {
    renderConfirm("isolated");
    const checkbox = screen.getByRole("checkbox", { name: "Delete session data" });
    expect(checkbox).not.toBeChecked();
  });

  it("shows no delete-session-data checkbox for the default profile", () => {
    renderConfirm("default");
    expect(screen.queryByRole("checkbox", { name: "Delete session data" })).not.toBeInTheDocument();
  });

  it("shows no delete-session-data checkbox for a named profile", () => {
    renderConfirm("work");
    expect(screen.queryByRole("checkbox", { name: "Delete session data" })).not.toBeInTheDocument();
  });

  it("passes deleteSessionData: false for a non-isolated profile even without a checkbox", async () => {
    const { ipc, target } = renderConfirm("default");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(ipc.removeService).toHaveBeenCalledWith(target.id, false);
    });
  });

  it("passes deleteSessionData: false for isolated when the checkbox is left unchecked", async () => {
    const { ipc, target } = renderConfirm("isolated");
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(ipc.removeService).toHaveBeenCalledWith(target.id, false);
    });
  });

  it("passes deleteSessionData: true for isolated when the checkbox is checked", async () => {
    const { ipc, target } = renderConfirm("isolated");
    fireEvent.click(screen.getByRole("checkbox", { name: "Delete session data" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(ipc.removeService).toHaveBeenCalledWith(target.id, true);
    });
  });

  it("calls onClose after a successful delete", async () => {
    let closed = false;
    renderConfirm("isolated", () => {
      closed = true;
    });
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => {
      expect(closed).toBe(true);
    });
  });

  it("calls onClose without deleting when Cancel is clicked", () => {
    let closed = false;
    const { ipc } = renderConfirm("isolated", () => {
      closed = true;
    });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(closed).toBe(true);
    expect(ipc.removeService).not.toHaveBeenCalled();
  });

  it("shows the command error returned by remove_service", async () => {
    const { ipc } = renderConfirm("isolated");
    ipc.removeService.mockRejectedValueOnce({ kind: "profile", message: "in use elsewhere" });

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("in use elsewhere");
  });
});
