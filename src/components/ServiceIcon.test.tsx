import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ServiceIcon } from "./ServiceIcon";
import { service } from "../test/fixtures";
import type { ServiceStatus } from "../ipc";

function noop(): void {}

function renderIcon(status: ServiceStatus | undefined, badgesEnabled: boolean) {
  render(
    <ServiceIcon
      service={service()}
      selected={false}
      size={64}
      draggable={false}
      isDropTarget={false}
      status={status}
      badgesEnabled={badgesEnabled}
      onSelect={noop}
      onDragStart={noop}
      onDragOverTarget={noop}
      onDrop={noop}
      onDragEnd={noop}
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
