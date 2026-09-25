/**
 * The raw React context `ShellStoreProvider.tsx` populates and
 * `useShellStore.ts` reads. Split into its own module (no components here)
 * so `react-refresh/only-export-components` doesn't flag either of those
 * two files for mixing component and non-component exports.
 */

import { createContext } from "react";
import type { ShellStore } from "./shellStore";

export const ShellStoreContext = createContext<ShellStore | null>(null);
