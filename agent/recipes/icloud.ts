import type { Recipe, RecipeContext, RecipeDescription, UnreadResult } from "./types";
import { titleCount, watchTitle } from "../strategies/title";

const HOST = "www.icloud.com";

// iCloud Mail puts the unread count in parentheses somewhere in the
// document title. This is the pattern `describe()` already advertised.
// There is no separate signed-out marker here: a title with no count is
// read as an empty inbox (design.md §2.2.14). A login page on another
// origin is Rust's `OffOrigin`, not this recipe's job.
const ICLOUD_TITLE_PATTERN = /\((\d+)\)/;

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
      reads: `Document title matches ${ICLOUD_TITLE_PATTERN.toString()}, e.g. "(3) Inbox"`,
    };
  },

  read(ctx: RecipeContext): Promise<UnreadResult> {
    const count = titleCount(ctx.document, ICLOUD_TITLE_PATTERN);
    return Promise.resolve({ count: count ?? 0, messages: [] });
  },

  watch(ctx: RecipeContext, onChange: () => void): () => void {
    return watchTitle(ctx.document, onChange);
  },
};
