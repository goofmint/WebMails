// Entry point for the agent injected into service webviews.
// Implements the startup sequence in design.md §2.2.14 ("Entry"), steps 1-6.
import type { ElumaBootstrap } from "./global";
import { matchRecipe } from "./recipes/registry";
import type { RecipeContext, SameOriginFetch } from "./recipes/types";
import type { Clock, RandomSource, Scheduler } from "./core/clock";
import { systemClock } from "./core/clock";
import { createReporter, defaultInvoke } from "./core/report";
import type { ReportInvoke } from "./core/report";
import { startLoop } from "./core/loop";

// `RecipeContext.fetch` scoped to the service's own origin. The real
// `SameOriginFetch` implementation lands in Task 2.6; until then any
// recipe that calls `ctx.fetch()` gets a clearly-labelled rejection instead
// of a fake/working fetch.
const notImplementedFetch: SameOriginFetch = () =>
  Promise.reject(new Error("[eluma-agent] SameOriginFetch is not implemented until Task 2.6"));

// The slice of `Window` that bootstrapAgent actually needs. The real
// `window` structurally satisfies this, so `bootstrapAgent(window)` below
// needs no cast; tests can pass a small plain object instead of a full
// `Window`.
export interface AgentWindow {
  readonly top: AgentWindow | null;
  __ELUMA__?: ElumaBootstrap;
  readonly location: { readonly origin: string };
  readonly document: Document;
}

function isElumaBootstrap(value: AgentWindow["__ELUMA__"]): value is ElumaBootstrap {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof value.serviceId === "string" &&
    value.serviceId.length > 0 &&
    typeof value.serviceUrl === "string" &&
    value.serviceUrl.length > 0 &&
    typeof value.reportIntervalMs === "number" &&
    Number.isFinite(value.reportIntervalMs) &&
    value.reportIntervalMs > 0 &&
    typeof value.reconcileIntervalMs === "number" &&
    Number.isFinite(value.reconcileIntervalMs) &&
    value.reconcileIntervalMs > 0
  );
}

export interface BootstrapDeps {
  readonly invoke?: ReportInvoke;
  readonly clock?: Clock;
  readonly scheduler?: Scheduler;
  readonly randomSource?: RandomSource;
}

// Boots the agent against `win`. Exported so tests can drive it with a fake
// window and inject invoke/clock/scheduler/random seams instead of the real
// Tauri bridge and timers.
export function bootstrapAgent(win: AgentWindow, deps: BootstrapDeps = {}): void {
  // Step 1 (frame guard), and must run before anything else: on Windows the
  // initialization script also runs in subframes (design.md §10).
  try {
    if (win.top !== win) {
      return;
    }
  } catch {
    // Accessing `top` across an origin boundary can throw; treat that the
    // same as "not the top frame".
    return;
  }

  // Step 2: read, then immediately delete, window.__ELUMA__.
  const bootstrap = win.__ELUMA__;
  delete win.__ELUMA__;

  if (!isElumaBootstrap(bootstrap)) {
    console.error("[eluma-agent] missing or invalid window.__ELUMA__; agent will not start");
    return;
  }

  let serviceUrl: URL;
  try {
    serviceUrl = new URL(bootstrap.serviceUrl);
  } catch {
    console.error(
      "[eluma-agent] window.__ELUMA__.serviceUrl is not a valid URL; agent will not start",
    );
    return;
  }

  // Step 3: install the Notification stub.
  // TODO(Task 4.3): replace window.Notification with the stub class
  // described in design.md §2.2.14 ("Notification stub") here, before any
  // recipe code runs. Not implemented in this task.

  // Step 4: recipe selection.
  const recipe = matchRecipe(serviceUrl);

  // Step 5: origin check. Rust handles the off-origin state (e.g. an auth
  // redirect); the agent simply does not start its loop.
  if (win.location.origin !== serviceUrl.origin) {
    return;
  }

  const context: RecipeContext = {
    serviceUrl,
    document: win.document,
    fetch: notImplementedFetch,
  };

  const report = createReporter({
    serviceId: bootstrap.serviceId,
    recipeId: recipe.id,
    serviceUrl,
    document: win.document,
    clock: deps.clock ?? systemClock,
    invoke: deps.invoke ?? defaultInvoke,
  });

  // Step 6: start the loop.
  startLoop({
    recipe,
    context,
    reconcileIntervalMs: bootstrap.reconcileIntervalMs,
    reportIntervalMs: bootstrap.reportIntervalMs,
    report,
    ...(deps.scheduler !== undefined ? { scheduler: deps.scheduler } : {}),
    ...(deps.randomSource !== undefined ? { randomSource: deps.randomSource } : {}),
  });
}

bootstrapAgent(window);
