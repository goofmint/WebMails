import type { Recipe, RecipeContext, RecipeDescription, UnreadResult } from "./types";
import { titleCount, watchTitle } from "../strategies/title";

// Not anchored, so it matches the count anywhere in the title, e.g.
// "(3) Inbox - Example" or "Example Mail (3)".
const TITLE_COUNT_PATTERN = /\((\d+)\)/;

// Fallback recipe: matches any service URL, so it must stay last in the
// registry (see registry.ts). Reads the unread count from the document
// title's "(<count>)" prefix, e.g. "(3) Inbox - Example".
export const generic: Recipe = {
  id: "generic",
  displayName: "Generic (title count)",
  defaultProfile: "isolated",

  matches(): boolean {
    return true;
  },

  describe(): RecipeDescription {
    return {
      strategy: "title",
      reads: `Document title matches ${TITLE_COUNT_PATTERN.toString()}, e.g. "(3) Inbox"`,
    };
  },

  read(ctx: RecipeContext): Promise<UnreadResult> {
    // The title strategy cannot tell an empty inbox apart from a
    // same-origin login page (design.md §2.2.14's count=null rule), so a
    // title with no count reads as 0, not null.
    const count = titleCount(ctx.document, TITLE_COUNT_PATTERN);
    return Promise.resolve({ count: count ?? 0, messages: [] });
  },

  watch(ctx: RecipeContext, onChange: () => void): () => void {
    return watchTitle(ctx.document, onChange);
  },
};
