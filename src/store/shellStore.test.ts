import { describe, expect, it, vi } from "vitest";
import { createShellStore } from "./shellStore";
import { createMockShellIpc } from "../test/mockShellIpc";
import { service, snapshot } from "../test/fixtures";
import type { ShellIpc, Snapshot } from "../ipc";
import type { UnlistenFn } from "@tauri-apps/api/event";

interface Deferred<T> {
  readonly promise: Promise<T>;
  resolve(value: T): void;
  reject(reason: unknown): void;
}

function createDeferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

/** Returns a callback-ignoring stand-in for an `on*` subscriber that resolves
 * with each queued promise in turn, one per call — lets a test control the
 * timing of successive `bootstrap()` generations' registration calls. */
function onceSequence<T>(promises: readonly Promise<T>[]): () => Promise<T> {
  let index = 0;
  return () => {
    const next = promises[index];
    index += 1;
    if (next === undefined) {
      throw new Error("onceSequence: no more promises queued");
    }
    return next;
  };
}

describe("createShellStore", () => {
  it("starts loading, then becomes ready with the fetched snapshot", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);

    expect(store.getState()).toEqual({ status: "loading" });

    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("ready");
    });

    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.snapshot.services).toHaveLength(2);
    expect(state.selectedId).toBeNull();
  });

  it("initializes selectedId from snapshot.activeServiceId", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "icloud" }));
    const store = createShellStore(ipc);

    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
  });

  it("re-syncs selectedId to activeServiceId on every refetch, overriding the previous local selection", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("icloud");
    const afterSelect = store.getState();
    if (afterSelect.status !== "ready") throw new Error("expected ready state");
    expect(afterSelect.selectedId).toBe("icloud");

    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ activeServiceId: "gmail" }));
    ipc.emitServicesChanged();

    await vi.waitFor(() => {
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.selectedId).toBe("gmail");
    });
  });

  it("refetches the snapshot on a service-icon-changed event", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));
    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);

    ipc.getSnapshot.mockResolvedValueOnce(
      snapshot({ icons: { gmail: { path: "/tmp/eluma/data/icons/gmail.png", version: 1 } } }),
    );
    ipc.emitServiceIconChanged("gmail");

    await vi.waitFor(() => {
      expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.snapshot.icons.gmail?.version).toBe(1);
    });
  });

  it("is idempotent: a second start() does not refetch", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);

    store.start();
    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("ready");
    });

    expect(ipc.getSnapshot).toHaveBeenCalledTimes(1);
  });

  it("select() optimistically updates selectedId and calls selectService", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
    expect(ipc.selectService).toHaveBeenCalledWith("icloud");
  });

  it("updates selectedId when a select-service event fires", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.emitSelectService("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
  });

  it("refetches the snapshot when a services-changed event fires", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ services: [service({ id: "outlook" })] }));
    ipc.emitServicesChanged();

    await vi.waitFor(() => {
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.snapshot.services).toHaveLength(1);
    });
    expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
  });

  it("reorder() optimistically reorders, then refetches on failure", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.reorderServices.mockRejectedValueOnce(new Error("nope"));
    ipc.getSnapshot.mockResolvedValueOnce(snapshot());

    store.reorder(["icloud", "gmail"]);

    const optimistic = store.getState();
    if (optimistic.status !== "ready") throw new Error("expected ready state");
    expect(optimistic.snapshot.services.map((s) => s.id)).toEqual(["icloud", "gmail"]);

    await vi.waitFor(() => {
      expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
    });
  });

  it("select() and reorder() do nothing when the snapshot has a configError", async () => {
    const errorSnapshot = snapshot({
      services: [],
      settings: null,
      configError: { file: "/tmp/config.toml", key: null, reason: "invalid TOML" },
    });
    const ipc = createMockShellIpc(errorSnapshot);
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.select("gmail");
    store.reorder(["gmail"]);

    expect(ipc.selectService).not.toHaveBeenCalled();
    expect(ipc.reorderServices).not.toHaveBeenCalled();
  });

  it("applies a status-changed event into snapshot.statuses, keyed by serviceId", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.snapshot.statuses).toEqual({ gmail: { kind: "ok", count: 3 } });

    ipc.emitStatusChanged("icloud", { kind: "stale" });

    const nextState = store.getState();
    if (nextState.status !== "ready") throw new Error("expected ready state");
    expect(nextState.snapshot.statuses).toEqual({
      gmail: { kind: "ok", count: 3 },
      icloud: { kind: "stale" },
    });
  });

  it("stop() unsubscribes from events", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    store.stop();
    ipc.emitSelectService("icloud");

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBeNull();
  });

  it("becomes an error state when getSnapshot rejects", async () => {
    const ipc = createMockShellIpc(snapshot());
    ipc.getSnapshot.mockRejectedValueOnce(new Error("boom"));
    const store = createShellStore(ipc);

    store.start();
    await vi.waitFor(() => {
      expect(store.getState().status).toBe("error");
    });

    const state = store.getState();
    if (state.status !== "error") throw new Error("expected error state");
    expect(state.message).toBe("boom");
  });

  it("select() logs and refetches when selectService fails, restoring selectedId from the backend", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    ipc.selectService.mockRejectedValueOnce(new Error("nope"));
    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ activeServiceId: "gmail" }));

    store.select("icloud");
    const optimistic = store.getState();
    if (optimistic.status !== "ready") throw new Error("expected ready state");
    expect(optimistic.selectedId).toBe("icloud");

    await vi.waitFor(() => {
      expect(ipc.getSnapshot).toHaveBeenCalledTimes(2);
    });
    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("gmail");
    expect(consoleErrorSpy).toHaveBeenCalled();
    consoleErrorSpy.mockRestore();
  });

  it("stop() before bootstrap resolves leaves no listeners registered", async () => {
    const servicesUnlisten = vi.fn();
    const selectUnlisten = vi.fn();
    const servicesDeferred = createDeferred<UnlistenFn>();
    const selectDeferred = createDeferred<UnlistenFn>();

    const ipc: ShellIpc = {
      getSnapshot: () => Promise.resolve(snapshot()),
      selectService: () => Promise.resolve(),
      reorderServices: () => Promise.resolve(),
      openSettings: () => Promise.resolve(),
      onServicesChanged: () => servicesDeferred.promise,
      onSelectService: () => selectDeferred.promise,
      onStatusChanged: () => Promise.resolve(() => {}),
      onServiceIconChanged: () => Promise.resolve(() => {}),
    };
    const store = createShellStore(ipc);

    store.start();
    store.stop();

    servicesDeferred.resolve(servicesUnlisten);
    selectDeferred.resolve(selectUnlisten);

    await vi.waitFor(() => {
      expect(servicesUnlisten).toHaveBeenCalledTimes(1);
      expect(selectUnlisten).toHaveBeenCalledTimes(1);
    });
    // bootstrap bailed out before ever calling refresh()
    expect(store.getState()).toEqual({ status: "loading" });
  });

  it("start → stop → start keeps only the latest generation's listeners", async () => {
    const unlistenA1 = vi.fn();
    const unlistenA2 = vi.fn();
    const unlistenB1 = vi.fn();
    const unlistenB2 = vi.fn();

    const deferredA1 = createDeferred<UnlistenFn>();
    const deferredA2 = createDeferred<UnlistenFn>();
    const deferredB1 = createDeferred<UnlistenFn>();
    const deferredB2 = createDeferred<UnlistenFn>();

    const ipc: ShellIpc = {
      getSnapshot: () => Promise.resolve(snapshot()),
      selectService: () => Promise.resolve(),
      reorderServices: () => Promise.resolve(),
      openSettings: () => Promise.resolve(),
      onServicesChanged: onceSequence([deferredA1.promise, deferredB1.promise]),
      onSelectService: onceSequence([deferredA2.promise, deferredB2.promise]),
      onStatusChanged: () => Promise.resolve(() => {}),
      onServiceIconChanged: () => Promise.resolve(() => {}),
    };
    const store = createShellStore(ipc);

    store.start(); // generation 1
    store.stop();
    store.start(); // generation 2 (or later) — the only one that should stick

    // Resolve the stale (generation-1) registrations first.
    deferredA1.resolve(unlistenA1);
    deferredA2.resolve(unlistenA2);
    // Then the current generation's.
    deferredB1.resolve(unlistenB1);
    deferredB2.resolve(unlistenB2);

    await vi.waitFor(() => {
      expect(store.getState().status).toBe("ready");
    });

    // The stale generation's registrations were unlistened as soon as they
    // resolved, and never wired into the store.
    expect(unlistenA1).toHaveBeenCalledTimes(1);
    expect(unlistenA2).toHaveBeenCalledTimes(1);
    expect(unlistenB1).not.toHaveBeenCalled();
    expect(unlistenB2).not.toHaveBeenCalled();

    // stop() only unlistens the current generation's registrations.
    store.stop();
    expect(unlistenB1).toHaveBeenCalledTimes(1);
    expect(unlistenB2).toHaveBeenCalledTimes(1);
  });

  it("bootstrap: one registration rejecting unlistens the others and sets error state", async () => {
    const unlistenServices = vi.fn();
    const getSnapshotSpy = vi.fn(() => Promise.resolve(snapshot()));

    const ipc: ShellIpc = {
      getSnapshot: getSnapshotSpy,
      selectService: () => Promise.resolve(),
      reorderServices: () => Promise.resolve(),
      openSettings: () => Promise.resolve(),
      onServicesChanged: () => Promise.resolve(unlistenServices),
      onSelectService: () => Promise.reject(new Error("registration failed")),
      onStatusChanged: () => Promise.resolve(() => {}),
      onServiceIconChanged: () => Promise.resolve(() => {}),
    };
    const store = createShellStore(ipc);

    store.start();

    await vi.waitFor(() => {
      expect(store.getState().status).toBe("error");
    });
    const state = store.getState();
    if (state.status !== "error") throw new Error("expected error state");
    expect(state.message).toBe("registration failed");
    expect(unlistenServices).toHaveBeenCalledTimes(1);
    expect(getSnapshotSpy).not.toHaveBeenCalled();
  });

  it("ignores a refresh that resolves after stop()", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const deferredSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(deferredSnapshot.promise);
    ipc.emitServicesChanged(); // starts a refresh() that awaits our deferred

    store.stop();

    deferredSnapshot.resolve(
      snapshot({ activeServiceId: "icloud", services: [service({ id: "outlook" })] }),
    );
    // The store's own continuation was attached to this promise before this
    // line runs, so by the time this await resumes, refresh() has already
    // decided (and, per this test, discarded) its result.
    await deferredSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    // Unchanged from what bootstrap's own refresh applied before stop() —
    // not the snapshot the stale, post-stop refresh resolved with.
    expect(state.selectedId).toBe("gmail");
    expect(state.snapshot.services).toHaveLength(2);
  });

  it("keeps the newer result when two overlapping refreshes resolve out of order", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const olderFetch = createDeferred<Snapshot>();
    const newerFetch = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(olderFetch.promise);
    ipc.getSnapshot.mockReturnValueOnce(newerFetch.promise);

    ipc.emitServicesChanged(); // starts the older refresh
    ipc.emitServicesChanged(); // starts the newer refresh

    // The newer request settles first...
    newerFetch.resolve(snapshot({ activeServiceId: "icloud" }));
    await newerFetch.promise;

    const afterNewer = store.getState();
    if (afterNewer.status !== "ready") throw new Error("expected ready state");
    expect(afterNewer.selectedId).toBe("icloud");

    // ...and the older one settling afterwards must not overwrite it.
    olderFetch.resolve(snapshot({ activeServiceId: "outlook" }));
    await olderFetch.promise;

    const afterOlder = store.getState();
    if (afterOlder.status !== "ready") throw new Error("expected ready state");
    expect(afterOlder.selectedId).toBe("icloud");
  });

  it("a stale generation's select() failure can't invalidate the current generation's in-flight refresh", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    // generation 1: select() fails, but its IPC call stays pending for now.
    const selectServiceFailure = createDeferred<void>();
    ipc.selectService.mockReturnValueOnce(selectServiceFailure.promise);
    store.select("icloud");

    // Stop and restart before that call settles — a new generation, with
    // its own in-flight refresh (blocked on `restartSnapshot`).
    store.stop();
    const restartSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(restartSnapshot.promise);
    store.start();
    await vi.waitFor(() => expect(ipc.getSnapshot).toHaveBeenCalledTimes(2));

    // Now let the stale (generation-1) select() failure settle. Its
    // refresh() call must bail out (wrong generation) without bumping the
    // refresh sequence counter, or it would wrongly invalidate the current
    // generation's already in-flight refresh below.
    const consoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    selectServiceFailure.reject(new Error("selectService failed"));
    await selectServiceFailure.promise.catch(() => {});
    consoleErrorSpy.mockRestore();

    // The current generation's refresh settles afterwards and must still
    // be applied.
    restartSnapshot.resolve(snapshot({ activeServiceId: "outlook" }));
    await restartSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("outlook");
  });

  it("keeps a newer select-service choice over a pending refresh's older snapshot", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const pendingSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(pendingSnapshot.promise);
    ipc.emitServicesChanged(); // starts a refresh() whose getSnapshot is now pending

    // A select-service event arrives from the backend while that refresh
    // is still in flight.
    ipc.emitSelectService("icloud");
    const midFlight = store.getState();
    if (midFlight.status !== "ready") throw new Error("expected ready state");
    expect(midFlight.selectedId).toBe("icloud");

    // The pending refresh's snapshot resolves with an *older* activeServiceId.
    pendingSnapshot.resolve(
      snapshot({ activeServiceId: "gmail", services: [service({ id: "outlook" })] }),
    );
    await pendingSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    // The newer select-service choice survives the older snapshot...
    expect(state.selectedId).toBe("icloud");
    // ...while the rest of that snapshot is still applied.
    expect(state.snapshot.services).toHaveLength(1);
  });

  it("keeps a newer status-changed event over a pending refresh's older statuses", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    const pendingSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(pendingSnapshot.promise);
    ipc.emitServicesChanged(); // starts a refresh() whose getSnapshot is now pending

    // A status-changed event arrives from the backend while that refresh
    // is still in flight.
    ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });
    const midFlight = store.getState();
    if (midFlight.status !== "ready") throw new Error("expected ready state");
    expect(midFlight.snapshot.statuses).toEqual({ gmail: { kind: "ok", count: 3 } });

    // The pending refresh's snapshot resolves with no statuses at all — the
    // backend's own read predates the event above.
    pendingSnapshot.resolve(snapshot({ services: [service({ id: "outlook" })] }));
    await pendingSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    // The status-changed event survives the older snapshot...
    expect(state.snapshot.statuses).toEqual({ gmail: { kind: "ok", count: 3 } });
    // ...while the rest of that snapshot is still applied.
    expect(state.snapshot.services).toHaveLength(1);
  });

  it("keeps a status-changed event that arrived before the store's first snapshot", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);

    const initialSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(initialSnapshot.promise);

    store.start();
    await vi.waitFor(() => expect(ipc.getSnapshot).toHaveBeenCalledTimes(1));
    expect(store.getState()).toEqual({ status: "loading" });

    // A status-changed event arrives before the store's very first snapshot
    // — `state` is still "loading", so there is no `snapshot.statuses` to
    // apply it to directly yet.
    ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });
    expect(store.getState()).toEqual({ status: "loading" });

    initialSnapshot.resolve(snapshot());
    await initialSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.snapshot.statuses).toEqual({ gmail: { kind: "ok", count: 3 } });
  });

  it("does not keep a stale status-changed event across a later refresh", async () => {
    const ipc = createMockShellIpc(snapshot());
    const store = createShellStore(ipc);
    store.start();
    await vi.waitFor(() => expect(store.getState().status).toBe("ready"));

    ipc.emitStatusChanged("gmail", { kind: "ok", count: 3 });
    const afterEvent = store.getState();
    if (afterEvent.status !== "ready") throw new Error("expected ready state");
    expect(afterEvent.snapshot.statuses).toEqual({ gmail: { kind: "ok", count: 3 } });

    // A later refresh's own snapshot now reports a different status for the
    // same service — that must win, not the earlier (already-applied) event.
    ipc.getSnapshot.mockResolvedValueOnce(snapshot({ statuses: { gmail: { kind: "stale" } } }));
    ipc.emitServicesChanged();

    await vi.waitFor(() => {
      const state = store.getState();
      if (state.status !== "ready") throw new Error("expected ready state");
      expect(state.snapshot.statuses).toEqual({ gmail: { kind: "stale" } });
    });
  });

  it("keeps a select-service choice that arrived before the store's first snapshot", async () => {
    const ipc = createMockShellIpc(snapshot({ activeServiceId: "gmail" }));
    const store = createShellStore(ipc);

    const initialSnapshot = createDeferred<Snapshot>();
    ipc.getSnapshot.mockReturnValueOnce(initialSnapshot.promise);

    store.start();
    await vi.waitFor(() => expect(ipc.getSnapshot).toHaveBeenCalledTimes(1));
    expect(store.getState()).toEqual({ status: "loading" });

    // A select-service event arrives before the store's very first snapshot
    // — `state` is still "loading", so there is no `selectedId` field to
    // stash this in yet.
    ipc.emitSelectService("icloud");
    expect(store.getState()).toEqual({ status: "loading" });

    initialSnapshot.resolve(snapshot({ activeServiceId: "gmail" }));
    await initialSnapshot.promise;

    const state = store.getState();
    if (state.status !== "ready") throw new Error("expected ready state");
    expect(state.selectedId).toBe("icloud");
  });
});
