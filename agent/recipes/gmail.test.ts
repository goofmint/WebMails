import { describe, expect, it } from "vitest";
import { createGmailRecipe, gmailFeedPath, parseGmailAtom } from "./gmail";
import type { RecipeContext } from "./types";
import feedValid from "./__fixtures__/gmail/feed-valid.xml?raw";
import feedEmpty from "./__fixtures__/gmail/feed-empty.xml?raw";
import feedMalformed from "./__fixtures__/gmail/feed-malformed.xml?raw";
import loginRedirectHtml from "./__fixtures__/gmail/login-redirect.html?raw";

const SERVICE_URL = new URL("https://mail.google.com/mail/u/0/");

function makeDocument(title: string): Document {
  const doc = new DOMParser().parseFromString(
    `<html><head><title>${title}</title></head><body></body></html>`,
    "text/html",
  );
  return doc;
}

// Builds a `Response`-shaped stub. `redirected` and `url` are real
// `Response` getters with no setter, so overriding them for a test needs
// `Object.defineProperty` rather than plain assignment.
function makeFetchResponse(overrides: {
  status?: number;
  redirected?: boolean;
  url?: string;
  body: string;
}): Response {
  const response = new Response(overrides.body, { status: overrides.status ?? 200 });
  Object.defineProperty(response, "redirected", {
    value: overrides.redirected ?? false,
    configurable: true,
  });
  Object.defineProperty(response, "url", {
    value: overrides.url ?? "https://mail.google.com/mail/u/0/feed/atom",
    configurable: true,
  });
  return response;
}

function makeContext(options: {
  title: string;
  fetchImpl: (path: string) => Promise<Response>;
}): RecipeContext {
  return {
    serviceUrl: SERVICE_URL,
    document: makeDocument(options.title),
    fetch: options.fetchImpl,
  };
}

describe("gmailFeedPath", () => {
  it("uses the numeric account segment from the service URL", () => {
    expect(gmailFeedPath(new URL("https://mail.google.com/mail/u/2/#inbox"))).toBe(
      "/mail/u/2/feed/atom",
    );
  });

  it("uses an email-address-shaped segment from the service URL", () => {
    expect(
      gmailFeedPath(new URL("https://mail.google.com/mail/u/someone@example.com/#inbox")),
    ).toBe("/mail/u/someone@example.com/feed/atom");
  });

  it("falls back to segment 0 when the service URL has no /mail/u/<segment> segment", () => {
    expect(gmailFeedPath(new URL("https://mail.google.com/"))).toBe("/mail/u/0/feed/atom");
  });
});

describe("parseGmailAtom", () => {
  it("parses a valid feed into fullcount and entries, skipping an entry with no id only if absent", () => {
    const parsed = parseGmailAtom(feedValid);
    expect(parsed).not.toBeNull();
    expect(parsed?.fullcount).toBe(3);
    expect(parsed?.entries).toHaveLength(3);
    expect(parsed?.entries[0]).toMatchObject({
      rawId: "tag:gmail.google.com,2004:1141377934898454727",
      from: "Alice Example",
      subject: "Quarterly report attached",
      messageId: "18f2a3b4c5d6e7f8",
    });
    expect(parsed?.entries[1]).toMatchObject({
      from: "Bob Example",
      subject: "Re: Lunch tomorrow?",
      messageId: "18f2a3b4c5d6e800",
    });
    // Third fixture entry has an empty <title> and no <name>, only <email>,
    // and its <link> carries no message_id query parameter.
    expect(parsed?.entries[2]).toMatchObject({
      from: "carol@example.com",
      subject: null,
      messageId: null,
    });
  });

  it("parses an empty feed as fullcount 0 with no entries", () => {
    const parsed = parseGmailAtom(feedEmpty);
    expect(parsed).toEqual({ fullcount: 0, entries: [] });
  });

  it("returns null for a malformed (unparseable) body", () => {
    expect(parseGmailAtom(feedMalformed)).toBeNull();
  });

  it("returns null when <fullcount> is missing", () => {
    const xml = `<feed xmlns='http://purl.org/atom/ns#'><title>Gmail</title></feed>`;
    expect(parseGmailAtom(xml)).toBeNull();
  });

  it("returns null when <fullcount> is not a non-negative integer", () => {
    const xml = `<feed xmlns='http://purl.org/atom/ns#'><fullcount>-1</fullcount></feed>`;
    expect(parseGmailAtom(xml)).toBeNull();
    const xml2 = `<feed xmlns='http://purl.org/atom/ns#'><fullcount>abc</fullcount></feed>`;
    expect(parseGmailAtom(xml2)).toBeNull();
  });

  it("skips an entry that has no <id>, keeping the others", () => {
    const xml = `<feed xmlns='http://purl.org/atom/ns#'>
      <fullcount>1</fullcount>
      <entry><title>No id here</title></entry>
      <entry><id>tag:gmail.google.com,2004:1</id><title>Has id</title></entry>
    </feed>`;
    const parsed = parseGmailAtom(xml);
    expect(parsed?.entries).toHaveLength(1);
    expect(parsed?.entries[0]?.subject).toBe("Has id");
  });

  it("caps entries at 100 even when the feed has more", () => {
    const entries = Array.from(
      { length: 120 },
      (_, i) => `<entry><id>tag:gmail.google.com,2004:${i}</id></entry>`,
    ).join("");
    const xml = `<feed xmlns='http://purl.org/atom/ns#'><fullcount>120</fullcount>${entries}</feed>`;
    const parsed = parseGmailAtom(xml);
    expect(parsed?.entries).toHaveLength(100);
  });

  it("truncates from and subject to 512 code points without splitting a surrogate pair", () => {
    // U+1F600 is a surrogate pair in UTF-16 (2 code units, 1 code point).
    const longSubject = "\u{1F600}".repeat(600);
    const xml = `<feed xmlns='http://purl.org/atom/ns#'>
      <fullcount>1</fullcount>
      <entry><id>tag:gmail.google.com,2004:1</id><title>${longSubject}</title></entry>
    </feed>`;
    const parsed = parseGmailAtom(xml);
    const subject = parsed?.entries[0]?.subject ?? "";
    expect(Array.from(subject)).toHaveLength(512);
    // No lone surrogate at the boundary.
    expect(subject.codePointAt(subject.length - 2)).toBe(0x1f600);
  });
});

describe("gmail recipe", () => {
  describe("describe()", () => {
    it("reports the fetch strategy and the derived feed path before any read", () => {
      const recipe = createGmailRecipe();
      expect(recipe.describe(SERVICE_URL)).toEqual({
        strategy: "fetch",
        reads: "GET /mail/u/0/feed/atom",
      });
    });

    it("reports the title strategy once validation has failed", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 500, body: "" })),
      });
      await recipe.read(ctx);
      expect(recipe.describe(SERVICE_URL).strategy).toBe("title");
    });
  });

  describe("read() — successful validation", () => {
    it("returns the fullcount and hashed, deep-linked messages from a valid feed", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (3) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 200, body: feedValid })),
      });

      const result = await recipe.read(ctx);

      expect(result.count).toBe(3);
      if (result.count === null) {
        throw new Error("expected a non-null result");
      }
      expect(result.messages).toHaveLength(3);
      const [first, , third] = result.messages;
      expect(first?.id).toMatch(/^[0-9a-f]{16}$/);
      expect(first?.from).toBe("Alice Example");
      expect(first?.subject).toBe("Quarterly report attached");
      expect(first?.link).toBe(`${SERVICE_URL.origin}${SERVICE_URL.pathname}#all/18f2a3b4c5d6e7f8`);
      // No message_id in the fixture's third entry -> no deep link.
      expect(third?.link).toBeNull();
    });

    it("hashes the same entry id to the same MessageRef id deterministically", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (3) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 200, body: feedValid })),
      });

      const first = await recipe.read(ctx);
      const second = await recipe.read(ctx);
      if (first.count === null || second.count === null) {
        throw new Error("expected non-null results");
      }
      expect(first.messages.map((m) => m.id)).toEqual(second.messages.map((m) => m.id));
    });

    it("reports 0 and no messages for an empty feed", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 200, body: feedEmpty })),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: 0, messages: [] });
    });

    it("switches mode to feed after one successful validation (describe reflects it)", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (3) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 200, body: feedValid })),
      });
      await recipe.read(ctx);
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");
    });
  });

  describe("read() — malformed body (sticky fallback on first use)", () => {
    it("falls back to the title count on first use and stays on title afterward", async () => {
      const recipe = createGmailRecipe();
      let fetchCalls = 0;
      const ctx = makeContext({
        title: "Inbox (5) - someone@example.com - Gmail",
        fetchImpl: () => {
          fetchCalls += 1;
          return Promise.resolve(makeFetchResponse({ status: 200, body: feedMalformed }));
        },
      });

      const firstResult = await recipe.read(ctx);
      expect(firstResult).toEqual({ count: 5, messages: [] });
      expect(recipe.describe(SERVICE_URL).strategy).toBe("title");

      // A second read must not fetch again: it's stuck on the title
      // strategy for the rest of this page's life.
      const secondResult = await recipe.read(ctx);
      expect(secondResult).toEqual({ count: 5, messages: [] });
      expect(fetchCalls).toBe(1);
    });

    it("does not stick to title when a malformed body happens after a prior successful validation", async () => {
      const recipe = createGmailRecipe();
      let body = feedValid;
      const ctx = makeContext({
        title: "Inbox (7) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 200, body })),
      });

      const validated = await recipe.read(ctx);
      expect(validated.count).toBe(3);
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");

      body = feedMalformed;
      const transient = await recipe.read(ctx);
      expect(transient).toEqual({ count: 7, messages: [] });
      // Still "fetch": the earlier successful validation keeps this
      // transient failure from sticking the recipe to title.
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");
    });
  });

  describe("read() — non-200 status (sticky fallback on first use)", () => {
    it("falls back to the title count on first use for a 500 and stays sticky", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (2) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 500, body: "" })),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: 2, messages: [] });
      expect(recipe.describe(SERVICE_URL).strategy).toBe("title");
    });
  });

  describe("read() — signed-out (redirect or 401 with no title count)", () => {
    it("returns count: null for a same-origin redirect with no title count", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Gmail",
        fetchImpl: () =>
          Promise.resolve(
            makeFetchResponse({
              status: 200,
              redirected: true,
              url: "https://mail.google.com/mail/u/0/",
              body: loginRedirectHtml,
            }),
          ),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: null });
      // Signed-out detection never sticks the recipe to title: it stays
      // ready to try the feed again once signed back in.
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");
    });

    it("returns count: null for a 401 with no title count", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 401, body: "" })),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: null });
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");
    });

    it("does not treat a redirect as signed-out when the title still has a count", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (4) - someone@example.com - Gmail",
        fetchImpl: () =>
          Promise.resolve(
            makeFetchResponse({
              status: 200,
              redirected: true,
              url: "https://mail.google.com/mail/u/0/",
              body: loginRedirectHtml,
            }),
          ),
      });

      const result = await recipe.read(ctx);
      // Falls back to the (non-null) title count instead of null.
      expect(result).toEqual({ count: 4, messages: [] });
    });

    it("does not treat a 401 as signed-out when the title still has a count", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (6) - someone@example.com - Gmail",
        fetchImpl: () => Promise.resolve(makeFetchResponse({ status: 401, body: "" })),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: 6, messages: [] });
    });
  });

  describe("read() — fetch rejection (cross-origin, etc.)", () => {
    it("falls back to the title count without changing mode when ctx.fetch rejects", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox (1) - someone@example.com - Gmail",
        fetchImpl: () => Promise.reject(new Error("[SameOriginFetch] rejected cross-origin")),
      });

      const result = await recipe.read(ctx);
      expect(result).toEqual({ count: 1, messages: [] });
      // Unvalidated + a plain fetch rejection is not a validation failure:
      // the recipe is still willing to try the feed again next time.
      expect(recipe.describe(SERVICE_URL).strategy).toBe("fetch");
    });
  });

  describe("watch()", () => {
    it("calls onChange when the title changes, and stops after unsubscribe", async () => {
      const recipe = createGmailRecipe();
      const ctx = makeContext({
        title: "Inbox - someone@example.com - Gmail",
        fetchImpl: () => Promise.reject(new Error("unused")),
      });
      let calls = 0;
      const unsubscribe = recipe.watch(ctx, () => {
        calls += 1;
      });

      ctx.document.title = "Inbox (1) - someone@example.com - Gmail";
      await Promise.resolve();
      expect(calls).toBe(1);

      unsubscribe();
      ctx.document.title = "Inbox (2) - someone@example.com - Gmail";
      await Promise.resolve();
      expect(calls).toBe(1);
    });
  });
});
