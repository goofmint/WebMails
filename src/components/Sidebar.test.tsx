import { describe, expect, it } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { ShellStoreProvider } from "../store/ShellStoreProvider";
import { createMockShellIpc } from "../test/mockShellIpc";
import { snapshot } from "../test/fixtures";
import { sidebarMetrics } from "./sidebarMetrics";

function renderSidebar(initialSnapshot = snapshot()) {
  const ipc = createMockShellIpc(initialSnapshot);
  render(
    <ShellStoreProvider ipc={ipc}>
      <Sidebar />
    </ShellStoreProvider>,
  );
  return ipc;
}

/** Fires the minimal HTML5 drag-and-drop event sequence Sidebar listens for. */
function fireDragAndDrop(source: Element, target: Element): void {
  fireEvent.dragStart(source);
  fireEvent.dragOver(target);
  fireEvent.drop(target);
  fireEvent.dragEnd(source);
}

describe("Sidebar", () => {
  it("marks the snapshot's activeServiceId as aria-current on initial render", async () => {
    renderSidebar(snapshot({ activeServiceId: "icloud" }));

    const gmail = await screen.findByRole("button", { name: "Gmail" });
    const icloud = screen.getByRole("button", { name: "iCloud" });
    expect(gmail).not.toHaveAttribute("aria-current");
    expect(icloud).toHaveAttribute("aria-current", "true");
  });

  it("selects a service on click and marks it aria-current", async () => {
    const ipc = renderSidebar();

    const gmail = await screen.findByRole("button", { name: "Gmail" });
    expect(gmail).not.toHaveAttribute("aria-current");

    fireEvent.click(gmail);

    expect(ipc.selectService).toHaveBeenCalledWith("gmail");
    expect(gmail).toHaveAttribute("aria-current", "true");
  });

  it("updates aria-current when a select-service event fires", async () => {
    const ipc = renderSidebar();
    const icloud = await screen.findByRole("button", { name: "iCloud" });
    expect(icloud).not.toHaveAttribute("aria-current");

    ipc.emitSelectService("icloud");

    await waitFor(() => {
      expect(icloud).toHaveAttribute("aria-current", "true");
    });
  });

  it("reorders services via drag-and-drop and calls reorderServices", async () => {
    const ipc = renderSidebar();
    await screen.findByRole("button", { name: "Gmail" });

    const gmailButton = screen.getByRole("button", { name: "Gmail" });
    const icloudButton = screen.getByRole("button", { name: "iCloud" });

    fireDragAndDrop(gmailButton, icloudButton);

    expect(ipc.reorderServices).toHaveBeenCalledWith(["icloud", "gmail"]);

    const buttons = screen.getAllByRole("button", { name: /Gmail|iCloud/ });
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual(["iCloud", "Gmail"]);
  });

  it("does not reorder when dropped on its own position", async () => {
    const ipc = renderSidebar();
    await screen.findByRole("button", { name: "Gmail" });

    const gmailButton = screen.getByRole("button", { name: "Gmail" });
    fireDragAndDrop(gmailButton, gmailButton);

    expect(ipc.reorderServices).not.toHaveBeenCalled();
  });

  it("refetches the snapshot when reorderServices fails", async () => {
    const ipc = renderSidebar();
    await screen.findByRole("button", { name: "Gmail" });

    ipc.reorderServices.mockRejectedValueOnce(new Error("nope"));
    ipc.getSnapshot.mockResolvedValueOnce(snapshot());

    const gmailButton = screen.getByRole("button", { name: "Gmail" });
    const icloudButton = screen.getByRole("button", { name: "iCloud" });
    fireDragAndDrop(gmailButton, icloudButton);

    await waitFor(() => {
      expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
    });
  });

  it("shows the config error screen instead of the list, with the gear button kept", async () => {
    const errorSnapshot = snapshot({
      services: [],
      settings: null,
      configError: {
        file: "/tmp/eluma/config.toml",
        key: "services[0].url",
        reason: "invalid scheme",
      },
    });
    renderSidebar(errorSnapshot);

    expect(await screen.findByText("/tmp/eluma/config.toml")).toBeInTheDocument();
    expect(screen.getByText("services[0].url")).toBeInTheDocument();
    expect(screen.getByText("invalid scheme")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Gmail" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Settings" })).toBeInTheDocument();
  });

  it("renders and updates a service's badge when a status-changed event fires", async () => {
    const ipc = renderSidebar();
    const gmail = await screen.findByRole("button", { name: "Gmail" });

    expect(within(gmail).queryByLabelText(/unread/)).not.toBeInTheDocument();

    ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });

    await waitFor(() => {
      expect(within(gmail).getByLabelText("3 unread")).toHaveTextContent("3");
    });

    ipc.emitStatusChanged("gmail", { kind: "ok", count: 1000 });

    await waitFor(() => {
      expect(within(gmail).getByLabelText("1000 unread")).toHaveTextContent("999+");
    });
  });

  it("renders no badge at all when settings.badge_sidebar is false", async () => {
    const { settings } = snapshot();
    if (settings === null) throw new Error("expected non-null settings");
    const ipc = renderSidebar(snapshot({ settings: { ...settings, badge_sidebar: false } }));
    const gmail = await screen.findByRole("button", { name: "Gmail" });

    act(() => {
      ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });
    });

    expect(within(gmail).queryByLabelText(/unread/)).not.toBeInTheDocument();
  });

  it("calls openSettings from both the add and settings buttons", async () => {
    const ipc = renderSidebar();
    await screen.findByRole("button", { name: "Gmail" });

    fireEvent.click(screen.getByRole("button", { name: "Add service" }));
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));

    expect(ipc.openSettings).toHaveBeenCalledTimes(2);
  });

  it("sizes each ServiceIcon using sidebarMetrics's derived iconSize for the default sidebarWidth", async () => {
    renderSidebar();
    const gmail = await screen.findByRole("button", { name: "Gmail" });

    const metrics = sidebarMetrics(64);
    expect(gmail.style.width).toBe(`${metrics.iconSize}px`);
    expect(gmail.style.height).toBe(`${metrics.iconSize}px`);
  });

  it("sizes each ServiceIcon using sidebarMetrics's derived iconSize for a non-default sidebarWidth", async () => {
    renderSidebar(snapshot({ sidebarWidth: 80 }));
    const gmail = await screen.findByRole("button", { name: "Gmail" });

    const metrics = sidebarMetrics(80);
    expect(gmail.style.width).toBe(`${metrics.iconSize}px`);
    expect(gmail.style.height).toBe(`${metrics.iconSize}px`);
  });

  it("renders 24px service icons with 40px action buttons and 20px glyphs at the 64px sidebar", async () => {
    renderSidebar();
    const gmail = await screen.findByRole("button", { name: "Gmail" });

    expect(gmail.style.width).toBe("24px");
    expect(gmail.style.height).toBe("24px");
    const aside = screen.getByRole("complementary", { name: "Services" });
    expect(aside.style.getPropertyValue("--sidebar-action-size")).toBe("40px");
    expect(aside.style.getPropertyValue("--sidebar-action-glyph-size")).toBe("20px");
  });

  it("sets sidebarMetrics's derived CSS custom properties on the aside element", async () => {
    renderSidebar();
    await screen.findByRole("button", { name: "Gmail" });

    const aside = screen.getByRole("complementary", { name: "Services" });
    const metrics = sidebarMetrics(64);
    expect(aside.style.getPropertyValue("--sidebar-icon-size")).toBe(`${metrics.iconSize}px`);
    expect(aside.style.getPropertyValue("--sidebar-list-padding")).toBe(`${metrics.listPadding}px`);
    expect(aside.style.getPropertyValue("--sidebar-list-gap")).toBe(`${metrics.listGap}px`);
    expect(aside.style.getPropertyValue("--sidebar-action-size")).toBe(
      `${metrics.actionButtonSize}px`,
    );
    expect(aside.style.getPropertyValue("--sidebar-action-glyph-size")).toBe(
      `${metrics.actionGlyphSize}px`,
    );
  });

  it("sets sidebarMetrics's derived CSS custom properties on the config error screen's root", async () => {
    const errorSnapshot = snapshot({
      services: [],
      settings: null,
      configError: {
        file: "/tmp/eluma/config.toml",
        key: null,
        reason: "invalid scheme",
      },
    });
    renderSidebar(errorSnapshot);
    await screen.findByText("/tmp/eluma/config.toml");

    const root = document.querySelector<HTMLElement>(".config-error-screen");
    expect(root).not.toBeNull();
    const metrics = sidebarMetrics(64);
    expect(root?.style.getPropertyValue("--sidebar-icon-size")).toBe(`${metrics.iconSize}px`);
    expect(root?.style.getPropertyValue("--sidebar-action-size")).toBe(
      `${metrics.actionButtonSize}px`,
    );
  });
});
