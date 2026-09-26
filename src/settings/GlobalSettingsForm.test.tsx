import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { GlobalSettingsForm } from "./GlobalSettingsForm";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { snapshot } from "../test/fixtures";
import type { Settings } from "../ipc";

const DEFAULT_SETTINGS: Settings = {
  reconcile_interval_seconds: 60,
  notifications: true,
  notification_batch_threshold: 5,
  badge_sidebar: true,
};

function renderForm(overrides: Partial<Settings> = {}) {
  const settings: Settings = { ...DEFAULT_SETTINGS, ...overrides };
  const ipc = createMockSettingsIpc(snapshot({ settings }));
  render(<GlobalSettingsForm ipc={ipc} settings={settings} />);
  return { ipc, settings };
}

describe("GlobalSettingsForm", () => {
  it("pre-fills every field from the given settings", () => {
    renderForm({
      reconcile_interval_seconds: 90,
      notifications: false,
      notification_batch_threshold: 8,
      badge_sidebar: false,
    });

    expect(screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)").value).toBe(
      "90",
    );
    expect(screen.getByRole("checkbox", { name: "Notifications" })).not.toBeChecked();
    expect(screen.getByLabelText<HTMLInputElement>("Notification batch threshold").value).toBe("8");
    expect(screen.getByRole("checkbox", { name: "Badge sidebar" })).not.toBeChecked();
  });

  it("disables submit when nothing has changed", () => {
    renderForm();
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("submits a patch containing only the changed field, as a number", async () => {
    const { ipc } = renderForm();

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateSettings).toHaveBeenCalledWith({ reconcile_interval_seconds: 120 });
    });
  });

  it("submits only the toggled checkbox fields", async () => {
    const { ipc } = renderForm({ notifications: true, badge_sidebar: true });

    fireEvent.click(screen.getByRole("checkbox", { name: "Notifications" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Badge sidebar" }));
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateSettings).toHaveBeenCalledWith({
        notifications: false,
        badge_sidebar: false,
      });
    });
  });

  it("updates the saved baseline and editable values from the returned settings on success", async () => {
    const { ipc } = renderForm();
    ipc.updateSettings.mockResolvedValueOnce({
      reconcile_interval_seconds: 120,
      notifications: true,
      notification_batch_threshold: 5,
      badge_sidebar: true,
    });

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await screen.findByText("Saved.");
    expect(screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)").value).toBe(
      "120",
    );
    // Nothing left to submit against the new baseline.
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("blocks submission and shows an error for an empty numeric field", () => {
    renderForm();

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "" },
    });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByText("Enter a whole number between 0 and 4294967295.")).toBeInTheDocument();
  });

  it("blocks submission for a decimal value", () => {
    renderForm();

    fireEvent.change(screen.getByLabelText("Notification batch threshold"), {
      target: { value: "5.5" },
    });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("blocks submission for a value outside u32's range", () => {
    renderForm();

    fireEvent.change(screen.getByLabelText("Notification batch threshold"), {
      target: { value: "4294967296" },
    });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("blocks submission for a negative value", () => {
    renderForm();

    fireEvent.change(screen.getByLabelText("Notification batch threshold"), {
      target: { value: "-1" },
    });

    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  });

  it("shows the command error returned by update_settings and keeps the edited value", async () => {
    const { ipc } = renderForm();
    ipc.updateSettings.mockRejectedValueOnce({ kind: "config", message: "invalid settings" });

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("invalid settings")).toBeInTheDocument();
    expect(screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)").value).toBe(
      "120",
    );
  });
});
