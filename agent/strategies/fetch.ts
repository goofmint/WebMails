// Same-origin, read-only fetch strategy (design.md §2.2.14, §6.2, §7.3).
// `createSameOriginFetch` builds the `SameOriginFetch` used in
// `RecipeContext.fetch`; `fetchText` is the shared helper recipes call to
// read a same-origin response as text.
import type { FetchTextResult, RecipeContext, SameOriginFetch } from "../recipes/types";

const REJECT_PREFIX = "[SameOriginFetch] rejected";

// True only for a URL that shares `serviceUrl`'s exact origin *and* scheme.
// The plain `origin` getter is not enough on its own: some non-http(s)
// schemes (e.g. `blob:`) embed a same-origin-looking URL in their path and
// `URL#origin` reports that inner origin, not an opaque one. Requiring the
// protocol to match `serviceUrl`'s (always `http:`/`https:`) closes that
// hole; `data:`/`javascript:` already fail on `origin` alone (`"null"`).
function isSameOrigin(target: URL, serviceUrl: URL): boolean {
  return target.protocol === serviceUrl.protocol && target.origin === serviceUrl.origin;
}

function resolve(path: string, serviceUrl: URL): URL {
  try {
    return new URL(path, serviceUrl);
  } catch {
    throw new Error(
      `${REJECT_PREFIX} unresolvable request "${path}" (base origin "${serviceUrl.origin}")`,
    );
  }
}

function rejectCrossOrigin(target: URL, serviceUrl: URL): never {
  throw new Error(
    `${REJECT_PREFIX} cross-origin request to origin "${target.origin}" (expected "${serviceUrl.origin}")`,
  );
}

function rejectNonGet(method: string): never {
  throw new Error(`${REJECT_PREFIX} non-GET request (method "${method}")`);
}

function rejectBody(): never {
  throw new Error(`${REJECT_PREFIX} request with a body (agent fetches are read-only)`);
}

// Builds a `SameOriginFetch` scoped to `serviceUrl`. `baseFetch` defaults to
// `globalThis.fetch`, bound at creation time (not at call time), so the
// returned function keeps working even if `globalThis.fetch` is later
// reassigned or called unbound.
export function createSameOriginFetch(
  serviceUrl: URL,
  baseFetch: typeof fetch = globalThis.fetch.bind(globalThis),
): SameOriginFetch {
  return async (path: string, init?: RequestInit): Promise<Response> => {
    // Read-only, enforced before anything else (design.md §6.2/§7.3: the
    // agent never mutates, clicks or submits). A page's own scripts can't
    // reach this closure at all, so this isn't a defense against them — it
    // keeps a recipe from accidentally sending a mutating request. `GET` is
    // the only allowed method (case-insensitive; an omitted method defaults
    // to `GET` per the Fetch spec and is allowed), and a `body` is rejected
    // outright since a `GET` request can't carry one meaningfully.
    // Snapshot `init` once: every check below and the request itself use
    // this copy, so an accessor on the caller's object can't return one
    // value to the validation and another to `baseFetch`.
    const snapshot: RequestInit = { ...init };
    const method = snapshot.method;
    if (method !== undefined && method.toUpperCase() !== "GET") {
      rejectNonGet(method);
    }
    if (snapshot.body !== undefined && snapshot.body !== null) {
      rejectBody();
    }

    const target = resolve(path, serviceUrl);
    if (!isSameOrigin(target, serviceUrl)) {
      rejectCrossOrigin(target, serviceUrl);
    }

    // Let the browser follow redirects (the default `redirect: "follow"`):
    // a same-origin login redirect must still be readable as text so a
    // recipe can detect the signed-out state, which `redirect: "manual"`
    // would prevent (it yields an opaque, unreadable response). Instead,
    // once the request settles, check the *final* `response.url` and
    // reject if it left `serviceUrl`'s origin — this is what stops a
    // cross-origin redirect from silently handing a recipe a foreign
    // response. `response.redirected` is passed through unchanged so a
    // same-origin redirect (e.g. Gmail's own login page) stays visible to
    // the caller.
    const response = await baseFetch(target.toString(), {
      ...snapshot,
      method: "GET",
      mode: "same-origin",
      credentials: "same-origin",
    });

    if (response.url !== "") {
      const finalUrl = resolve(response.url, serviceUrl);
      if (!isSameOrigin(finalUrl, serviceUrl)) {
        rejectCrossOrigin(finalUrl, serviceUrl);
      }
    }

    return response;
  };
}

// Shared strategy helper (design.md §2.2.14): calls `ctx.fetch(path)` and
// reads the body as text. Non-200 responses are returned as-is; a
// cross-origin rejection from `ctx.fetch` propagates. No Atom parsing, feed
// path derivation, or fallback happens here — that is each recipe's job.
export async function fetchText(ctx: RecipeContext, path: string): Promise<FetchTextResult> {
  const response = await ctx.fetch(path);
  const body = await response.text();
  return {
    status: response.status,
    redirected: response.redirected,
    url: response.url,
    body,
  };
}
