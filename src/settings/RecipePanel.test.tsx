import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { matchRecipe } from "../../agent/recipes/registry";
import { service } from "../test/fixtures";
import { RecipePanel } from "./RecipePanel";

const gmailService = service({
  id: "svc-gmail",
  name: "Personal Gmail",
  url: "https://mail.google.com/mail/u/0/",
});
const icloudService = service({
  id: "svc-icloud",
  name: "iCloud",
  url: "https://www.icloud.com/mail",
});
const outlookService = service({
  id: "svc-outlook",
  name: "Work Outlook",
  url: "https://outlook.live.com/mail/0/inbox",
});
const genericService = service({
  id: "svc-generic",
  name: "Fastmail",
  url: "https://fastmail.example.com/",
});
const brokenService = service({
  id: "svc-broken",
  name: "Broken Service",
  url: "not a url",
});

function itemFor(name: string): HTMLElement {
  const heading = screen.getByRole("heading", { level: 3, name });
  const item = heading.closest("li");
  if (item === null) {
    throw new Error(`expected an <li> ancestor for heading "${name}"`);
  }
  return item;
}

describe("RecipePanel", () => {
  it("shows a short message for an empty service list", () => {
    render(<RecipePanel services={[]} />);
    expect(screen.getByText("No services to show a recipe for.")).toBeInTheDocument();
    expect(screen.queryByRole("listitem")).not.toBeInTheDocument();
  });

  it("shows Gmail's recipe name, strategy, and reads", () => {
    render(<RecipePanel services={[gmailService]} />);
    const item = within(itemFor("Personal Gmail"));

    const expected = matchRecipe(new URL(gmailService.url)).describe(new URL(gmailService.url));

    expect(item.getByText("Gmail")).toBeInTheDocument();
    expect(item.getByText("fetch")).toBeInTheDocument();
    expect(item.getByText(expected.reads)).toBeInTheDocument();
  });

  it("shows iCloud Mail's recipe name and title strategy", () => {
    render(<RecipePanel services={[icloudService]} />);
    const item = within(itemFor("iCloud"));

    expect(item.getByText("iCloud Mail")).toBeInTheDocument();
    expect(item.getByText("title")).toBeInTheDocument();
  });

  it("shows Outlook's recipe name and title strategy", () => {
    render(<RecipePanel services={[outlookService]} />);
    const item = within(itemFor("Work Outlook"));

    expect(item.getByText("Outlook")).toBeInTheDocument();
    expect(item.getByText("title")).toBeInTheDocument();
  });

  it("shows the generic recipe for a URL matching no specific recipe", () => {
    render(<RecipePanel services={[genericService]} />);
    const item = within(itemFor("Fastmail"));

    expect(item.getByText("Generic (title count)")).toBeInTheDocument();
    expect(item.getByText("title")).toBeInTheDocument();
  });

  it("shows an error row, not a recipe, for a service with an unparseable URL", () => {
    render(<RecipePanel services={[brokenService]} />);
    const item = within(itemFor("Broken Service"));

    expect(item.getByRole("alert")).toHaveTextContent("not a url");
    expect(item.queryByText("Recipe")).not.toBeInTheDocument();
  });

  it("renders one row per service, in the given order", () => {
    render(<RecipePanel services={[gmailService, icloudService, brokenService]} />);
    const headings = screen.getAllByRole("heading", { level: 3 });
    expect(headings.map((h) => h.textContent)).toEqual([
      "Personal Gmail",
      "iCloud",
      "Broken Service",
    ]);
  });
});
