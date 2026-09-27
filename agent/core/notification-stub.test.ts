import { describe, expect, it, vi } from "vitest";
import type { NotificationConstructorLike, NotificationStubWindow } from "./notification-stub";
import { installNotificationStub } from "./notification-stub";

function makeWindow(): NotificationStubWindow {
  return {};
}

// Reads the installed stub back off the window, failing loudly (rather
// than falling back to some default) if installation somehow left it
// unset — mirrors the project's "no fallback defaults" rule.
function installedStub(win: NotificationStubWindow): NotificationConstructorLike {
  const stub = win.Notification;
  if (!stub) {
    throw new Error("installNotificationStub did not set win.Notification");
  }
  return stub;
}

describe("installNotificationStub", () => {
  it("sets permission to 'denied'", () => {
    const win = makeWindow();
    installNotificationStub(win);
    expect(installedStub(win).permission).toBe("denied");
  });

  it("resolves requestPermission() to 'denied'", async () => {
    const win = makeWindow();
    installNotificationStub(win);
    await expect(installedStub(win).requestPermission()).resolves.toBe("denied");
  });

  it("invokes the legacy requestPermission(callback) form with 'denied', and still resolves the promise", async () => {
    const win = makeWindow();
    installNotificationStub(win);

    const received: string[] = [];
    const promise = installedStub(win).requestPermission((permission) => {
      received.push(permission);
    });

    // Called asynchronously, not during requestPermission() itself.
    expect(received).toEqual([]);
    await expect(promise).resolves.toBe("denied");
    await Promise.resolve();
    expect(received).toEqual(["denied"]);
  });

  it("still resolves to 'denied' when the legacy callback throws", async () => {
    const win = makeWindow();
    installNotificationStub(win);
    const errorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const promise = installedStub(win).requestPermission(() => {
      throw new Error("page callback failed");
    });

    await expect(promise).resolves.toBe("denied");
    await Promise.resolve();
    expect(errorSpy).toHaveBeenCalled();
    errorSpy.mockRestore();
  });

  it("throws when the stub is invoked as a constructor", () => {
    const win = makeWindow();
    installNotificationStub(win);
    const stub = installedStub(win);

    // `NotificationConstructorLike` deliberately carries no construct
    // signature (see notification-stub.ts's doc comment: giving it one
    // would need an `as unknown as …` cast in the *production* code,
    // which the project's "no any/unknown" rule forbids). This one bridge
    // through `unknown`, confined to this test, is what is needed to
    // exercise the stub's real, runtime-only `[[Construct]]` behavior — a
    // page script calls `new Notification(...)` on exactly this kind of
    // value, and this must throw the way the real DOM constructor would
    // reject `new` on invalid input.
    const Constructable = stub as unknown as new (title: string) => object;
    expect(() => new Constructable("hello")).toThrow(TypeError);
  });

  it("throws when the stub is invoked as a plain function (no 'new')", () => {
    const win = makeWindow();
    installNotificationStub(win);
    const stub = installedStub(win);

    const callable = stub as unknown as (title: string) => unknown;
    expect(() => callable("hello")).toThrow(TypeError);
  });

  it("installs a non-configurable, non-writable window.Notification binding", () => {
    const win = makeWindow();
    installNotificationStub(win);

    const descriptor = Object.getOwnPropertyDescriptor(win, "Notification");
    expect(descriptor?.writable).toBe(false);
    expect(descriptor?.configurable).toBe(false);

    // This file is an ES module, so it (like the real agent bundle) already
    // runs in strict mode, where `delete` on a non-configurable own
    // property throws instead of silently failing.
    expect(() => {
      delete win.Notification;
    }).toThrow(TypeError);
  });

  it("is installed before a simulated page script runs, so the page never observes the real API", () => {
    const win = makeWindow();

    // Simulates the design's ordering guarantee (design.md §2.2.14: the
    // stub is installed "before any page script runs" — the init script
    // runs before page scripts): install first, exactly like main.ts does
    // at its marked step, then run what stands in for a page script.
    installNotificationStub(win);

    function pageScript(pageWindow: NotificationStubWindow): NotificationPermission {
      const stub = installedStub(pageWindow);
      return stub.permission;
    }

    expect(pageScript(win)).toBe("denied");
  });
});
