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

// Fetch scoped to the service's own origin, and read-only (see
// strategies/fetch.ts for the implementation, `createSameOriginFetch`).
// `path` is resolved against `RecipeContext.serviceUrl`; the returned
// promise rejects, without calling the underlying fetch, when: resolution
// fails; the resolved URL's origin differs from `serviceUrl`'s (including
// protocol-relative and non-http(s) URLs such as `data:`/`blob:`/
// `javascript:`); `init.method` is present and not `GET` (case-insensitive;
// an omitted method is allowed and defaults to `GET`); or `init.body` is
// present (design.md §6.2/§7.3: the agent never mutates, clicks or
// submits). It also rejects when the response is ultimately served from a
// different origin (e.g. a cross-origin login redirect). `init.mode` and
// `init.credentials` are always forced to `"same-origin"`; other `init`
// fields are passed through unchanged.
export type SameOriginFetch = (path: string, init?: RequestInit) => Promise<Response>;

// Result of `fetchText` (strategies/fetch.ts): the response's status and
// final URL, whether a redirect occurred, and the body read via `text()`.
// Non-200 responses are returned, not rejected; only cross-origin targets
// reject (see `SameOriginFetch` above).
export interface FetchTextResult {
  readonly status: number;
  readonly redirected: boolean;
  readonly url: string;
  readonly body: string;
}

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
