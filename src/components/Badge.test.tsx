import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Badge, BADGE_COUNT_CAP } from "./Badge";
import type { ServiceStatus } from "../ipc";

function renderBadge(status: ServiceStatus, enabled = true) {
  return render(<Badge status={status} enabled={enabled} />);
}

describe("Badge", () => {
  it("renders nothing while loading", () => {
    const { container } = renderBadge({ kind: "loading" });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing for ok(0)", () => {
    const { container } = renderBadge({ kind: "ok", count: 0 });
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a numeric pill for ok(1)", () => {
    renderBadge({ kind: "ok", count: 1 });
    const pill = screen.getByLabelText("1 unread");
    expect(pill).toHaveTextContent("1");
  });

  it(`renders the exact count at the cap (ok(${BADGE_COUNT_CAP}))`, () => {
    renderBadge({ kind: "ok", count: BADGE_COUNT_CAP });
    const pill = screen.getByLabelText(`${BADGE_COUNT_CAP} unread`);
    expect(pill).toHaveTextContent(String(BADGE_COUNT_CAP));
  });

  it("caps ok(1000) at '999+', one past the cap", () => {
    renderBadge({ kind: "ok", count: BADGE_COUNT_CAP + 1 });
    const pill = screen.getByLabelText(`${BADGE_COUNT_CAP + 1} unread`);
    expect(pill).toHaveTextContent(`${BADGE_COUNT_CAP}+`);
  });

  it("renders a hollow grey dot for stale, with no number", () => {
    renderBadge({ kind: "stale" });
    const dot = screen.getByLabelText("Not updating");
    expect(dot).toHaveTextContent("");
  });

  it.each(["ReportedNone", "OffOrigin", "CreateFailed"])(
    "renders a warning glyph for needsAttention (reason: %s)",
    (reason) => {
      renderBadge({ kind: "needsAttention", reason });
      expect(screen.getByLabelText("Needs attention")).toHaveTextContent("⚠");
    },
  );

  it("renders a warning glyph for needsAttention even with no reason", () => {
    renderBadge({ kind: "needsAttention" });
    expect(screen.getByLabelText("Needs attention")).toHaveTextContent("⚠");
  });

  it("renders nothing for every status when disabled", () => {
    const statuses: readonly ServiceStatus[] = [
      { kind: "loading" },
      { kind: "ok", count: 0 },
      { kind: "ok", count: 5 },
      { kind: "ok", count: BADGE_COUNT_CAP + 1 },
      { kind: "stale" },
      { kind: "needsAttention", reason: "ReportedNone" },
    ];

    for (const status of statuses) {
      const { container } = renderBadge(status, false);
      expect(container).toBeEmptyDOMElement();
    }
  });
});
