import { describe, expect, it } from "vitest";
import { collectIconCandidates } from "./icons";

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
});
