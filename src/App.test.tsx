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

  it("renders nothing for the #/settings route", () => {
    window.location.hash = "#/settings";
    const { container } = render(<App />);
    expect(container).toBeEmptyDOMElement();
  });
});
