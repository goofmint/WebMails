import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { EditServiceForm } from "./EditServiceForm";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { service } from "../test/fixtures";

function renderForm(overrides: Parameters<typeof service>[0] = {}) {
  const target = service(overrides);
  const ipc = createMockSettingsIpc({
    settings: null,
    services: [target],
    statuses: {},
    sidebarWidth: 64,
    activeServiceId: null,
  });
  const onClose = () => {};
  render(
    <EditServiceForm ipc={ipc} service={target} existingNamedProfiles={[]} onClose={onClose} />,
  );
  return { ipc, target };
}

describe("EditServiceForm", () => {
  it("pre-fills every field from the given service", () => {
    renderForm({
      name: "Gmail",
      url: "https://mail.google.com/",
      profile: "default",
      notifications: true,
    });

    expect(screen.getByLabelText<HTMLInputElement>("Name").value).toBe("Gmail");
    expect(screen.getByLabelText<HTMLInputElement>("URL").value).toBe("https://mail.google.com/");
    expect(screen.getByRole("radio", { name: "Default (shared)" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Notifications" })).toBeChecked();
  });

  it("disables submit when nothing has changed", () => {
    renderForm();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("submits a patch containing only the changed name field", async () => {
    const { ipc, target } = renderForm({ name: "Gmail" });

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Gmail Renamed" } });
    expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateService).toHaveBeenCalledWith(target.id, { name: "Gmail Renamed" });
    });
  });

  it("submits only the toggled notifications field", async () => {
    const { ipc, target } = renderForm({ notifications: true });

    fireEvent.click(screen.getByRole("checkbox", { name: "Notifications" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateService).toHaveBeenCalledWith(target.id, { notifications: false });
    });
  });

  it("submits only the changed url field, and shows the recreate-page notice", () => {
    renderForm({ url: "https://mail.google.com/" });

    fireEvent.change(screen.getByLabelText("URL"), {
      target: { value: "https://mail.google.com/mail/u/1/" },
    });

    expect(screen.getByText(/recreates this service/)).toBeInTheDocument();
  });

  it("submits only the changed profile field when switching to isolated", async () => {
    const { ipc, target } = renderForm({ profile: "default" });

    fireEvent.click(screen.getByRole("radio", { name: "Isolated (this service only)" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateService).toHaveBeenCalledWith(target.id, { profile: "isolated" });
    });
  });

  it("keeps submit disabled while a named profile's name is invalid", () => {
    renderForm({ profile: "default" });

    fireEvent.click(screen.getByRole("radio", { name: "Named" }));
    fireEvent.change(screen.getByLabelText("Profile name"), { target: { value: "Bad Name" } });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the command error returned by update_service", async () => {
    const { ipc } = renderForm({ name: "Gmail" });
    ipc.updateService.mockRejectedValueOnce({ kind: "config", message: "name already used" });

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Gmail 2" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("name already used");
  });

  it("calls onClose without submitting when Cancel is clicked", () => {
    const target = service({ name: "Gmail" });
    const ipc = createMockSettingsIpc({
      settings: null,
      services: [target],
      statuses: {},
      sidebarWidth: 64,
      activeServiceId: null,
    });
    let closed = false;
    render(
      <EditServiceForm
        ipc={ipc}
        service={target}
        existingNamedProfiles={[]}
        onClose={() => {
          closed = true;
        }}
      />,
    );

    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Something else" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(closed).toBe(true);
    expect(ipc.updateService).not.toHaveBeenCalled();
  });
});
