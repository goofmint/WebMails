/**
 * `createShellStore`: a small `useSyncExternalStore`-compatible store fed
 * by `ShellIpc` (Task 1.10; design.md §2.2.13's "React context plus
 * `useSyncExternalStore` over a small store... No state library.").
 *
 * State is one of three immutable, tagged variants; every transition
 * produces a brand new object (never a mutation), so `getState()` results
 * are safe to compare with `===`.
 */

import type { ShellIpc, ServiceConfig, Snapshot } from "../ipc";
import type { UnlistenFn } from "@tauri-apps/api/event";

export type ShellState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly snapshot: Snapshot; readonly selectedId: string | null }
  | { readonly status: "error"; readonly message: string };

export interface ShellStore {
  getState(): ShellState;
  subscribe(listener: () => void): () => void;
  /** Idempotent: a second call while already started does nothing. */
  start(): void;
  /** Unsubscribes from every backend event. Safe to call more than once. */
  stop(): void;
  select(id: string): void;
  reorder(ids: readonly string[]): void;
  openSettings(): void;
}

function errorMessage(caughtError: unknown): string {
  return caughtError instanceof Error ? caughtError.message : String(caughtError);
}

/** `services` reordered to match `ids`, dropping any id `services` has no entry for. */
function reorderServicesByIds(
  services: readonly ServiceConfig[],
  ids: readonly string[],
): readonly ServiceConfig[] {
  const byId = new Map(services.map((service) => [service.id, service] as const));
  const reordered: ServiceConfig[] = [];
  for (const id of ids) {
    const service = byId.get(id);
    if (service !== undefined) {
      reordered.push(service);
    }
  }
  return reordered;
}

export function createShellStore(ipc: ShellIpc): ShellStore {
  let state: ShellState = { status: "loading" };
  const listeners = new Set<() => void>();
  let unlistenFns: (() => void)[] = [];
  let started = false;
  // Bumped by every start()/stop(), and captured by each bootstrap() call —
  // lets a bootstrap that is still in flight when the store is stopped (or
  // stopped and restarted before it resolves) recognize it's stale once its
  // subscriptions come back, instead of clobbering a later generation's
  // `unlistenFns` or leaking listeners the store no longer owns.
  let generation = 0;
  // Bumped by every refresh() call and compared against the call's own
  // captured value once its fetch settles — lets two refreshes started in
  // the same generation (e.g. a services-changed event firing again before
  // the first refetch lands) apply only the most recently *started* one's
  // result, regardless of which one's promise happens to settle first.
  let latestRefreshSeq = 0;
  // Bumped by every selectedId-choosing event — a user select() and the
  // backend's own `select-service` event — and captured by refresh() when
  // it starts. If this has moved on by the time a pending refresh's
  // snapshot comes back, that snapshot's `activeServiceId` is older than
  // the selection the user/backend has since made, so refresh() keeps
  // `latestSelectedId` instead of clobbering it.
  let selectionSeq = 0;
  // The id from the most recent selectedId-choosing event, tracked
  // independently of `state` — a `select-service` event (or select())
  // can arrive while `state` is still "loading" or "error", when there is
  // no `selectedId` field to stash it in, and it must still survive until
  // the snapshot that follows.
  let latestSelectedId: string | null = null;

  function getState(): ShellState {
    return state;
  }

  function subscribe(listener: () => void): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  function setState(next: ShellState): void {
    state = next;
    for (const listener of listeners) {
      listener();
    }
  }

  // True once some *other* refresh() call has become current since `seq`
  // was captured, or the generation `gen` belonged to is no longer this
  // store's — either way, this call's result must not be applied.
  function isStaleRefresh(gen: number, seq: number): boolean {
    return gen !== generation || !started || seq !== latestRefreshSeq;
  }

  /**
   * Fetches the snapshot and applies it, but only if — once the fetch
   * settles — this call is still both the caller's generation's (`gen`,
   * captured by the caller before any `await`) and the most recently
   * started refresh overall (`seq`). Every caller (bootstrap, the
   * services-changed listener, and select()/reorder()'s failure paths)
   * passes the generation it observed at its own call site, not whatever
   * `generation` happens to hold once this settles.
   */
  async function refresh(gen: number): Promise<void> {
    if (gen !== generation || !started) {
      // Already stale at call time — bail before touching `latestRefreshSeq`
      // at all. A stale-generation call that *did* take a seq here would
      // still correctly no-op itself below, but the seq it took would
      // become the new "latest", wrongly invalidating the current
      // generation's own in-flight refresh once that one's result comes
      // back and finds its (older, but legitimate) seq no longer current.
      return;
    }
    const seq = ++latestRefreshSeq;
    // Snapshotted so that, once the fetch below resolves, we can tell
    // whether a newer selection (select() or a `select-service` event) has
    // happened in the meantime and, if so, keep it instead of applying this
    // now-stale response's `activeServiceId`.
    const selectionSeqAtStart = selectionSeq;
    try {
      const snapshot = await ipc.getSnapshot();
      if (isStaleRefresh(gen, seq)) {
        return;
      }
      // The backend is authoritative for which service is active
      // (`snapshot.activeServiceId`), so a refetch normally re-syncs
      // `selectedId` to it — unless a newer selection has happened while
      // this fetch was pending, in which case that selection wins (even if
      // `state` was still "loading"/"error" when it arrived, so there was
      // no `state.selectedId` to read it back from) and only the rest of
      // the snapshot (services/statuses/etc.) is applied.
      const keepNewerSelection = selectionSeq !== selectionSeqAtStart;
      const selectedId = keepNewerSelection ? latestSelectedId : snapshot.activeServiceId;
      setState({ status: "ready", snapshot, selectedId });
    } catch (caughtError) {
      if (isStaleRefresh(gen, seq)) {
        return;
      }
      setState({ status: "error", message: errorMessage(caughtError) });
    }
  }

  async function bootstrap(gen: number): Promise<void> {
    const results = await Promise.allSettled([
      ipc.onServicesChanged(() => {
        void refresh(gen);
      }),
      ipc.onSelectService(({ id }) => {
        selectionSeq += 1;
        latestSelectedId = id;
        if (state.status === "ready") {
          setState({ ...state, selectedId: id });
        }
      }),
    ]);

    // Retain every registration that succeeded even if another one
    // rejected, so a partial failure below can still unlisten them instead
    // of leaking a listener the backend registered but this store lost
    // track of.
    const fulfilledUnlistens: UnlistenFn[] = [];
    let rejectedError: unknown;
    let hasRejection = false;
    for (const result of results) {
      if (result.status === "fulfilled") {
        fulfilledUnlistens.push(result.value);
      } else if (!hasRejection) {
        hasRejection = true;
        rejectedError = result.reason;
      }
    }

    if (gen !== generation || !started) {
      // A newer generation has started (or the store was stopped) since
      // this bootstrap began — unlisten what it just registered and bail
      // rather than assigning `unlistenFns` for a generation the store no
      // longer owns.
      for (const unlisten of fulfilledUnlistens) {
        unlisten();
      }
      return;
    }

    if (hasRejection) {
      for (const unlisten of fulfilledUnlistens) {
        unlisten();
      }
      setState({ status: "error", message: errorMessage(rejectedError) });
      return;
    }

    unlistenFns = fulfilledUnlistens;
    await refresh(gen);
  }

  function start(): void {
    if (started) {
      return;
    }
    started = true;
    generation += 1;
    void bootstrap(generation);
  }

  function stop(): void {
    if (!started) {
      return;
    }
    started = false;
    generation += 1;
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns = [];
  }

  function select(id: string): void {
    if (state.status !== "ready" || state.snapshot.configError) {
      return;
    }
    // Captured synchronously, before `ipc.selectService`'s promise settles —
    // if the store is stopped (and maybe restarted) in the meantime, this
    // keeps referring to the generation `select()` was actually called
    // under, not whatever `generation` holds once the catch runs.
    const gen = generation;
    selectionSeq += 1;
    latestSelectedId = id;
    setState({ ...state, selectedId: id });
    ipc.selectService(id).catch((caughtError: unknown) => {
      console.error("selectService failed:", errorMessage(caughtError));
      // The optimistic selectedId above is now unconfirmed — refetch so it
      // is restored from the backend's authoritative activeServiceId
      // (see refresh()'s own comment) rather than left pointing at a
      // service the backend never actually selected.
      void refresh(gen);
    });
  }

  function reorder(ids: readonly string[]): void {
    if (state.status !== "ready" || state.snapshot.configError) {
      return;
    }
    // See select()'s identical capture above.
    const gen = generation;
    const { snapshot } = state;
    setState({
      ...state,
      snapshot: { ...snapshot, services: reorderServicesByIds(snapshot.services, ids) },
    });
    ipc.reorderServices(ids).catch(() => {
      void refresh(gen);
    });
  }

  function openSettings(): void {
    ipc.openSettings().catch((caughtError: unknown) => {
      console.error("openSettings failed:", errorMessage(caughtError));
    });
  }

  return { getState, subscribe, start, stop, select, reorder, openSettings };
}
