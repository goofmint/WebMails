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
  const view = render(<GlobalSettingsForm ipc={ipc} settings={settings} />);
  return { ipc, settings, view };
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

  it("keeps a persistent status region mounted and empty until a save succeeds", () => {
    renderForm();

    const status = screen.getByRole("status");
    expect(status).toBeInTheDocument();
    expect(status).toHaveTextContent("");
  });

  it("shows 'Saved.' in the status region only once justSaved is true and there is no error", async () => {
    const { ipc } = renderForm();

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => {
      expect(ipc.updateSettings).toHaveBeenCalled();
    });
    expect(await screen.findByRole("status")).toHaveTextContent("Saved.");
  });

  it("syncs the saved baseline and untouched fields when the settings prop changes, but keeps a dirty edit", () => {
    const { ipc, settings, view } = renderForm();

    // Dirty: edit the reconcile interval locally without saving.
    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "999" },
    });

    const nextSettings: Settings = {
      ...settings,
      reconcile_interval_seconds: 42,
      notification_batch_threshold: 9,
    };
    view.rerender(<GlobalSettingsForm ipc={ipc} settings={nextSettings} />);

    // The dirty field keeps the user's in-progress edit...
    expect(screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)").value).toBe(
      "999",
    );
    // ...but an untouched field picks up the new prop value.
    expect(screen.getByLabelText<HTMLInputElement>("Notification batch threshold").value).toBe("9");

    // The new prop value is also the new baseline: saving submits only
    // the still-dirty field, since the untouched one already matches it.
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(ipc.updateSettings).toHaveBeenCalledWith({ reconcile_interval_seconds: 999 });
  });

  it("disables every input while a save is in flight", async () => {
    const { ipc } = renderForm();
    let resolveUpdate: (settings: Settings) => void = () => {};
    ipc.updateSettings.mockImplementationOnce(
      () =>
        new Promise<Settings>((resolve) => {
          resolveUpdate = resolve;
        }),
    );

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)")).toBeDisabled();
    expect(screen.getByLabelText<HTMLInputElement>("Notification batch threshold")).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Notifications" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: "Badge sidebar" })).toBeDisabled();

    resolveUpdate({
      reconcile_interval_seconds: 120,
      notifications: true,
      notification_batch_threshold: 5,
      badge_sidebar: true,
    });

    await waitFor(() => {
      expect(
        screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)"),
      ).not.toBeDisabled();
    });
  });

  it("clears a shown error as soon as any input changes", async () => {
    renderForm().ipc.updateSettings.mockRejectedValueOnce({
      kind: "config",
      message: "invalid settings",
    });

    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "120" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("invalid settings")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("checkbox", { name: "Badge sidebar" }));

    expect(screen.queryByText("invalid settings")).not.toBeInTheDocument();
  });

  it("marks an invalid numeric field with aria-invalid and points aria-describedby at its error", () => {
    renderForm();
    const input = screen.getByLabelText<HTMLInputElement>("Reconcile interval (seconds)");

    expect(input).toHaveAttribute("aria-required", "true");
    expect(input).not.toHaveAttribute("aria-describedby");
    expect(input.getAttribute("aria-invalid")).toBe("false");

    fireEvent.change(input, { target: { value: "" } });

    expect(input.getAttribute("aria-invalid")).toBe("true");
    const describedBy = input.getAttribute("aria-describedby");
    expect(describedBy).toBeTruthy();
    const errorEl = describedBy === null ? null : document.getElementById(describedBy);
    expect(errorEl).toHaveTextContent("Enter a whole number between 0 and 4294967295.");
  });

  it("gives the two numeric fields distinct error ids", () => {
    renderForm();
    fireEvent.change(screen.getByLabelText("Reconcile interval (seconds)"), {
      target: { value: "" },
    });
    fireEvent.change(screen.getByLabelText("Notification batch threshold"), {
      target: { value: "" },
    });

    const reconcileDescribedBy = screen
      .getByLabelText<HTMLInputElement>("Reconcile interval (seconds)")
      .getAttribute("aria-describedby");
    const thresholdDescribedBy = screen
      .getByLabelText<HTMLInputElement>("Notification batch threshold")
      .getAttribute("aria-describedby");

    expect(reconcileDescribedBy).toBeTruthy();
    expect(thresholdDescribedBy).toBeTruthy();
    expect(reconcileDescribedBy).not.toBe(thresholdDescribedBy);
  });
});
