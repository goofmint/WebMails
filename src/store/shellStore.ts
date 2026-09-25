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
  let disposed = false;

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

  async function refresh(): Promise<void> {
    try {
      const snapshot = await ipc.getSnapshot();
      // The backend is authoritative for which service is active
      // (`snapshot.activeServiceId`), so every (re)fetch re-syncs
      // `selectedId` to it rather than keeping whatever this store
      // guessed before — `select()`'s own optimistic update and the
      // `select-service` event are what keep `selectedId` current between
      // fetches.
      setState({ status: "ready", snapshot, selectedId: snapshot.activeServiceId });
    } catch (caughtError) {
      setState({ status: "error", message: errorMessage(caughtError) });
    }
  }

  async function bootstrap(): Promise<void> {
    try {
      const subscriptions = await Promise.all([
        ipc.onServicesChanged(() => {
          void refresh();
        }),
        ipc.onSelectService(({ id }) => {
          if (state.status === "ready") {
            setState({ ...state, selectedId: id });
          }
        }),
      ]);

      if (disposed) {
        for (const unlisten of subscriptions) {
          unlisten();
        }
        return;
      }

      unlistenFns = subscriptions;
    } catch (caughtError) {
      setState({ status: "error", message: errorMessage(caughtError) });
      return;
    }

    await refresh();
  }

  function start(): void {
    if (started) {
      return;
    }
    started = true;
    disposed = false;
    void bootstrap();
  }

  function stop(): void {
    if (!started) {
      return;
    }
    started = false;
    disposed = true;
    for (const unlisten of unlistenFns) {
      unlisten();
    }
    unlistenFns = [];
  }

  function select(id: string): void {
    if (state.status !== "ready" || state.snapshot.configError) {
      return;
    }
    setState({ ...state, selectedId: id });
    ipc.selectService(id).catch((caughtError: unknown) => {
      console.error("selectService failed:", errorMessage(caughtError));
    });
  }

  function reorder(ids: readonly string[]): void {
    if (state.status !== "ready" || state.snapshot.configError) {
      return;
    }
    const { snapshot } = state;
    setState({
      ...state,
      snapshot: { ...snapshot, services: reorderServicesByIds(snapshot.services, ids) },
    });
    ipc.reorderServices(ids).catch(() => {
      void refresh();
    });
  }

  function openSettings(): void {
    ipc.openSettings().catch((caughtError: unknown) => {
      console.error("openSettings failed:", errorMessage(caughtError));
    });
  }

  return { getState, subscribe, start, stop, select, reorder, openSettings };
}
