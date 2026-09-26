import { describe, expect, it } from "vitest";
import { collectIconCandidates, isDisallowedIconHost } from "./icons";

const SERVICE_URL = new URL("https://mail.example.com/inbox");

function makeDoc(html: string): Document {
  return new DOMParser().parseFromString(html, "text/html");
}

describe("collectIconCandidates", () => {
  it("orders apple-touch-icon, then rel=icon by largest declared size, then favicon.ico", () => {
    const doc = makeDoc(`
      <html><head>
        <link rel="apple-touch-icon" href="/apple.png">
        <link rel="icon" href="/small.png" sizes="16x16">
        <link rel="icon" href="/large.png" sizes="192x192">
      </head><body></body></html>
    `);

    expect(collectIconCandidates(doc, SERVICE_URL)).toEqual([
      "https://mail.example.com/apple.png",
      "https://mail.example.com/large.png",
      "https://mail.example.com/small.png",
      "https://mail.example.com/favicon.ico",
    ]);
  });

  it("resolves relative hrefs to absolute URLs", () => {
    const doc = makeDoc(
      '<html><head><link rel="icon" href="icons/x.png"></head><body></body></html>',
    );

    expect(collectIconCandidates(doc, SERVICE_URL)).toContain(
      "https://mail.example.com/icons/x.png",
    );
  });

  it("drops non-http(s) URLs", () => {
    const doc = makeDoc(
      '<html><head><link rel="icon" href="data:image/png;base64,AAAA"></head><body></body></html>',
    );

    const candidates = collectIconCandidates(doc, SERVICE_URL);
    expect(candidates.some((url) => url.startsWith("data:"))).toBe(false);
    expect(candidates).toEqual(["https://mail.example.com/favicon.ico"]);
  });

  it("deduplicates repeated URLs", () => {
    const doc = makeDoc(`
      <html><head>
        <link rel="icon" href="/same.png">
        <link rel="icon" href="/same.png">
      </head><body></body></html>
    `);

    expect(collectIconCandidates(doc, SERVICE_URL)).toEqual([
      "https://mail.example.com/same.png",
      "https://mail.example.com/favicon.ico",
    ]);
  });

  it("caps the result at 8 candidates", () => {
    const links = Array.from(
      { length: 12 },
      (_, i) => `<link rel="icon" href="/icon-${i}.png">`,
    ).join("\n");
    const doc = makeDoc(`<html><head>${links}</head><body></body></html>`);

    expect(collectIconCandidates(doc, SERVICE_URL)).toHaveLength(8);
  });

  it("always includes /favicon.ico at the service origin", () => {
    const doc = makeDoc("<html><head></head><body></body></html>");

    expect(collectIconCandidates(doc, SERVICE_URL)).toEqual([
      "https://mail.example.com/favicon.ico",
    ]);
  });

  it("drops a disallowed candidate without spending one of the 8 slots", () => {
    const links = [
      '<link rel="icon" href="http://127.0.0.1/evil.png">',
      ...Array.from({ length: 8 }, (_, i) => `<link rel="icon" href="/icon-${i}.png">`),
    ].join("\n");
    const doc = makeDoc(`<html><head>${links}</head><body></body></html>`);

    const candidates = collectIconCandidates(doc, SERVICE_URL);
    expect(candidates.some((url) => url.includes("127.0.0.1"))).toBe(false);
    expect(candidates).toHaveLength(8);
    expect(candidates).toContain("https://mail.example.com/icon-7.png");
  });
});

describe("isDisallowedIconHost", () => {
  const allow = (url: string): boolean => isDisallowedIconHost(new URL(url));

  it("rejects loopback IPv4 (127.0.0.0/8)", () => {
    expect(allow("http://127.0.0.1/icon.png")).toBe(true);
    expect(allow("http://127.255.255.255/icon.png")).toBe(true);
  });

  it("rejects private IPv4 10.0.0.0/8", () => {
    expect(allow("http://10.0.0.5/icon.png")).toBe(true);
  });

  it("rejects private IPv4 172.16.0.0/12", () => {
    expect(allow("http://172.16.0.1/icon.png")).toBe(true);
    expect(allow("http://172.31.255.255/icon.png")).toBe(true);
    expect(allow("http://172.15.255.255/icon.png")).toBe(false);
    expect(allow("http://172.32.0.0/icon.png")).toBe(false);
  });

  it("rejects private IPv4 192.168.0.0/16", () => {
    expect(allow("http://192.168.1.1/icon.png")).toBe(true);
  });

  it("rejects link-local IPv4 169.254.0.0/16", () => {
    expect(allow("http://169.254.1.1/icon.png")).toBe(true);
  });

  it("rejects the unspecified IPv4 address 0.0.0.0", () => {
    expect(allow("http://0.0.0.0/icon.png")).toBe(true);
  });

  it("rejects loopback IPv6 (::1) from a bracketed URL host", () => {
    expect(allow("http://[::1]/icon.png")).toBe(true);
  });

  it("rejects link-local IPv6 fe80::/10", () => {
    expect(allow("http://[fe80::1]/icon.png")).toBe(true);
  });

  it("rejects unique-local IPv6 fc00::/7", () => {
    expect(allow("http://[fc00::1]/icon.png")).toBe(true);
    expect(allow("http://[fdff::1]/icon.png")).toBe(true);
  });

  it("rejects an IPv4-mapped IPv6 address whose mapped address is disallowed", () => {
    expect(allow("http://[::ffff:127.0.0.1]/icon.png")).toBe(true);
    expect(allow("http://[::ffff:10.1.2.3]/icon.png")).toBe(true);
  });

  it("accepts an IPv4-mapped IPv6 address whose mapped address is public", () => {
    expect(allow("http://[::ffff:93.184.216.34]/icon.png")).toBe(false);
  });

  it("rejects the localhost name", () => {
    expect(allow("http://localhost/icon.png")).toBe(true);
    expect(allow("http://localhost:8080/icon.png")).toBe(true);
  });

  it("accepts a public IPv4 address", () => {
    expect(allow("http://93.184.216.34/icon.png")).toBe(false);
  });

  it("accepts a domain name without resolving it", () => {
    expect(allow("https://mail.example.com/icon.png")).toBe(false);
  });
});
