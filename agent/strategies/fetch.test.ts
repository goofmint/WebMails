import { describe, expect, it, vi } from "vitest";
import type { RecipeContext } from "../recipes/types";
import { createSameOriginFetch, fetchText } from "./fetch";

const SERVICE_URL = new URL("https://mail.google.com/mail/u/0/");

// A minimal same-shaped stand-in for the parts of `Response` this module
// reads (`status`, `redirected`, `url`, `text()`). Cast to `Response` at the
// call site instead of widening this helper's return type, so every caller
// still gets full `Response` typing.
function makeResponse(overrides: {
  status?: number;
  redirected?: boolean;
  url?: string;
  body?: string;
}): Response {
  return {
    status: overrides.status ?? 200,
    redirected: overrides.redirected ?? false,
    url: overrides.url ?? "",
    text: () => Promise.resolve(overrides.body ?? ""),
  } as Response;
}

function makeBaseFetch(response: Response): ReturnType<typeof vi.fn<typeof fetch>> {
  return vi.fn<typeof fetch>().mockResolvedValue(response);
}

describe("createSameOriginFetch", () => {
  const crossOriginCases: ReadonlyArray<readonly [string, string]> = [
    ["absolute URL on another host", "https://evil.example/x"],
    ["protocol-relative URL", "//evil.example/x"],
    ["backslash authority (treated as protocol-relative on special schemes)", "\\\\evil.example/x"],
    ["userinfo used to disguise the real host", "https://mail.google.com@evil.example/"],
    ["look-alike host (subdomain-looking suffix)", "https://mail.google.com.evil.example/"],
    ["different subdomain", "https://evil.mail.google.com/"],
    ["scheme downgrade to http", "http://mail.google.com/"],
    ["different port", "https://mail.google.com:8443/"],
    ["javascript: URL", "javascript:alert(1)"],
    ["data: URL", "data:text/html,hi"],
    ["blob: URL wrapping a same-origin-looking path", "blob:https://mail.google.com/00000000-0000"],
  ];

  it.each(crossOriginCases)(
    "rejects %s without calling the underlying fetch",
    async (_label, url) => {
      const baseFetch = makeBaseFetch(makeResponse({}));
      const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

      await expect(sameOriginFetch(url)).rejects.toThrow();
      expect(baseFetch).not.toHaveBeenCalled();
    },
  );

  const nonGetMethodCases: ReadonlyArray<readonly [string, string]> = [
    ["POST", "POST"],
    ["PUT", "PUT"],
    ["DELETE", "DELETE"],
    ["lowercase post (case-insensitive)", "post"],
  ];

  it.each(nonGetMethodCases)(
    "rejects a %s request without calling the underlying fetch",
    async (_label, method) => {
      const baseFetch = makeBaseFetch(makeResponse({}));
      const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

      await expect(sameOriginFetch("/feed/atom", { method })).rejects.toThrow();
      expect(baseFetch).not.toHaveBeenCalled();
    },
  );

  it("rejects a request with a body without calling the underlying fetch", async () => {
    const baseFetch = makeBaseFetch(makeResponse({}));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await expect(sameOriginFetch("/feed/atom", { body: "x=1" })).rejects.toThrow();
    expect(baseFetch).not.toHaveBeenCalled();
  });

  it("allows an explicit GET request (case-insensitive)", async () => {
    const baseFetch = makeBaseFetch(makeResponse({ url: "https://mail.google.com/feed/atom" }));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("/feed/atom", { method: "get" });

    expect(baseFetch).toHaveBeenCalledTimes(1);
  });

  it("allows a request with an omitted method (defaults to GET)", async () => {
    const baseFetch = makeBaseFetch(makeResponse({ url: "https://mail.google.com/feed/atom" }));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("/feed/atom");

    expect(baseFetch).toHaveBeenCalledTimes(1);
  });

  it("rejects an unresolvable path without calling the underlying fetch", async () => {
    const baseFetch = makeBaseFetch(makeResponse({}));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await expect(sameOriginFetch("http://[::1")).rejects.toThrow();
    expect(baseFetch).not.toHaveBeenCalled();
  });

  it("allows a root-relative path, resolving it against serviceUrl", async () => {
    const baseFetch = makeBaseFetch(makeResponse({ url: "https://mail.google.com/feed/atom" }));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("/feed/atom");

    expect(baseFetch).toHaveBeenCalledTimes(1);
    expect(baseFetch.mock.calls[0]?.[0]).toBe("https://mail.google.com/feed/atom");
  });

  it("allows a slash-less relative path, resolving it against serviceUrl", async () => {
    const baseFetch = makeBaseFetch(
      makeResponse({ url: "https://mail.google.com/mail/u/0/feed/atom" }),
    );
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("feed/atom");

    expect(baseFetch.mock.calls[0]?.[0]).toBe("https://mail.google.com/mail/u/0/feed/atom");
  });

  it("allows a same-origin absolute URL and passes the resolved absolute URL through", async () => {
    const baseFetch = makeBaseFetch(
      makeResponse({ url: "https://mail.google.com/mail/u/0/feed/atom" }),
    );
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("https://mail.google.com/mail/u/0/feed/atom");

    expect(baseFetch.mock.calls[0]?.[0]).toBe("https://mail.google.com/mail/u/0/feed/atom");
  });

  it("forces mode and credentials to same-origin while preserving other init fields", async () => {
    const baseFetch = makeBaseFetch(makeResponse({ url: "https://mail.google.com/feed/atom" }));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await sameOriginFetch("/feed/atom", {
      mode: "cors",
      credentials: "omit",
      headers: { Accept: "application/atom+xml" },
      cache: "no-store",
    });

    const init = baseFetch.mock.calls[0]?.[1];
    expect(init).toMatchObject({
      mode: "same-origin",
      credentials: "same-origin",
      headers: { Accept: "application/atom+xml" },
      cache: "no-store",
    });
  });

  it("sends GET even if a method getter changes after validation", async () => {
    const baseFetch = makeBaseFetch(makeResponse({ url: "https://mail.google.com/feed/atom" }));
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);
    let reads = 0;
    const init: RequestInit = {
      get method() {
        reads += 1;
        return reads === 1 ? "GET" : "POST";
      },
    };

    await sameOriginFetch("/feed/atom", init);

    expect(baseFetch.mock.calls[0]?.[1]).toMatchObject({ method: "GET" });
  });

  it("rejects when the final response.url lands on another origin (cross-origin redirect)", async () => {
    const baseFetch = makeBaseFetch(
      makeResponse({ status: 200, redirected: true, url: "https://accounts.google.com/login" }),
    );
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    await expect(sameOriginFetch("/feed/atom")).rejects.toThrow();
    expect(baseFetch).toHaveBeenCalledTimes(1);
  });

  it("allows a same-origin redirect, exposing response.redirected", async () => {
    const response = makeResponse({
      status: 200,
      redirected: true,
      url: "https://mail.google.com/mail/u/0/login",
    });
    const baseFetch = makeBaseFetch(response);
    const sameOriginFetch = createSameOriginFetch(SERVICE_URL, baseFetch);

    const result = await sameOriginFetch("/feed/atom");

    expect(result).toBe(response);
    expect(result.redirected).toBe(true);
  });

  it("captures globalThis.fetch bound at creation time when no baseFetch is given", async () => {
    const response = makeResponse({ url: "https://mail.google.com/feed/atom" });
    const stub = vi.fn<typeof fetch>().mockResolvedValue(response);
    const originalFetch = globalThis.fetch;
    globalThis.fetch = stub;
    try {
      const sameOriginFetch = createSameOriginFetch(SERVICE_URL);
      await sameOriginFetch("/feed/atom");
      expect(stub).toHaveBeenCalledTimes(1);
    } finally {
      globalThis.fetch = originalFetch;
    }
  });
});

describe("fetchText", () => {
  function makeContext(baseFetch: ReturnType<typeof vi.fn<typeof fetch>>): RecipeContext {
    const document = new DOMParser().parseFromString(
      "<html><head></head><body></body></html>",
      "text/html",
    );
    return {
      serviceUrl: SERVICE_URL,
      document,
      fetch: createSameOriginFetch(SERVICE_URL, baseFetch),
    };
  }

  it("returns status, redirected, url and body read via text()", async () => {
    const baseFetch = makeBaseFetch(
      makeResponse({
        status: 200,
        redirected: false,
        url: "https://mail.google.com/mail/u/0/feed/atom",
        body: "<feed></feed>",
      }),
    );
    const ctx = makeContext(baseFetch);

    const result = await fetchText(ctx, "/feed/atom");

    expect(result).toEqual({
      status: 200,
      redirected: false,
      url: "https://mail.google.com/mail/u/0/feed/atom",
      body: "<feed></feed>",
    });
  });

  it("resolves (does not reject) a non-200 same-origin response", async () => {
    const baseFetch = makeBaseFetch(
      makeResponse({
        status: 404,
        url: "https://mail.google.com/mail/u/0/feed/atom",
        body: "not found",
      }),
    );
    const ctx = makeContext(baseFetch);

    const result = await fetchText(ctx, "/feed/atom");

    expect(result.status).toBe(404);
    expect(result.body).toBe("not found");
  });

  it("propagates a cross-origin rejection and never calls the underlying fetch", async () => {
    const baseFetch = makeBaseFetch(makeResponse({}));
    const ctx = makeContext(baseFetch);

    await expect(fetchText(ctx, "https://evil.example/feed/atom")).rejects.toThrow();
    expect(baseFetch).not.toHaveBeenCalled();
  });
});
