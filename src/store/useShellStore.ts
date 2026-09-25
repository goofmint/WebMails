/**
 * Hooks for reading the `ShellStore` a `ShellStoreProvider` ancestor
 * provided (Task 1.10; design.md §2.2.13: `useSyncExternalStore`).
 */

import { useContext, useSyncExternalStore } from "react";
import { ShellStoreContext } from "./ShellStoreContext";
import type { ShellState, ShellStore } from "./shellStore";

/** The store itself, for calling `select`/`reorder`/`openSettings`. */
export function useShellStore(): ShellStore {
  const store = useContext(ShellStoreContext);
  if (store === null) {
    throw new Error("useShellStore must be used within a ShellStoreProvider");
  }
  return store;
}

/** The store's current state, subscribed via `useSyncExternalStore`. */
export function useShellState(): ShellState {
  const store = useShellStore();
  return useSyncExternalStore(
    (onStoreChange) => store.subscribe(onStoreChange),
    () => store.getState(),
  );
}
