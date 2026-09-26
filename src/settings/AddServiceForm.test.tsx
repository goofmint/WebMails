import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { AddServiceForm } from "./AddServiceForm";
import { createMockSettingsIpc } from "../test/mockSettingsIpc";
import { service } from "../test/fixtures";

function renderForm(services = [service()]) {
  const ipc = createMockSettingsIpc({
    settings: null,
    services,
    statuses: {},
    sidebarWidth: 64,
    activeServiceId: null,
  });
  render(<AddServiceForm ipc={ipc} services={services} />);
  return ipc;
}

function urlInput(): HTMLInputElement {
  return screen.getByLabelText("URL");
}
function nameInput(): HTMLInputElement {
  return screen.getByLabelText("Name");
}
function submitButton(): HTMLButtonElement {
  return screen.getByRole("button", { name: "Add" });
}

describe("AddServiceForm", () => {
  it("pre-fills the default profile for a Gmail URL", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/0/" } });

    expect(screen.getByRole("radio", { name: "Default (shared)" })).toBeChecked();
    expect(nameInput().value).toBe("mail.google.com");
  });

  it("pre-fills the isolated profile for an iCloud /mail URL", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://www.icloud.com/mail" } });

    expect(screen.getByRole("radio", { name: "Isolated (this service only)" })).toBeChecked();
  });

  it("pre-fills the isolated profile for a URL that matches no specific recipe", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://fastmail.example.com/" } });

    expect(screen.getByRole("radio", { name: "Isolated (this service only)" })).toBeChecked();
  });

  it("does not overwrite a manually chosen profile when the URL changes again", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/0/" } });
    expect(screen.getByRole("radio", { name: "Default (shared)" })).toBeChecked();

    fireEvent.click(screen.getByRole("radio", { name: "Isolated (this service only)" }));
    expect(screen.getByRole("radio", { name: "Isolated (this service only)" })).toBeChecked();

    // Changing the URL again to another Gmail-like URL must not reset the
    // manual choice back to "default".
    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/1/" } });
    expect(screen.getByRole("radio", { name: "Isolated (this service only)" })).toBeChecked();
  });

  it("does not overwrite a manually edited name when the URL changes again", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/0/" } });
    fireEvent.change(nameInput(), { target: { value: "My Gmail" } });

    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/1/" } });
    expect(nameInput().value).toBe("My Gmail");
  });

  it("disables submit for an invalid URL, and enables it once the URL is valid", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "not a url" } });
    fireEvent.change(nameInput(), { target: { value: "Something" } });
    expect(submitButton()).toBeDisabled();

    fireEvent.change(urlInput(), { target: { value: "https://example.com/" } });
    expect(submitButton()).not.toBeDisabled();
  });

  it("disables submit until a named profile's name is valid", () => {
    renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://example.com/" } });
    fireEvent.change(nameInput(), { target: { value: "Something" } });
    fireEvent.click(screen.getByRole("radio", { name: "Named" }));
    expect(submitButton()).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Profile name"), { target: { value: "My Team" } });
    expect(submitButton()).toBeDisabled();

    fireEvent.change(screen.getByLabelText("Profile name"), { target: { value: "team" } });
    expect(submitButton()).not.toBeDisabled();
  });

  it("submits add_service then select_service with the new id, and resets the form", async () => {
    const ipc = renderForm();
    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/0/" } });
    fireEvent.change(nameInput(), { target: { value: "Personal Gmail" } });

    fireEvent.click(submitButton());

    await waitFor(() => {
      expect(ipc.addService).toHaveBeenCalledWith(
        "Personal Gmail",
        "https://mail.google.com/mail/u/0/",
        "default",
      );
    });
    expect(ipc.selectService).toHaveBeenCalledWith("new-service");
    await waitFor(() => {
      expect(urlInput().value).toBe("");
    });
    expect(nameInput().value).toBe("");
  });

  it("shows the command error and keeps the form filled in when add_service fails", async () => {
    const ipc = renderForm();
    ipc.addService.mockRejectedValueOnce({ kind: "config", message: "url already in use" });

    fireEvent.change(urlInput(), { target: { value: "https://example.com/" } });
    fireEvent.change(nameInput(), { target: { value: "Example" } });
    fireEvent.click(submitButton());

    expect(await screen.findByRole("alert")).toHaveTextContent("url already in use");
    expect(ipc.selectService).not.toHaveBeenCalled();
    expect(urlInput().value).toBe("https://example.com/");
  });

  it("resets the form after a successful add even when select_service fails", async () => {
    const ipc = renderForm();
    ipc.selectService.mockRejectedValueOnce({ kind: "config", message: "cannot select service" });

    fireEvent.change(urlInput(), { target: { value: "https://mail.google.com/mail/u/0/" } });
    fireEvent.change(nameInput(), { target: { value: "Personal Gmail" } });
    fireEvent.click(submitButton());

    // The service was created — its form must not stay primed to submit
    // add_service again with the same values.
    await waitFor(() => {
      expect(urlInput().value).toBe("");
    });
    expect(nameInput().value).toBe("");
    expect(ipc.addService).toHaveBeenCalledWith(
      "Personal Gmail",
      "https://mail.google.com/mail/u/0/",
      "default",
    );

    // The selectService failure is reported on its own.
    expect(await screen.findByRole("alert")).toHaveTextContent("cannot select service");
  });
});
