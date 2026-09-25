// Shared recipe types for the agent. Mirrors design.md §2.2.14 exactly:
// type names and field names must not change, and no extra fields should be
// added here without updating the design doc first.

export type ProfileDefault = "default" | "isolated";

export type UnreadResult =
  { readonly count: number; readonly messages: readonly MessageRef[] } | { readonly count: null };

export interface MessageRef {
  readonly id: string;
  readonly from: string | null;
  readonly subject: string | null;
  readonly link: string | null;
}

// Fetch scoped to the service's own origin; rejects any URL that is not on
// serviceUrl's origin. The real implementation is added in Task 2.6.
export type SameOriginFetch = (path: string, init?: RequestInit) => Promise<Response>;

export interface RecipeContext {
  readonly serviceUrl: URL;
  readonly document: Document;
  readonly fetch: SameOriginFetch; // rejects any URL not on serviceUrl's origin
}

export interface RecipeDescription {
  readonly strategy: "title" | "fetch" | "selector";
  readonly reads: string; // human-readable, e.g. "GET /mail/u/<addr>/feed/atom"
}

export interface Recipe {
  readonly id: string;
  readonly displayName: string;
  readonly defaultProfile: ProfileDefault;
  matches(serviceUrl: URL): boolean;
  describe(serviceUrl: URL): RecipeDescription;
  read(ctx: RecipeContext): Promise<UnreadResult>;
  watch(ctx: RecipeContext, onChange: () => void): () => void; // returns unsubscribe
}
