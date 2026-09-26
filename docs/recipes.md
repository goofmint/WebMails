# Writing a recipe

A **recipe** teaches Eluma how to read one webmail service's unread count (and,
where possible, sender/subject/link for individual messages) from inside that
service's own webview. Adding a service to Eluma means adding one TypeScript
module and registering it — no Rust is touched (`SPEC.md` §7.5).

This guide documents the current agent code under `agent/recipes/` and
`agent/strategies/`. Every type, constant and file path below is quoted from
the actual source, not from the design doc's sketch of it — where the two
differ (there is one such case, noted below), the source wins.

## How this fits together

- Recipes are **compiled into the app**. Nothing is loaded from disk or the
  network at runtime, and there is no `eval` or dynamic script loading
  anywhere in this path (`SPEC.md` §14). A recipe reaches users only through a
  reviewed PR and a release.
- The agent (`agent/main.ts`, not covered in depth here) picks a recipe with
  `matchRecipe(new URL(serviceUrl))` (`agent/recipes/registry.ts`) and calls
  its `read()`/`watch()` on a loop (`design.md` §2.2.14).
- Each result is sent to Rust via `report_unread`
  (`agent/core/report.ts`'s `createReporter`). Rust validates every field
  independently (`src-tauri/src/agent_bridge/validate.rs`) — see
  [The report contract](#the-report-contract) below — because the page's own
  scripts can call the same command (`SPEC.md` §7.4).

## The `Recipe` interface

All types below are exported from `agent/recipes/types.ts`. The file's own
header comment is explicit that these names must not change without updating
`design.md` §2.2.14 first — treat the interface as frozen unless you're doing
that.

```ts
export type ProfileDefault = "default" | "isolated";

export type UnreadResult =
  { readonly count: number; readonly messages: readonly MessageRef[] } | { readonly count: null };

export interface MessageRef {
  readonly id: string;
  readonly from: string | null;
  readonly subject: string | null;
  readonly link: string | null;
}

export type SameOriginFetch = (path: string, init?: RequestInit) => Promise<Response>;

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
```

Field by field:

- **`id`** — a short, stable, lowercase identifier (`"gmail"`, `"generic"`).
  It is sent to Rust as `recipeId` and validated there against
  `[a-z0-9-]{1,64}` (`src-tauri/src/agent_bridge/validate.rs`,
  `ReportError::RecipeIdInvalidChars` / `RecipeIdTooLong`) — keep your `id`
  within that character set and length from the start.
- **`displayName`** — shown to the user (SPEC §14: "A per-service panel shows
  what its recipe does"). Human-readable, e.g. `"Gmail"`, `"iCloud Mail"`.
- **`defaultProfile`** — `"default"` or `"isolated"` (see `design.md` §5 for
  what these mean for session storage). Gmail uses `"default"`; every other
  current recipe uses `"isolated"`. Pick `"isolated"` unless you have a
  specific reason not to.
- **`matches(serviceUrl)`** — pure, synchronous, host/path check against the
  _service URL_ the user configured, never against `document.location` (the
  current page navigates during login; the service URL does not). See
  [Registration](#registration-in-registryts) for the exact-host rule.
- **`describe(serviceUrl)`** — pure, returns a `RecipeDescription`. This is
  the only thing a settings-panel UI needs to show "what this recipe reads"
  (`SPEC.md` §14) — see [How `describe()` is used](#how-describe-is-used).
  `strategy` must be one of `"title" | "fetch" | "selector"`; `reads` is a
  short human-readable string such as `"GET /mail/u/0/feed/atom"` or a title
  regex's `.toString()`.
- **`read(ctx)`** — async, returns a `Promise<UnreadResult>`. This is where
  the count-vs-null rule below applies.
- **`watch(ctx, onChange)`** — installs an observer (title `MutationObserver`,
  DOM `MutationObserver`, etc.) and calls `onChange()` whenever the
  underlying signal changes; returns an unsubscribe function. Must never
  throw synchronously, and must stop calling `onChange` once unsubscribed.

### The `count: null` vs `count: 0` rule

This is the one rule every recipe must get right (`design.md` §2.2.14 "D1",
`SPEC.md` §7.4):

- **`count: null`** means "the agent is running but cannot produce a count":
  a login/signed-out page, a load failure, or a recipe that could not read
  the page. Rust renders this as `NeedsAttention` and it **never** clears or
  triggers a notification (`SPEC.md` §7.4). `messages` is always empty when
  `count` is `null` — the type itself enforces this (`UnreadResult`'s second
  arm has no `messages` field at all).
- **`count: 0`** means "signed in, readable, and there really are zero unread
  messages."
- **The title strategy alone cannot tell these apart** — a same-origin login
  page and an empty inbox can both produce a title with no parenthesized
  count. That is why `titleCount()` returning `null` (see below) is _not_
  automatically `count: null` at the `Recipe` level: `generic.ts` and
  Gmail's title-only fallback both convert a `titleCount() === null` into
  `{ count: 0, messages: [] }`, not `{ count: null }`.
- A recipe should only return `{ count: null }` when it has an
  **independent** way to detect a signed-out/unreachable state. Gmail's is a
  same-origin redirect or a `401` from the feed fetch, _combined with_ the
  title having no count either (`agent/recipes/gmail.ts`'s `read()`) —
  either signal alone is not enough, because a `401`/redirect can also occur
  transiently on a page that already has a count in its title.

If you don't have such a signal, don't invent one — return `0` when there's
no count found, same as `generic`.

## Rules

These are **review requirements**, not something the runtime sandboxes a
recipe into (`design.md` §6.2: "The agent never mutates the DOM, never
clicks, and never submits ... Review checks recipes for this."). A recipe is
plain TypeScript compiled into the app; nothing stops it, at the language
level, from calling `.click()` on an element. Reviewers must check for this
by reading the diff.

- **Read-only.** A recipe must never mutate the DOM, click an element, submit
  a form, or otherwise interact with the page beyond reading it
  (`design.md` §6.2, §7.3; `SPEC.md` §7.3). Read `document`/DOM nodes and
  call `ctx.fetch` — nothing else touches the page.
- **Same-origin only, via `ctx.fetch`.** All network access goes through
  `ctx.fetch` (a `SameOriginFetch`, built by `createSameOriginFetch` in
  `agent/strategies/fetch.ts`). This is runtime-enforced, not just a review
  rule: it throws (rejects the returned promise) before calling the
  underlying `fetch` when:
  - the path doesn't resolve to a URL at all;
  - the resolved URL's origin (scheme + host + port) differs from
    `serviceUrl`'s, including protocol-relative URLs and non-`http(s)`
    schemes such as `data:`/`blob:`/`javascript:`;
  - the final response URL (after redirects) is on a different origin — e.g.
    a cross-origin login redirect;
  - `init.method` is present and isn't `GET` (case-insensitive; an omitted
    method defaults to `GET` and is allowed);
  - `init.body` is present.

  `init.mode` and `init.credentials` are always forced to `"same-origin"`;
  other `init` fields pass through unchanged. In short: **`ctx.fetch` is
  GET-only, and only within the service's own origin.**

- **No `eval`, `Function`, or dynamic script loading.** Recipes are reviewed,
  compiled TypeScript; nothing is loaded from disk or network at runtime
  (`SPEC.md` §14).
- **No third-party requests.** Every fetch a recipe makes must be to the
  service's own origin (enforced by `ctx.fetch`, above). Never call the
  global `fetch`/`XMLHttpRequest` directly from a recipe.
- **Ids are hashed, never raw.** `state.json` stores message ids only as
  hashes, never raw provider ids, subjects or senders (`design.md` §6.2).
  `agent/recipes/gmail.ts`'s `hashEntryId` is the existing convention: SHA-256
  via `crypto.subtle.digest` (available in every target webview and in the
  Vitest/jsdom test environment), hex-encoded, and truncated to a fixed
  length (16 hex characters there — `ID_HASH_HEX_LENGTH`). Follow the same
  pattern for any new recipe that has a provider-specific raw id to turn into
  `MessageRef.id`.
- **Truncate long text yourself.** Nothing downstream truncates for you
  gracefully — Rust's validator _rejects_ an over-length field outright (see
  [The report contract](#the-report-contract)). `gmail.ts`'s
  `truncateCodePoints` caps `from`/`subject` at `MAX_TEXT_CODE_POINTS` (512
  Unicode code points, splitting on code points so a surrogate pair is never
  cut in half) and caps the number of messages at `MAX_MESSAGES` (100).
  Match or stay under Rust's limits (below).

## The report contract

`Recipe.read()`'s result eventually becomes an `UnreadReportDto` (built by
`createReporter` in `agent/core/report.ts`) sent through `report_unread`, and
validated in `src-tauri/src/agent_bridge/validate.rs`. A recipe author
doesn't call any of this directly, but the limits below are enforced there
and a report that violates them is **dropped entirely, not clamped** — so a
recipe that produces an out-of-bounds value silently loses that report cycle.

| Field                | Required?                 | `null` allowed?             | Limit / check                                                           | On violation                                                                           |
| -------------------- | ------------------------- | --------------------------- | ----------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `count`              | yes                       | yes (signed-out/unreadable) | `0 ≤ count ≤ 1_000_000` when non-null                                   | negative → `CountNegative`; over max → `CountTooLarge`                                 |
| `messages`           | yes (array, may be empty) | n/a                         | at most 100 entries                                                     | `TooManyMessages`                                                                      |
| `messages[].id`      | yes                       | no                          | at most 512 Unicode scalar values                                       | `StringTooLong`                                                                        |
| `messages[].from`    | yes (key present)         | yes                         | at most 512 Unicode scalar values                                       | `StringTooLong`                                                                        |
| `messages[].subject` | yes (key present)         | yes                         | at most 512 Unicode scalar values                                       | `StringTooLong`                                                                        |
| `messages[].link`    | yes (key present)         | yes                         | must be `https`, on the service's own origin                            | `InvalidLink`                                                                          |
| `recipeId`           | yes                       | no                          | 1–64 chars, `[a-z0-9-]` only                                            | `RecipeIdTooLong` / `RecipeIdInvalidChars`                                             |
| `iconCandidates`     | yes (array, may be empty) | n/a                         | at most 8 entries; `http`/`https`; host not loopback/private/link-local | `TooManyIconCandidates` / `IconCandidateInvalidScheme` / `IconCandidateDisallowedHost` |

Two things worth calling out explicitly since they're easy to get backwards:

- A **nullable field's key must still be present** — Rust's DTO deserializer
  (`agent_bridge/dto.rs`'s `required_nullable`) treats an _omitted_ key as a
  deserialization error, distinct from an explicit `null`. `report.ts`
  already always sends `from`/`subject`/`link` as `null` rather than omitting
  them, and `count: null` the same way — a new recipe doesn't need to do
  anything extra here since it only ever returns `UnreadResult`, which
  `report.ts` converts.
- `link`, if not `null`, is validated as being on the **service's own
  origin** — if your recipe can't build a same-origin deep link for a
  message (no permalink information, or the destination is legitimately
  elsewhere), use `null` for `link` rather than a link you can't guarantee is
  same-origin. `gmail.ts` does exactly this: an entry whose feed `<link>` has
  no usable `message_id` gets `link: null`.

`observedAt` is informational only (`design.md` §2.2.6 as summarized in
`dto.rs`'s doc comment) — liveness is judged by when Rust _receives_ the
report, not by this timestamp. `report.ts` fills it from an injected `Clock`;
recipe authors don't set it.

## Strategy helpers

Recipes don't reimplement page-reading from scratch; they call shared
helpers. All three strategies in `SPEC.md` §7.1 are implemented:
`agent/strategies/title.ts`, `agent/strategies/fetch.ts`, and
`agent/strategies/selector.ts`.

### `title` — `agent/strategies/title.ts`

```ts
export function titleCount(doc: Document, pattern: RegExp): number | null;
export function watchTitle(doc: Document, cb: () => void): Unwatch; // Unwatch = () => void
```

- `titleCount` matches `pattern` against `doc.title` and returns the first
  capture group parsed as a decimal integer, or `null` when there's no match
  **or** the captured digits don't parse to a safe, non-negative integer.
  `null` means "no count found in the title" — nothing more; see
  [the count rule](#the-count-null-vs-count-0-rule) for how callers turn
  that into `0` or `null` at the `Recipe` level. A `g`/`y`-flagged `pattern`
  is handled correctly (matching never relies on `lastIndex`).
- `watchTitle` puts a `MutationObserver` on `doc.head` (falling back to
  `doc.documentElement` if `head` is absent) with `childList: true, subtree:
true, characterData: true`, so it catches both an in-place title text edit
  and the `<title>` element itself being replaced. It never mutates the
  document. Returns an unsubscribe function that disconnects the observer.

Example (this is exactly what `agent/recipes/generic.ts` does):

```ts
const TITLE_COUNT_PATTERN = /\((\d+)\)/;

read(ctx: RecipeContext): Promise<UnreadResult> {
  const count = titleCount(ctx.document, TITLE_COUNT_PATTERN);
  return Promise.resolve({ count: count ?? 0, messages: [] });
},

watch(ctx: RecipeContext, onChange: () => void): () => void {
  return watchTitle(ctx.document, onChange);
},
```

A more specific title pattern, anchored so it doesn't match an unrelated
parenthesized number elsewhere in the title (from `agent/recipes/gmail.ts`):

```ts
// "Inbox (3) - me@example.com - Gmail" has a count; an opened message's
// title (no leading "(n) ") does not. Anchoring on " - Gmail" at the end
// and requiring " - " right after the count keeps this from matching, say,
// a year in an opened message's subject.
const GMAIL_TITLE_PATTERN = /\((\d+)\) - .+ - Gmail$/;
```

### `fetch` — `agent/strategies/fetch.ts`

```ts
export function createSameOriginFetch(serviceUrl: URL, baseFetch?: typeof fetch): SameOriginFetch;
export async function fetchText(ctx: RecipeContext, path: string): Promise<FetchTextResult>;
```

- `createSameOriginFetch` is what builds `ctx.fetch` — see
  [Rules](#rules) above for exactly what it rejects and why. Recipes don't
  call this directly; it's already on `ctx`.
- `fetchText(ctx, path)` calls `ctx.fetch(path)` and reads the body as text,
  returning `{ status, redirected, url, body }`. **Non-200 responses are
  returned, not rejected** — only a cross-origin target rejects (propagated
  from `ctx.fetch`). No parsing or fallback logic lives here; that's each
  recipe's job.

Example (`agent/recipes/gmail.ts`'s `read()`, simplified):

```ts
let fetched: FetchTextResult;
try {
  fetched = await fetchText(ctx, gmailFeedPath(ctx.serviceUrl));
} catch {
  // Cross-origin rejection, network failure, etc. — treated as a one-off
  // miss, not a validation failure: fall back to the title for this cycle
  // only.
  return titleOnlyResult(ctx);
}

if (fetched.status === 200 && !fetched.redirected) {
  const parsed = parseGmailAtom(fetched.body);
  if (parsed !== null) {
    return {
      count: parsed.fullcount,
      messages: await toMessageRefs(parsed.entries, ctx.serviceUrl),
    };
  }
}
// ...non-200/redirect/malformed handling, and the sticky-fallback-to-title
// state machine on first use — see the file itself for the full logic.
```

A `fetch`-strategy recipe that parses a custom body format (Gmail's Atom
feed) does that parsing itself — `parseGmailAtom` in `gmail.ts` is a
recipe-local helper, not a shared strategy function, because the response
shape is specific to Gmail's feed.

### `selector` — `agent/strategies/selector.ts`

```ts
export function selectorCount(
  document: Document,
  selector: string,
  mode: "text" | "rows",
): number | null;
export function watchSelector(document: Document, selector: string, cb: () => void): () => void;
```

The last resort (`SPEC.md` §7.1: "fragile in exactly the way Ferdium recipes
are fragile"), for a service with no title count and no cheap endpoint. Reads
a CSS-selected node instead. No current recipe uses it yet — reach for
`title` first, `fetch` where a cheap same-origin endpoint exists, and
`selector` only when neither is available.

- **Both functions validate the selector eagerly.** An invalid CSS selector
  throws immediately (`selector strategy: invalid selector "...": <reason>`,
  with the original `DOMException` as `cause`) rather than being swallowed
  into `null`/a no-op — an invalid selector is a bug in the recipe, not a
  "no data" signal.
- **`selectorCount(document, selector, mode)`:**
  - `mode: "rows"` returns `document.querySelectorAll(selector).length`.
    `0` here is a real signal (no matching rows) — unlike `"text"` mode
    below, this can legitimately mean "confirmed zero", not "couldn't read
    it".
  - `mode: "text"` reads the first matching element's `textContent` and
    parses the first run of digits out of it (comma thousands separators
    are stripped, e.g. `"1,234"` → `1234`; trailing non-digit text is
    ignored, so a capped badge like `"99+"` parses as `99`). Returns `null`
    — not `0` — when there is no matching element, the text is empty, or it
    has no parseable digit run: per the [count-null-vs-0 rule](#the-count-null-vs-count-0-rule),
    a missing or unparseable signal must never be reported as a confirmed
    zero.
- **`watchSelector(document, selector, cb)`** puts a `MutationObserver` on
  `document.documentElement` (`childList`, `subtree`, `characterData`) and
  calls `cb` at most once per mutation batch, whenever a mutation's target,
  or an added/removed node, matches, contains, or is contained by an element
  matching `selector` — so both a text edit inside the matched element and
  the matched element itself being replaced, added, or removed are caught.
  Coalescing/debouncing calls to `cb` _across_ batches is the caller's job,
  same as `watchTitle`; `watchSelector` only guarantees at most one call per
  batch. Returns an unsubscribe function that disconnects the observer and
  stops further calls to `cb`.

Example — a `"rows"`-mode recipe counting `<li class="unread">` elements in
an inbox list:

```ts
const ROW_SELECTOR = "li.unread";

read(ctx: RecipeContext): Promise<UnreadResult> {
  const count = selectorCount(ctx.document, ROW_SELECTOR, "rows");
  return Promise.resolve({ count: count ?? 0, messages: [] });
},

watch(ctx: RecipeContext, onChange: () => void): () => void {
  return watchSelector(ctx.document, ROW_SELECTOR, onChange);
},
```

See `agent/strategies/selector.test.ts` for the reference test coverage for a
`selector`-strategy helper: both modes' parsing rules, the invalid-selector
throw (in both `selectorCount` and `watchSelector`), a matched element's text
changing, a matched element being replaced/added/removed, an unrelated DOM
change _not_ triggering `cb`, multiple relevant changes in one batch
coalescing to a single `cb` call, and unsubscribe.

## Registration in `registry.ts`

`agent/recipes/registry.ts`:

```ts
const specificRecipes: readonly Recipe[] = [gmail, icloud, outlook];
export const recipes: readonly Recipe[] = [...specificRecipes, generic];

export function matchRecipe(serviceUrl: URL): Recipe {
  for (const recipe of specificRecipes) {
    if (recipe.matches(serviceUrl)) {
      return recipe;
    }
  }
  return generic;
}
```

- Add your recipe to `specificRecipes`, in whatever order you like relative
  to the other specific recipes (their host patterns don't overlap in
  practice, but `matchRecipe` always takes the first match, so put a more
  specific matcher earlier if you're ever unsure).
- **`generic` must stay out of `specificRecipes` and last in `recipes`.**
  `generic.matches()` always returns `true`, so it must never be checked
  before a more specific recipe — `registry.test.ts` asserts both the exact
  order (`["gmail", "icloud", "outlook", "generic"]`) and that `generic` is
  last.
- **`matches()` must be an exact host check with an explicit path boundary**
  — not `hostname.includes(...)` or a substring check, both of which a
  look-alike domain can defeat. The existing recipes show the two shapes:
  - Host-only (`agent/recipes/gmail.ts`, `outlook.ts`):
    `serviceUrl.hostname === HOST`. This alone already rejects
    `evil-mail.google.com.example` (a _suffix_ look-alike) and
    `notmail.google.com.evil.example` (a _substring_ look-alike) — `===`
    does not do substring matching.
  - Host **and** path boundary (`agent/recipes/icloud.ts`):
    ```ts
    matches(serviceUrl: URL): boolean {
      if (serviceUrl.hostname !== HOST) {
        return false;
      }
      return serviceUrl.pathname === "/mail" || serviceUrl.pathname.startsWith("/mail/");
    }
    ```
    Note the explicit `pathname.startsWith("/mail/")` (with the trailing
    slash) rather than `startsWith("/mail")` — the latter would also match
    `/mailfoo`. `registry.test.ts` has a case for exactly this
    (`https://www.icloud.com/mailfoo` must fall through to `generic`).

## Fixture-based tests

Vitest runs in the `jsdom` environment for everything under `agent/` (and
`src/`), configured in `vitest.config.ts`:

```ts
test: {
  environment: "jsdom",
  setupFiles: ["src/test/setup.ts"],
  include: ["src/**/*.test.{ts,tsx}", "agent/**/*.test.ts"],
}
```

So `document`, `DOMParser`, `MutationObserver`, `crypto.subtle`, etc. are all
available in tests without extra setup. Run the suite with `pnpm test`
(Vitest) or narrow it to one recipe with `pnpm test agent/recipes/gmail`.

**Fixtures live next to the recipe**, under `agent/recipes/__fixtures__/<recipe-id>/`.
The only recipe with fixtures today is Gmail:

```
agent/recipes/__fixtures__/gmail/feed-valid.xml
agent/recipes/__fixtures__/gmail/feed-empty.xml
agent/recipes/__fixtures__/gmail/feed-malformed.xml
agent/recipes/__fixtures__/gmail/login-redirect.html
```

Import a fixture as a raw string with Vite's `?raw` suffix (this is a Vite
feature, not a project-specific helper — nothing else needs configuring):

```ts
import feedValid from "./__fixtures__/gmail/feed-valid.xml?raw";
```

`agent/recipes/gmail.test.ts` is the reference for what a `fetch`-strategy
recipe's tests should cover; `agent/recipes/generic.test.ts` for a
`title`-strategy recipe. Between them, a new recipe's test file should cover:

- **`matches()`**: the real host/path matches, plus every look-alike your
  registration logic is supposed to reject (suffix and substring look-alikes
  at minimum — see `registry.test.ts` for the exact cases run against every
  registered recipe).
- **`describe()`**: the reported `strategy` and `reads` string, before and
  (if your recipe can switch strategy, like Gmail's sticky fallback) after a
  state change.
- **A fake `ctx.fetch`**: don't hit the network. Build a `RecipeContext`
  whose `fetch` is a `vi.fn()` returning a `Response`-like value (or
  rejecting, to test the cross-origin/network-failure path). Gmail's test
  file's `makeFetchResponse` helper is the pattern — `redirected` and `url`
  are real `Response` getters with no setter, so overriding them needs
  `Object.defineProperty`, not plain assignment.
- **Fake timers / async flushing**: `watch()` callbacks fire via
  `MutationObserver`, which resolves on a microtask — `await
Promise.resolve()` (or a small `flushMicrotasks()` helper, as in
  `generic.test.ts`) after mutating the DOM, before asserting `onChange` was
  called.
- **DOM changes, not just initial state**: replacing the `<title>` element
  outright (not just editing its text), per `generic.test.ts`'s
  `"watch() notifies on a replaced <title> element"` case — `watchTitle`'s
  `MutationObserver` is on `<head>` specifically so this is caught.
- **Unsubscribe**: call the function `watch()` returns, then change the
  underlying signal again, and assert `onChange` was _not_ called again.
- **Every branch of the `count`-vs-`null` rule** your recipe implements: a
  normal count, no-count-found (→ `0`), and (if applicable) your recipe's own
  signed-out detection (→ `null`) — plus, if that detection combines two
  signals like Gmail's (redirect/401 _and_ no title count), a case for each
  signal firing _without_ the other, showing the fallback to the title count
  instead of `null`.
- **No personal data in fixtures.** Fixture bodies must use placeholder
  names/emails/subjects (Gmail's fixtures use `Alice Example`,
  `alice@example.com`-style addresses) — never real captured traffic.

## How `describe()` is used

`SPEC.md` §14: "A per-service panel shows what its recipe does: which
strategy, which selector or endpoint, what it reads." `tasks.md` Task 5.1
("Recipe panel") is the settings component that renders this — it shows each
service's `recipe.displayName` plus its `describe(serviceUrl)` result
(`strategy` and `reads`) for every configured service. That component does
not exist in this codebase yet (Task 5.1 is tracked separately from this
guide, Task 5.2); what matters for a recipe author is that `describe()` must
stay a **pure, synchronous, side-effect-free** function of `serviceUrl` — the
panel can call it at any time, independent of whether `read()` has ever run,
to show what the recipe is _configured_ to do. Where a recipe's strategy can
change at runtime (Gmail's sticky fallback to `title`), `describe()` must
reflect the _current_ state, as `gmail.test.ts`'s
`"reports the title strategy once validation has failed"` case checks.

## Worked example: adding a new title-based recipe

Say you're adding a fictional service, `mail.example-mail.test`, whose title
looks like `"(3) Inbox — ExampleMail"` when signed in with unread mail, and
has no parenthesized count when there's none or when signed out (same
ambiguity as `generic`, so this recipe returns `0`, not `null`, on no match —
see [the count rule](#the-count-null-vs-count-0-rule)).

1. **Write the recipe module**, `agent/recipes/examplemail.ts`:

   ```ts
   import type { Recipe, RecipeContext, RecipeDescription, UnreadResult } from "./types";
   import { titleCount, watchTitle } from "../strategies/title";

   const HOST = "mail.example-mail.test";
   const TITLE_PATTERN = /^\((\d+)\)/;

   export const examplemail: Recipe = {
     id: "examplemail",
     displayName: "ExampleMail",
     defaultProfile: "isolated",

     matches(serviceUrl: URL): boolean {
       return serviceUrl.hostname === HOST;
     },

     describe(): RecipeDescription {
       return { strategy: "title", reads: `Document title matches ${TITLE_PATTERN.toString()}` };
     },

     read(ctx: RecipeContext): Promise<UnreadResult> {
       const count = titleCount(ctx.document, TITLE_PATTERN);
       return Promise.resolve({ count: count ?? 0, messages: [] });
     },

     watch(ctx: RecipeContext, onChange: () => void): () => void {
       return watchTitle(ctx.document, onChange);
     },
   };
   ```

2. **Register it** in `agent/recipes/registry.ts`: import `examplemail` and
   add it to `specificRecipes` (anywhere before `generic`, which stays
   implicit and last).

3. **Add a test file**, `agent/recipes/examplemail.test.ts`, modeled on
   `generic.test.ts` since this is a pure title-strategy recipe: `matches()`
   against the real host and at least one look-alike host; `describe()`;
   `read()` for a title with a count, a title with none (→ `0`); `watch()`
   noticing a replaced `<title>` element and stopping after unsubscribe.

4. **Run `pnpm test agent/recipes/examplemail`**, then the full suite
   (`pnpm test`), `pnpm lint`, and `pnpm typecheck`.

5. If a manual account is available, follow `docs/manual-checks.md`'s pattern
   to record that the real service shows a live count — this guide's own
   commands (steps 1–4) can be verified without one.

No Rust file changes, and no changes to `src-tauri/`, are needed to add a
recipe.

## Reviewer checklist

- [ ] `matches()` is an exact host check (`hostname === "..."`), with an
      explicit path boundary if the recipe is also path-scoped (`===` or
      `startsWith("/segment/")` — never a bare substring/`includes` check).
- [ ] The recipe never mutates the DOM, clicks, or submits — read-only
      throughout `read()` and `watch()`.
- [ ] All network access goes through `ctx.fetch`; nothing calls the global
      `fetch`/`XMLHttpRequest` directly.
- [ ] No `eval`, `Function(...)`, or dynamically constructed/loaded script.
- [ ] `count: null` is returned only when the recipe has an independent
      signed-out/unreachable signal — not merely "no count found," which is
      `0`.
- [ ] `messages` is empty whenever `count` is `null` (the type already
      enforces this, but check the recipe doesn't work around it).
- [ ] Any provider-specific raw id becomes a hashed `MessageRef.id`, never
      the raw id itself.
- [ ] `from`/`subject` are truncated to a bounded length (matching or under
      Rust's 512-Unicode-scalar-value limit — see
      [The report contract](#the-report-contract)) and `messages` is capped
      to a bounded count (100 or fewer).
- [ ] `link`, when non-null, is same-origin and `https` — or `null` if that
      can't be guaranteed.
- [ ] `watch()` returns a working unsubscribe, and stops calling `onChange`
      after it's called.
- [ ] Tests cover: matching (real host + look-alikes), `describe()`, every
      branch of the count/null logic, `watch()`'s replaced-element and
      unsubscribe cases, and (for a `fetch` recipe) a fake `ctx.fetch`
      covering the non-200/redirect/malformed-body paths, using fixtures
      under `agent/recipes/__fixtures__/<recipe-id>/` with no real personal
      data.
- [ ] `pnpm test`, `pnpm lint`, and `pnpm typecheck` all pass.

## Current limitations

- `agent/recipes/icloud.ts` and `agent/recipes/outlook.ts` are currently
  stubs: `matches()` and `describe()` are real, but `read()` always resolves
  `{ count: null }` and `watch()` is a no-op. Both are pending `tasks.md`
  Task 2.8 (recording real signed-in/signed-out title observations from the
  live services first, in `docs/recipes-notes.md`, before implementing the
  title strategy and signed-out detection for either).
- The recipe panel described in [How `describe()` is used](#how-describe-is-used)
  (`tasks.md` Task 5.1) does not exist yet in this codebase.
- For the manual, on-real-account portion of adding or changing a recipe
  (confirming a live count on the actual service), follow the pattern in
  `docs/manual-checks.md` — it is the project's existing record of
  service-specific manual verification steps per milestone.
