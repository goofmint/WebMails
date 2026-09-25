import type { Recipe, RecipeDescription, UnreadResult } from "./types";

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
      reads: 'Document title matches /\\((\\d+)\\)/, e.g. "(3) Inbox"',
    };
  },

  read(): Promise<UnreadResult> {
    // Task 2.5 implements the title strategy (matching `\((\d+)\)` in
    // `document.title`) here.
    return Promise.resolve({ count: null });
  },

  watch(): () => void {
    // Task 2.5 implements title observation (a MutationObserver on <head>)
    // here.
    return () => {};
  },
};
