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

function toAbsoluteHttpUrl(href: string, base: URL): URL | null {
  try {
    const resolved = new URL(href, base);
    if (resolved.protocol !== "http:" && resolved.protocol !== "https:") {
      return null;
    }
    return resolved;
  } catch {
    return null;
  }
}

function parseIpv4(hostname: string): readonly [number, number, number, number] | null {
  const match = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(hostname);
  if (!match) {
    return null;
  }
  const a = Number.parseInt(match[1] ?? "", 10);
  const b = Number.parseInt(match[2] ?? "", 10);
  const c = Number.parseInt(match[3] ?? "", 10);
  const d = Number.parseInt(match[4] ?? "", 10);
  if (a > 255 || b > 255 || c > 255 || d > 255) {
    return null;
  }
  return [a, b, c, d];
}

// Matches src-tauri/src/agent_bridge/validate.rs's is_disallowed_ipv4, plus
// the unspecified address (0.0.0.0), which the receiver does not check but
// which must never be sent as a candidate either.
function isDisallowedIpv4([a, b, c, d]: readonly [number, number, number, number]): boolean {
  if (a === 127) {
    return true; // 127.0.0.0/8, loopback
  }
  if (a === 10) {
    return true; // 10.0.0.0/8, private
  }
  if (a === 172 && b >= 16 && b <= 31) {
    return true; // 172.16.0.0/12, private
  }
  if (a === 192 && b === 168) {
    return true; // 192.168.0.0/16, private
  }
  if (a === 169 && b === 254) {
    return true; // 169.254.0.0/16, link-local
  }
  if (a === 0 && b === 0 && c === 0 && d === 0) {
    return true; // 0.0.0.0, unspecified
  }
  return false;
}

type Ipv6Segments = readonly [number, number, number, number, number, number, number, number];

// Expands a bracket-stripped IPv6 literal (already canonicalized by the URL
// parser, e.g. "::1" or "::ffff:7f00:1") into its 8 16-bit segments, or
// `null` if it is not a well-formed IPv6 address.
function parseIpv6Segments(hostname: string): Ipv6Segments | null {
  const halves = hostname.split("::");
  if (halves.length > 2) {
    return null;
  }
  const head = halves[0] ? halves[0].split(":") : [];
  const tail = halves.length === 2 && halves[1] ? halves[1].split(":") : [];
  const missing = halves.length === 2 ? 8 - head.length - tail.length : 0;
  if (missing < 0 || (halves.length === 1 && head.length !== 8)) {
    return null;
  }
  const groups = [...head, ...Array.from({ length: missing }, () => "0"), ...tail];
  if (groups.length !== 8) {
    return null;
  }
  const segments = groups.map((group) => Number.parseInt(group, 16));
  if (segments.some((segment) => !Number.isInteger(segment) || segment < 0 || segment > 0xffff)) {
    return null;
  }
  const [s0, s1, s2, s3, s4, s5, s6, s7] = segments;
  return [s0 ?? 0, s1 ?? 0, s2 ?? 0, s3 ?? 0, s4 ?? 0, s5 ?? 0, s6 ?? 0, s7 ?? 0];
}

// Matches src-tauri/src/agent_bridge/validate.rs's is_disallowed_ipv6:
// loopback (::1), IPv4-mapped (::ffff:a.b.c.d, re-checked as IPv4),
// unicast link-local (fe80::/10) and unique local (fc00::/7).
function isDisallowedIpv6(segments: Ipv6Segments): boolean {
  if (segments.slice(0, 7).every((segment) => segment === 0) && segments[7] === 1) {
    return true;
  }
  if (segments.slice(0, 5).every((segment) => segment === 0) && segments[5] === 0xffff) {
    const mapped: [number, number, number, number] = [
      segments[6] >> 8,
      segments[6] & 0xff,
      segments[7] >> 8,
      segments[7] & 0xff,
    ];
    return isDisallowedIpv4(mapped);
  }
  if ((segments[0] & 0xffc0) === 0xfe80) {
    return true; // fe80::/10, unicast link-local
  }
  if ((segments[0] & 0xfe00) === 0xfc00) {
    return true; // fc00::/7, unique local
  }
  return false;
}

// Whether `url`'s host is loopback, private or link-local and must never be
// sent as an icon candidate — the receiver
// (src-tauri/src/agent_bridge/validate.rs's `is_disallowed_host`) rejects
// the entire report if any candidate fails this check, so the agent must
// filter these out itself. Also rejects the `localhost` name, which the
// receiver's DNS-free check cannot catch since it only inspects literal IP
// hosts.
export function isDisallowedIconHost(url: URL): boolean {
  const hostname = url.hostname;
  if (hostname === "localhost") {
    return true;
  }
  if (hostname.startsWith("[") && hostname.endsWith("]")) {
    const segments = parseIpv6Segments(hostname.slice(1, -1));
    return segments !== null && isDisallowedIpv6(segments);
  }
  const ipv4 = parseIpv4(hostname);
  return ipv4 !== null && isDisallowedIpv4(ipv4);
}

// Priority order: apple-touch-icon links (document order), then rel=icon
// links sorted by the largest declared `sizes` first, then the service
// origin's `/favicon.ico`. Every candidate is resolved to an absolute
// http(s) URL; non-http(s) URLs and candidates with a disallowed host are
// dropped before deduplication and the 8-entry cap, so neither consumes a
// slot.
export function collectIconCandidates(document: Document, serviceUrl: URL): readonly string[] {
  const candidates: string[] = [];
  const seen = new Set<string>();

  const addCandidate = (href: string | null): void => {
    if (!href || candidates.length >= MAX_ICON_CANDIDATES) {
      return;
    }
    const resolved = toAbsoluteHttpUrl(href, serviceUrl);
    if (!resolved || isDisallowedIconHost(resolved)) {
      return;
    }
    const absolute = resolved.href;
    if (seen.has(absolute)) {
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
