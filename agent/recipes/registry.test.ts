import { describe, expect, it } from "vitest";
import { matchRecipe, recipes } from "./registry";
import { generic } from "./generic";

describe("recipe registry", () => {
  it("orders recipes gmail, icloud, outlook, generic", () => {
    expect(recipes.map((recipe) => recipe.id)).toEqual(["gmail", "icloud", "outlook", "generic"]);
  });

  it("has a unique id for every recipe", () => {
    const ids = recipes.map((recipe) => recipe.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("places generic last, and it always matches", () => {
    const last = recipes[recipes.length - 1];
    expect(last?.id).toBe("generic");
    expect(last?.matches(new URL("https://anything.example/"))).toBe(true);
  });

  it("generic itself always matches, regardless of the URL", () => {
    expect(generic.matches(new URL("https://example.com/"))).toBe(true);
    expect(generic.matches(new URL("https://mail.google.com/mail/u/0/"))).toBe(true);
  });

  describe("gmail", () => {
    it("matches mail.google.com and reports the default profile", () => {
      const recipe = matchRecipe(new URL("https://mail.google.com/mail/u/0/#inbox"));
      expect(recipe.id).toBe("gmail");
      expect(recipe.defaultProfile).toBe("default");
    });

    it("does not match a look-alike host with mail.google.com as a suffix of a longer domain", () => {
      const recipe = matchRecipe(new URL("https://evil-mail.google.com.example/"));
      expect(recipe.id).toBe("generic");
    });

    it("does not match a host that merely contains mail.google.com as a substring", () => {
      const recipe = matchRecipe(new URL("https://notmail.google.com.evil.example/"));
      expect(recipe.id).toBe("generic");
    });
  });

  describe("icloud", () => {
    it("matches www.icloud.com/mail exactly and reports the default profile", () => {
      const recipe = matchRecipe(new URL("https://www.icloud.com/mail"));
      expect(recipe.id).toBe("icloud");
      expect(recipe.defaultProfile).toBe("isolated");
    });

    it("matches nested paths under /mail/", () => {
      const recipe = matchRecipe(new URL("https://www.icloud.com/mail/inbox"));
      expect(recipe.id).toBe("icloud");
    });

    it("does not match /mailfoo, which only shares the /mail prefix", () => {
      const recipe = matchRecipe(new URL("https://www.icloud.com/mailfoo"));
      expect(recipe.id).toBe("generic");
    });

    it("does not match a look-alike host", () => {
      const recipe = matchRecipe(new URL("https://www.icloud.com.evil.example/mail"));
      expect(recipe.id).toBe("generic");
    });

    it("does not match icloud on the wrong path", () => {
      const recipe = matchRecipe(new URL("https://www.icloud.com/notes"));
      expect(recipe.id).toBe("generic");
    });
  });

  describe("outlook", () => {
    it("matches outlook.live.com and reports the default profile", () => {
      const recipe = matchRecipe(new URL("https://outlook.live.com/mail/0/inbox"));
      expect(recipe.id).toBe("outlook");
      expect(recipe.defaultProfile).toBe("isolated");
    });

    it("does not match a look-alike host", () => {
      const recipe = matchRecipe(new URL("https://outlook.live.com.evil.example/mail/0/inbox"));
      expect(recipe.id).toBe("generic");
    });
  });

  describe("fallback", () => {
    it("falls back to generic for an unrelated host", () => {
      const recipe = matchRecipe(new URL("https://example.com/"));
      expect(recipe.id).toBe("generic");
      expect(recipe.defaultProfile).toBe("isolated");
    });
  });
});
