// The Notification stub (design.md §2.2.14, Task 4.3): replaces
// `window.Notification` before any page script runs, so a service page can
// never display a page-originated OS notification. `permission` is always
// `'denied'`; `requestPermission()` always resolves — and, for the legacy
// callback form, always invokes the callback with — `'denied'`; and
// constructing an instance always throws.
//
// No TS `class` (project rule): a plain `function` value is used instead,
// since only a real `function` declaration — not an arrow function, and
// not a plain object literal — has a JS engine `[[Construct]]` slot, so it
// is the only kind of value a page's `new Notification(...)` can actually
// invoke (reaching our throw) rather than the engine itself rejecting the
// call as "not a constructor".

const DENIED_PERMISSION: NotificationPermission = "denied";

// The stub's own minimal shape. Deliberately not `typeof Notification`
// (the real DOM constructor's type): that type also requires `prototype`
// and `maxActions` fields plus a `new (...): Notification` construct
// signature, which a plain function value's *static* TS type never has
// (confirmed empirically — TypeScript does not infer a construct
// signature for an ordinary `function`), so satisfying it would need an
// `as unknown as …` cast, which the project's "no `any`/`unknown`" rule
// forbids. Nothing in this codebase ever does `new window.Notification(...)`
// or reads `Notification.prototype` itself — only arbitrary page scripts
// do, at the JS level, where the stub's real runtime `function`-ness (not
// its static TS type here) is what makes `new` reach the throw.
export interface NotificationConstructorLike {
  readonly permission: NotificationPermission;
  requestPermission(
    deprecatedCallback?: NotificationPermissionCallback,
  ): Promise<NotificationPermission>;
}

// The window slice `installNotificationStub` needs. Structurally satisfied
// by the real `Window` (whose `Notification` is `typeof Notification`, a
// superset of `NotificationConstructorLike`) and by `main.ts`'s own
// `AgentWindow`, so callers pass either with no cast.
export interface NotificationStubWindow {
  Notification?: NotificationConstructorLike;
}

function throwNotConstructible(): never {
  throw new TypeError(
    "Notification is disabled in Eluma service webviews; page-originated notifications are not supported",
  );
}

// Returns a fresh real `function` declaration (not an arrow function, and
// not a single module-level shared function) each time it is called: only
// a real `function` has a JS engine `[[Construct]]` slot, so
// `new Notification(title, options)` reaches this body (and throws)
// instead of the engine rejecting the call outright — calling it without
// `new` throws the same way — and a *fresh* one is needed per stub because
// `createNotificationStub` below tightens `permission` into a
// non-writable, non-configurable own property on it, which a second call
// reusing the same function object could no longer overwrite. Declared
// with no parameters — `title`/`options` are never read (the body always
// throws), and omitting them avoids unused-parameter names; JS does not
// enforce arity, so a caller passing `(title, options)` is unaffected.
function makeNotificationConstructor(): () => never {
  return function NotificationStubConstructor(): never {
    return throwNotConstructible();
  };
}

function requestPermission(
  deprecatedCallback?: NotificationPermissionCallback,
): Promise<NotificationPermission> {
  // Legacy callback form (pre-Promise API): invoked with the same fixed
  // result the returned Promise resolves with.
  if (typeof deprecatedCallback === "function") {
    deprecatedCallback(DENIED_PERMISSION);
  }
  return Promise.resolve(DENIED_PERMISSION);
}

// Builds one stub instance. A fresh function per call (no module-level
// singleton), so tests can install an independent stub into an isolated
// fake window without sharing mutable state across them.
function createNotificationStub(): NotificationConstructorLike {
  const stub: NotificationConstructorLike = Object.assign(makeNotificationConstructor(), {
    permission: DENIED_PERMISSION,
    requestPermission,
  });

  // Re-tightened after `Object.assign` (which only ever produces ordinary
  // writable/configurable data properties): nothing that runs after this
  // can reassign `permission`/`requestPermission` to anything but the
  // fixed denied behavior.
  Object.defineProperty(stub, "permission", {
    value: DENIED_PERMISSION,
    writable: false,
    configurable: false,
    enumerable: true,
  });
  Object.defineProperty(stub, "requestPermission", {
    value: requestPermission,
    writable: false,
    configurable: false,
    enumerable: true,
  });

  return stub;
}

// Installs the stub as `win.Notification` (design.md §2.2.14, Task 4.3's
// entry step 3: "Install the Notification stub" — run right after reading
// `window.__ELUMA__`, before the origin check, and before any page script
// runs). Uses `Object.defineProperty`, not a plain assignment, so the
// binding itself is also non-configurable/non-writable: a page script
// loaded after this cannot restore or replace `window.Notification`.
export function installNotificationStub(win: NotificationStubWindow): void {
  Object.defineProperty(win, "Notification", {
    value: createNotificationStub(),
    writable: false,
    configurable: false,
    enumerable: true,
  });
}
