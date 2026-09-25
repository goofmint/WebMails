import type { Recipe, RecipeDescription, UnreadResult } from "./types";

const HOST = "mail.google.com";

// Matches Gmail exactly by hostname. Uses exact comparison (not
// endsWith/includes) so a look-alike host such as
// "evil-mail.google.com.example" does not match.
export const gmail: Recipe = {
  id: "gmail",
  displayName: "Gmail",
  defaultProfile: "default",

  matches(serviceUrl: URL): boolean {
    return serviceUrl.hostname === HOST;
  },

  describe(): RecipeDescription {
    return {
      strategy: "fetch",
      reads: "GET /mail/u/<segment>/feed/atom",
    };
  },

  read(): Promise<UnreadResult> {
    // Task 2.7 implements the fetch strategy: GET the Atom feed for the
    // service URL's <segment>, parse <fullcount>, and fall back to the
    // title strategy when validation fails (design.md §2.2.14).
    return Promise.resolve({ count: null });
  },

  watch(): () => void {
    // Task 2.7 implements watchTitle-driven refetching here.
    return () => {};
  },
};
