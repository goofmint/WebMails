import { describe, expect, it } from "vitest";
import { matchRecipe } from "../../agent/recipes/registry";
import { buildRecipeRows } from "./recipeRows";

describe("buildRecipeRows", () => {
  it("returns an empty list for no services", () => {
    expect(buildRecipeRows([])).toEqual([]);
  });

  it("builds an ok row for a Gmail service, using the registry's own description", () => {
    const url = "https://mail.google.com/mail/u/0/";
    const [row] = buildRecipeRows([{ id: "svc-gmail", name: "Personal Gmail", url }]);
    const expected = matchRecipe(new URL(url)).describe(new URL(url));

    expect(row).toEqual({
      id: "svc-gmail",
      name: "Personal Gmail",
      kind: "ok",
      displayName: "Gmail",
      strategy: expected.strategy,
      reads: expected.reads,
    });
  });

  it("builds an ok row for an iCloud Mail service", () => {
    const url = "https://www.icloud.com/mail";
    const [row] = buildRecipeRows([{ id: "svc-icloud", name: "iCloud", url }]);

    expect(row).toMatchObject({ kind: "ok", displayName: "iCloud Mail", strategy: "title" });
  });

  it("builds an ok row for an Outlook service", () => {
    const url = "https://outlook.live.com/mail/0/inbox";
    const [row] = buildRecipeRows([{ id: "svc-outlook", name: "Outlook", url }]);

    expect(row).toMatchObject({ kind: "ok", displayName: "Outlook", strategy: "title" });
  });

  it("builds an ok row for a URL matching no specific recipe (generic)", () => {
    const url = "https://fastmail.example.com/";
    const [row] = buildRecipeRows([{ id: "svc-generic", name: "Fastmail", url }]);

    expect(row).toMatchObject({
      kind: "ok",
      displayName: "Generic (title count)",
      strategy: "title",
    });
  });

  it("builds an error row, not a default recipe row, for an unparseable URL", () => {
    const [row] = buildRecipeRows([{ id: "svc-bad", name: "Broken", url: "not a url" }]);

    expect(row).toEqual({
      id: "svc-bad",
      name: "Broken",
      kind: "error",
      message: 'Could not parse this service\'s URL: "not a url".',
    });
  });

  it("preserves config order and keys rows by id", () => {
    const rows = buildRecipeRows([
      { id: "a", name: "A", url: "https://mail.google.com/" },
      { id: "b", name: "B", url: "not a url" },
      { id: "c", name: "C", url: "https://outlook.live.com/mail/0/inbox" },
    ]);

    expect(rows.map((r) => r.id)).toEqual(["a", "b", "c"]);
    expect(rows[1]?.kind).toBe("error");
  });
});
