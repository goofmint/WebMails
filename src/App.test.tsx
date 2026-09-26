import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import App from "./App";

describe("App", () => {
  afterEach(() => {
    window.location.hash = "";
  });

  it("renders the shell sidebar at the default route", () => {
    render(<App />);
    expect(screen.getByRole("complementary", { name: "Services" })).toBeInTheDocument();
  });

  it("renders the settings screen for the #/settings route", () => {
    window.location.hash = "#/settings";
    const { container } = render(<App />);
    // The real settingsIpc's get_snapshot has resolved or rejected before
    // this assertion runs (Task 1.12's SettingsApp), so the exact content
    // isn't asserted here (see SettingsApp.test.tsx) — only that this route
    // no longer renders nothing, unlike before Task 1.12.
    expect(container).not.toBeEmptyDOMElement();
  });
});
