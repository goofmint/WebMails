import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { ServiceIcon } from "./ServiceIcon";
import { service } from "../test/fixtures";
import type { ServiceStatus } from "../ipc";

// `convertFileSrc` calls into `window.__TAURI_INTERNALS__`, which does not
// exist outside a real Tauri webview (jsdom has no such global) — mocked
// here to a simple, inspectable transform instead.
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (path: string) => `asset://localhost/${path}`,
}));

function noop() {
  /* no-op */
}

const handlers = {
  onSelect: noop,
  onDragStart: noop,
  onDragOverTarget: noop,
  onDrop: noop,
  onDragEnd: noop,
};

describe("ServiceIcon", () => {
  it("renders the cached PNG through convertFileSrc, with a cache-busting version", () => {
    render(
      <ServiceIcon
        service={service({ id: "gmail", name: "Gmail" })}
        cachedIcon={{ path: "/tmp/eluma/data/icons/gmail.png", version: 7 }}
        selected={false}
        size={64}
        draggable={false}
        isDropTarget={false}
        status={undefined}
        badgesEnabled={true}
        {...handlers}
      />,
    );

    const img = screen.getByRole("button", { name: "Gmail" }).querySelector("img");
    expect(img).not.toBeNull();
    expect(img?.getAttribute("src")).toBe("asset://localhost//tmp/eluma/data/icons/gmail.png?v=7");
  });

  it("renders the generated letter icon when there is no cached icon", () => {
    render(
      <ServiceIcon
        service={service({ id: "gmail", name: "Gmail" })}
        cachedIcon={undefined}
        selected={false}
        size={64}
        draggable={false}
        isDropTarget={false}
        status={undefined}
        badgesEnabled={true}
        {...handlers}
      />,
    );

    const button = screen.getByRole("button", { name: "Gmail" });
    expect(button.querySelector("img")).toBeNull();
    expect(button.querySelector(".service-icon__initial")?.textContent).toBe("G");
  });

  it("falls back to the letter icon when the cached image fails to load", () => {
    render(
      <ServiceIcon
        service={service({ id: "gmail", name: "Gmail" })}
        cachedIcon={{ path: "/tmp/eluma/data/icons/gmail.png", version: 1 }}
        selected={false}
        size={64}
        draggable={false}
        isDropTarget={false}
        status={undefined}
        badgesEnabled={true}
        {...handlers}
      />,
    );

    const button = screen.getByRole("button", { name: "Gmail" });
    const img = button.querySelector("img");
    expect(img).not.toBeNull();
    if (img === null) throw new Error("expected an img element");
    fireEvent.error(img);

    expect(button.querySelector("img")).toBeNull();
    expect(button.querySelector(".service-icon__initial")?.textContent).toBe("G");
  });

  it("gives the letter icon a deterministic background colour derived from the service id", () => {
    render(
      <ServiceIcon
        service={service({ id: "gmail-personal", name: "Gmail" })}
        cachedIcon={undefined}
        selected={false}
        size={64}
        draggable={false}
        isDropTarget={false}
        status={undefined}
        badgesEnabled={true}
        {...handlers}
      />,
    );

    const initial = screen
      .getByRole("button", { name: "Gmail" })
      .querySelector<HTMLElement>(".service-icon__initial");
    expect(initial).not.toBeNull();
    const first = initial?.style.backgroundColor;
    expect(first).toBeTruthy();

    // Re-rendering the exact same service id must produce the exact same
    // colour (deterministic, not e.g. seeded from render order).
    render(
      <ServiceIcon
        service={service({ id: "gmail-personal", name: "Gmail" })}
        cachedIcon={undefined}
        selected={false}
        size={64}
        draggable={false}
        isDropTarget={false}
        status={undefined}
        badgesEnabled={true}
        {...handlers}
      />,
    );
    const buttons = screen.getAllByRole("button", { name: "Gmail" });
    const second = buttons[1];
    expect(second).toBeDefined();
    const secondInitial = second?.querySelector<HTMLElement>(".service-icon__initial");
    expect(secondInitial?.style.backgroundColor).toBe(first);
  });
});

function renderIcon(status: ServiceStatus | undefined, badgesEnabled: boolean) {
  render(
    <ServiceIcon
      service={service()}
      cachedIcon={undefined}
      selected={false}
      size={64}
      draggable={false}
      isDropTarget={false}
      status={status}
      badgesEnabled={badgesEnabled}
      {...handlers}
    />,
  );
}

describe("ServiceIcon's accessible name", () => {
  it("is just the service name when there is no status yet", () => {
    renderIcon(undefined, true);
    expect(screen.getByRole("button", { name: "Gmail" })).toBeInTheDocument();
  });

  it("is just the service name when badges are disabled, even for a status that would otherwise show one", () => {
    renderIcon({ kind: "ok", count: 3 }, false);
    expect(screen.getByRole("button", { name: "Gmail" })).toBeInTheDocument();
  });

  it("is just the service name for ok(0), which Badge itself renders nothing for", () => {
    renderIcon({ kind: "ok", count: 0 }, true);
    expect(screen.getByRole("button", { name: "Gmail" })).toBeInTheDocument();
  });

  it("is just the service name while loading, which Badge itself renders nothing for", () => {
    renderIcon({ kind: "loading" }, true);
    expect(screen.getByRole("button", { name: "Gmail" })).toBeInTheDocument();
  });

  it("appends Badge's own label for a positive unread count", () => {
    renderIcon({ kind: "ok", count: 3 }, true);
    expect(screen.getByRole("button", { name: "Gmail, 3 unread" })).toBeInTheDocument();
  });

  it("appends Badge's own label for stale", () => {
    renderIcon({ kind: "stale" }, true);
    expect(screen.getByRole("button", { name: "Gmail, Not updating" })).toBeInTheDocument();
  });

  it("appends Badge's own label for needsAttention", () => {
    renderIcon({ kind: "needsAttention", reason: "OffOrigin" }, true);
    expect(screen.getByRole("button", { name: "Gmail, Needs attention" })).toBeInTheDocument();
  });
});
