import type { Recipe, RecipeDescription, UnreadResult } from "./types";

const HOST = "www.icloud.com";

// Matches iCloud Mail by exact hostname plus a path boundary on "/mail":
// "/mail" and "/mail/..." match, but "/mailfoo" does not.
export const icloud: Recipe = {
  id: "icloud",
  displayName: "iCloud Mail",
  defaultProfile: "isolated",

  matches(serviceUrl: URL): boolean {
    if (serviceUrl.hostname !== HOST) {
      return false;
    }
    return serviceUrl.pathname === "/mail" || serviceUrl.pathname.startsWith("/mail/");
  },

  describe(): RecipeDescription {
    return {
      strategy: "title",
      reads: 'Document title matches /\\((\\d+)\\)/, e.g. "(3) Inbox"',
    };
  },

  read(): Promise<UnreadResult> {
    // Task 2.8 implements the title strategy and iCloud's signed-out
    // detection here (design.md §2.2.14).
    return Promise.resolve({ count: null });
  },

  watch(): () => void {
    // Task 2.8 implements title observation here.
    return () => {};
  },
};
