// Collects candidate icon URLs from the current service page. The result is
// attached only to the first `report_unread` call after load (see
// agent/core/report.ts and design.md §2.2.14).

const MAX_ICON_CANDIDATES = 8;

function relTokens(link: Element): readonly string[] {
  return (link.getAttribute("rel") ?? "").toLowerCase().trim().split(/\s+/);
}

// Largest declared width from a `sizes` attribute such as "16x16 32x32".
// Malformed or missing sizes sort last (size 0), not first.
function largestDeclaredSize(link: Element): number {
  const sizesAttr = link.getAttribute("sizes");
  if (!sizesAttr) {
    return 0;
  }
  let largest = 0;
  for (const token of sizesAttr.trim().split(/\s+/)) {
    const [widthPart] = token.toLowerCase().split("x");
    const width = Number.parseInt(widthPart ?? "", 10);
    if (Number.isFinite(width) && width > largest) {
      largest = width;
    }
  }
  return largest;
}

function toAbsoluteHttpUrl(href: string, base: URL): string | null {
  try {
    const resolved = new URL(href, base);
    if (resolved.protocol !== "http:" && resolved.protocol !== "https:") {
      return null;
    }
    return resolved.href;
  } catch {
    return null;
  }
}

// Priority order: apple-touch-icon links (document order), then rel=icon
// links sorted by the largest declared `sizes` first, then the service
// origin's `/favicon.ico`. Every candidate is resolved to an absolute
// http(s) URL; non-http(s) URLs and duplicates are dropped, and the result
// is capped at 8 entries.
export function collectIconCandidates(document: Document, serviceUrl: URL): readonly string[] {
  const candidates: string[] = [];
  const seen = new Set<string>();

  const addCandidate = (href: string | null): void => {
    if (!href || candidates.length >= MAX_ICON_CANDIDATES) {
      return;
    }
    const absolute = toAbsoluteHttpUrl(href, serviceUrl);
    if (!absolute || seen.has(absolute)) {
      return;
    }
    seen.add(absolute);
    candidates.push(absolute);
  };

  const links = Array.from(document.querySelectorAll("link[rel]"));

  for (const link of links) {
    if (relTokens(link).includes("apple-touch-icon")) {
      addCandidate(link.getAttribute("href"));
    }
  }

  const iconLinks = links
    .filter((link) => relTokens(link).includes("icon"))
    .map((link) => ({ link, size: largestDeclaredSize(link) }))
    .sort((a, b) => b.size - a.size);
  for (const { link } of iconLinks) {
    addCandidate(link.getAttribute("href"));
  }

  addCandidate(new URL("/favicon.ico", serviceUrl).href);

  return candidates;
}
