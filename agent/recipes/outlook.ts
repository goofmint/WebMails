import type { Recipe, RecipeDescription, UnreadResult } from "./types";

const HOST = "outlook.live.com";

// Matches Outlook.com exactly by hostname (no path restriction, unlike
// icloud.ts).
export const outlook: Recipe = {
  id: "outlook",
  displayName: "Outlook",
  defaultProfile: "isolated",

  matches(serviceUrl: URL): boolean {
    return serviceUrl.hostname === HOST;
  },

  describe(): RecipeDescription {
    return {
      strategy: "title",
      reads: 'Document title matches /\\((\\d+)\\)/, e.g. "(3) Inbox"',
    };
  },

  read(): Promise<UnreadResult> {
    // Task 2.8 implements the title strategy and Outlook's signed-out
    // detection here (design.md §2.2.14).
    return Promise.resolve({ count: null });
  },

  watch(): () => void {
    // Task 2.8 implements title observation here.
    return () => {};
  },
};
