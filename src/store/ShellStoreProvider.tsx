/**
 * React context provider for `createShellStore` (Task 1.10; design.md
 * §2.2.13: "React context plus `useSyncExternalStore` over a small
 * store... No state library."). See `useShellStore.ts` for the hooks that
 * read this provider's context.
 */

import { useEffect, useMemo, type ReactNode } from "react";
import type { ShellIpc } from "../ipc";
import { createShellStore } from "./shellStore";
import { ShellStoreContext } from "./ShellStoreContext";

export interface ShellStoreProviderProps {
  readonly ipc: ShellIpc;
  readonly children: ReactNode;
}

/** Creates one store per `ipc` instance, starting it on mount and stopping it on unmount. */
export function ShellStoreProvider({ ipc, children }: ShellStoreProviderProps) {
  const store = useMemo(() => createShellStore(ipc), [ipc]);

  useEffect(() => {
    store.start();
    return () => {
      store.stop();
    };
  }, [store]);

  return <ShellStoreContext.Provider value={store}>{children}</ShellStoreContext.Provider>;
}
