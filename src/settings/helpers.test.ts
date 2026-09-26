import { describe, expect, it } from "vitest";
import {
  defaultProfileSelection,
  distinctNamedProfiles,
  isProfileSelectionValid,
  isValidProfileName,
  parseServiceUrl,
  profileSelectionToString,
  profileStringToSelection,
  suggestServiceDetails,
} from "./helpers";

describe("parseServiceUrl", () => {
  it("parses a valid https URL", () => {
    const url = parseServiceUrl("https://mail.google.com/mail/u/0/");
    expect(url).not.toBeNull();
    expect(url?.hostname).toBe("mail.google.com");
  });

  it("parses a valid http URL", () => {
    expect(parseServiceUrl("http://example.com/")).not.toBeNull();
  });

  it("returns null for unparseable input", () => {
    expect(parseServiceUrl("not a url")).toBeNull();
  });

  it("returns null for a non-http(s) scheme", () => {
    expect(parseServiceUrl("ftp://example.com/")).toBeNull();
    expect(parseServiceUrl("mailto:someone@example.com")).toBeNull();
  });
});

describe("suggestServiceDetails", () => {
  it("suggests the default profile for a Gmail URL", () => {
    const suggestion = suggestServiceDetails(new URL("https://mail.google.com/mail/u/0/"));
    expect(suggestion.profile).toEqual({ kind: "default", namedValue: "" });
    expect(suggestion.name).toBe("mail.google.com");
  });

  it("suggests the isolated profile for an iCloud /mail URL", () => {
    const suggestion = suggestServiceDetails(new URL("https://www.icloud.com/mail"));
    expect(suggestion.profile).toEqual({ kind: "isolated", namedValue: "" });
  });

  it("suggests the isolated profile for an Outlook URL", () => {
    const suggestion = suggestServiceDetails(new URL("https://outlook.live.com/mail/0/inbox"));
    expect(suggestion.profile).toEqual({ kind: "isolated", namedValue: "" });
  });

  it("suggests the isolated profile for a URL matching no specific recipe (generic)", () => {
    const suggestion = suggestServiceDetails(new URL("https://fastmail.example.com/"));
    expect(suggestion.profile).toEqual({ kind: "isolated", namedValue: "" });
  });
});

describe("profile selection conversions", () => {
  it("converts 'default' and 'isolated' profile strings to their kind, with no named value", () => {
    expect(profileStringToSelection("default")).toEqual({ kind: "default", namedValue: "" });
    expect(profileStringToSelection("isolated")).toEqual({ kind: "isolated", namedValue: "" });
  });

  it("converts any other profile string to a named selection", () => {
    expect(profileStringToSelection("work")).toEqual({ kind: "named", namedValue: "work" });
  });

  it("round-trips a selection back to its profile string", () => {
    expect(profileSelectionToString({ kind: "default", namedValue: "" })).toBe("default");
    expect(profileSelectionToString({ kind: "isolated", namedValue: "" })).toBe("isolated");
    expect(profileSelectionToString({ kind: "named", namedValue: "work" })).toBe("work");
  });

  it("defaultProfileSelection wraps a recipe's ProfileDefault with no named value", () => {
    expect(defaultProfileSelection("default")).toEqual({ kind: "default", namedValue: "" });
    expect(defaultProfileSelection("isolated")).toEqual({ kind: "isolated", namedValue: "" });
  });
});

describe("isValidProfileName / isProfileSelectionValid", () => {
  it("accepts lowercase alphanumeric-with-hyphen names up to 48 characters", () => {
    expect(isValidProfileName("work")).toBe(true);
    expect(isValidProfileName("gmail-work-2")).toBe(true);
    expect(isValidProfileName("a".repeat(48))).toBe(true);
  });

  it("rejects empty, too-long, or invalid-character names", () => {
    expect(isValidProfileName("")).toBe(false);
    expect(isValidProfileName("a".repeat(49))).toBe(false);
    expect(isValidProfileName("My Profile")).toBe(false);
    expect(isValidProfileName("Work")).toBe(false);
    expect(isValidProfileName("work_profile")).toBe(false);
  });

  it("default/isolated selections are always valid regardless of namedValue", () => {
    expect(isProfileSelectionValid({ kind: "default", namedValue: "" })).toBe(true);
    expect(isProfileSelectionValid({ kind: "isolated", namedValue: "anything" })).toBe(true);
  });

  it("a named selection is valid only when its namedValue is a valid profile name", () => {
    expect(isProfileSelectionValid({ kind: "named", namedValue: "work" })).toBe(true);
    expect(isProfileSelectionValid({ kind: "named", namedValue: "" })).toBe(false);
    expect(isProfileSelectionValid({ kind: "named", namedValue: "My Profile" })).toBe(false);
  });
});

describe("distinctNamedProfiles", () => {
  it("collects distinct non-default/isolated profiles, sorted", () => {
    const services = [
      { profile: "default" },
      { profile: "isolated" },
      { profile: "work" },
      { profile: "family" },
      { profile: "work" },
    ];
    expect(distinctNamedProfiles(services)).toEqual(["family", "work"]);
  });

  it("returns an empty list when every service uses default/isolated", () => {
    expect(distinctNamedProfiles([{ profile: "default" }, { profile: "isolated" }])).toEqual([]);
  });
});
