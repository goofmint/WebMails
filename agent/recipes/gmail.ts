// Gmail recipe (design.md §2.2.14, §6.2, §7.1; SPEC.md §7.1). Reads
// `GET /mail/u/<segment>/feed/atom`, an undocumented-but-long-standing
// Atom feed that (for a signed-in session) returns `<fullcount>` plus one
// `<entry>` per unread message. First use validates the response; any
// validation failure switches this recipe to the title strategy for the
// rest of the page's life (a fresh page load creates a fresh recipe via
// `createGmailRecipe`, so the fallback is never permanent across reloads).
import type {
  FetchTextResult,
  MessageRef,
  Recipe,
  RecipeContext,
  RecipeDescription,
  UnreadResult,
} from "./types";
import { fetchText } from "../strategies/fetch";
import { titleCount, watchTitle } from "../strategies/title";

const HOST = "mail.google.com";

// Gmail's title looks like "Inbox (3) - me@example.com - Gmail" when there
// is an unread count, and has no parenthesized number at all when there is
// none (e.g. "Some subject - me@example.com - Gmail" for an opened
// message). Anchoring on the trailing " - Gmail" and requiring the count to
// be immediately followed by " - " keeps this from matching an unrelated
// parenthesized number elsewhere in the title (e.g. a year in an opened
// message's subject) — something the fully generic `\((\d+)\)` pattern
// cannot distinguish.
const GMAIL_TITLE_PATTERN = /\((\d+)\) - .+ - Gmail$/;

// `from` and `subject` are capped at 512 Unicode code points (not UTF-16
// code units, so surrogate pairs are never split), and at most this many
// messages are ever returned, matching the CodeRabbit review plan for this
// task.
const MAX_TEXT_CODE_POINTS = 512;
const MAX_MESSAGES = 100;

// `MessageRef.id` is a hash of the feed entry's raw `<id>` text, not the
// raw text itself (SPEC.md §6.2: "state.json contains message ids (hashes)
// only"). Hashing is SHA-256 via `crypto.subtle` (available in every
// evergreen webview this agent runs in, and in the Vitest/jsdom test
// environment used here — verified to support `digest()`, not just
// `typeof crypto.subtle`), hex-encoded and truncated to the leftmost 16
// hex characters (64 of the 256 bits). 64 bits of a cryptographic digest is
// far more collision-resistant than this app ever needs (one mailbox's
// unread list, capped at 100 entries) while keeping stored/logged ids
// short; the truncation point is fixed here so hashing stays deterministic
// across calls and across recipe instances.
const ID_HASH_HEX_LENGTH = 16;

type GmailMode = "unvalidated" | "feed" | "title";

interface GmailFeedEntry {
  readonly rawId: string;
  readonly from: string | null;
  readonly subject: string | null;
  readonly messageId: string | null;
}

export interface GmailFeedParseResult {
  readonly fullcount: number;
  readonly entries: readonly GmailFeedEntry[];
}

// Derives the feed path from the service URL's own `/mail/u/<segment>/...`
// path segment, never from `document.location` (the current page URL
// changes during login/navigation; the service URL does not). Falls back
// to segment `0` when the service URL carries no `/mail/u/<segment>`
// segment at all (e.g. a bare `https://mail.google.com/`).
export function gmailFeedPath(serviceUrl: URL): string {
  const match = /\/mail\/u\/([^/]+)/.exec(serviceUrl.pathname);
  const segment = match?.[1] ?? "0";
  return `/mail/u/${segment}/feed/atom`;
}

function childrenByLocalName(parent: Element, localName: string): Element[] {
  return Array.from(parent.children).filter((child) => child.localName === localName);
}

function firstChildByLocalName(parent: Element, localName: string): Element | null {
  return childrenByLocalName(parent, localName)[0] ?? null;
}

function textOf(el: Element | null): string | null {
  const text = el?.textContent?.trim();
  return text === undefined ? null : text;
}

function truncateCodePoints(text: string, maxCodePoints: number): string {
  const codePoints = Array.from(text);
  return codePoints.length <= maxCodePoints ? text : codePoints.slice(0, maxCodePoints).join("");
}

// The historical, undocumented Gmail Atom feed puts each entry's permalink
// in a `<link rel="alternate" href="...&message_id=<id>&...">` element; the
// query parameter is used as-is (no base conversion), since it is already
// in the form Gmail's own web UI accepts as a deep-link fragment.
function extractMessageId(entryEl: Element): string | null {
  const linkEl =
    childrenByLocalName(entryEl, "link").find((el) => el.getAttribute("rel") === "alternate") ??
    firstChildByLocalName(entryEl, "link");
  const href = linkEl?.getAttribute("href");
  if (href === null || href === undefined || href === "") {
    return null;
  }
  let messageId: string | null;
  try {
    messageId = new URL(href).searchParams.get("message_id");
  } catch {
    return null;
  }
  return messageId === null || messageId === "" ? null : messageId;
}

function buildDeepLink(serviceUrl: URL, messageId: string): string {
  const url = new URL(serviceUrl.toString());
  url.hash = `all/${messageId}`;
  return url.toString();
}

function extractFrom(authorEl: Element | null): string | null {
  if (authorEl === null) {
    return null;
  }
  const name = textOf(firstChildByLocalName(authorEl, "name"));
  const email = textOf(firstChildByLocalName(authorEl, "email"));
  const from = name !== null && name !== "" ? name : email !== null && email !== "" ? email : null;
  return from === null ? null : truncateCodePoints(from, MAX_TEXT_CODE_POINTS);
}

function extractSubject(entryEl: Element): string | null {
  const subject = textOf(firstChildByLocalName(entryEl, "title"));
  return subject === null || subject === ""
    ? null
    : truncateCodePoints(subject, MAX_TEXT_CODE_POINTS);
}

// Parses a Gmail Atom feed body (`DOMParser`, `application/xml`). Returns
// `null` — "malformed" — when: the XML does not parse at all (a
// `parsererror` element appears anywhere in the tree, which is where every
// engine puts it); `<fullcount>` is missing, non-numeric, negative, or not
// a safe integer. An entry with no `<id>` is skipped rather than failing
// the whole feed (design.md §2.2.14: "`id` is a hash of the entry id", so
// an entry without one has nothing to hash). Element lookup uses
// `localName` throughout so the feed's `http://purl.org/atom/ns#`
// namespace never has to be matched exactly.
export function parseGmailAtom(body: string): GmailFeedParseResult | null {
  const doc = new DOMParser().parseFromString(body, "application/xml");
  if (doc.getElementsByTagName("parsererror").length > 0) {
    return null;
  }
  const root = doc.documentElement;
  const fullcountText = textOf(firstChildByLocalName(root, "fullcount"));
  if (fullcountText === null || !/^\d+$/.test(fullcountText)) {
    return null;
  }
  const fullcount = Number(fullcountText);
  if (!Number.isSafeInteger(fullcount)) {
    return null;
  }

  const entries: GmailFeedEntry[] = [];
  for (const entryEl of childrenByLocalName(root, "entry")) {
    if (entries.length >= MAX_MESSAGES) {
      break;
    }
    const rawId = textOf(firstChildByLocalName(entryEl, "id"));
    if (rawId === null || rawId === "") {
      continue;
    }
    entries.push({
      rawId,
      from: extractFrom(firstChildByLocalName(entryEl, "author")),
      subject: extractSubject(entryEl),
      messageId: extractMessageId(entryEl),
    });
  }

  return { fullcount, entries };
}

function toHex(buffer: ArrayBuffer): string {
  return Array.from(new Uint8Array(buffer))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

async function hashEntryId(rawId: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(rawId));
  return toHex(digest).slice(0, ID_HASH_HEX_LENGTH);
}

async function toMessageRefs(
  entries: readonly GmailFeedEntry[],
  serviceUrl: URL,
): Promise<readonly MessageRef[]> {
  return Promise.all(
    entries.map(async (entry): Promise<MessageRef> => ({
      id: await hashEntryId(entry.rawId),
      from: entry.from,
      subject: entry.subject,
      link: entry.messageId === null ? null : buildDeepLink(serviceUrl, entry.messageId),
    })),
  );
}

function titleOnlyResult(ctx: RecipeContext): UnreadResult {
  return { count: titleCount(ctx.document, GMAIL_TITLE_PATTERN) ?? 0, messages: [] };
}

// Builds one Gmail recipe instance with its own private `mode` state
// (`createGmailRecipe` is called once, below, to produce the exported
// `gmail` recipe — a fresh page load reruns `main.ts`, which re-imports
// this module and gets a fresh closure, so the sticky fallback never
// outlives the page it was observed on).
export function createGmailRecipe(): Recipe {
  let mode: GmailMode = "unvalidated";

  return {
    id: "gmail",
    displayName: "Gmail",
    defaultProfile: "default",

    matches(serviceUrl: URL): boolean {
      return serviceUrl.hostname === HOST;
    },

    describe(serviceUrl: URL): RecipeDescription {
      return {
        strategy: mode === "title" ? "title" : "fetch",
        reads: `GET ${gmailFeedPath(serviceUrl)}`,
      };
    },

    async read(ctx: RecipeContext): Promise<UnreadResult> {
      if (mode === "title") {
        return titleOnlyResult(ctx);
      }

      let fetched: FetchTextResult;
      try {
        fetched = await fetchText(ctx, gmailFeedPath(ctx.serviceUrl));
      } catch {
        // A fetch failure (network error, or a cross-origin rejection from
        // `SameOriginFetch`, e.g. a login redirect that left Gmail's origin)
        // is a failed read: report `count: null` for this cycle (design.md
        // §5.1). It never changes `mode`, so the next read retries the feed.
        return { count: null };
      }

      const titleHasCount = titleCount(ctx.document, GMAIL_TITLE_PATTERN) !== null;

      // Signed-out state (design.md §2.2.14, D1): a same-origin redirect
      // away from the feed, or a 401, *combined with* a title that carries
      // no count (the title strategy alone cannot tell an empty inbox from
      // a login page). This is checked before anything else and never
      // changes `mode` — signing back in must let the feed resume working.
      if ((fetched.redirected || fetched.status === 401) && !titleHasCount) {
        return { count: null };
      }

      if (fetched.status === 200 && !fetched.redirected) {
        const parsed = parseGmailAtom(fetched.body);
        if (parsed !== null) {
          mode = "feed";
          const messages = await toMessageRefs(parsed.entries, ctx.serviceUrl);
          return { count: parsed.fullcount, messages };
        }
      }

      // Anything else — a non-signed-out redirect, a non-200/401 status,
      // 401 with a title count present, or a malformed body — is a
      // validation failure. On first use this sticks the recipe to the
      // title strategy for the rest of the page's life; once the feed has
      // already validated once (`mode === "feed"`), the same failure is
      // read as transient and `mode` stays `"feed"`.
      if (mode === "unvalidated") {
        mode = "title";
      }
      return titleOnlyResult(ctx);
    },

    watch(ctx: RecipeContext, onChange: () => void): () => void {
      return watchTitle(ctx.document, onChange);
    },
  };
}

export const gmail: Recipe = createGmailRecipe();
