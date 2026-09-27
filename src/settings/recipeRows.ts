/**
 * Pure row model for the settings screen's recipe panel (Task 5.1;
 * design.md §2.2.13 "Recipe panel (§14): recipe name, strategy, and what
 * it reads, from `recipe.describe()`."; SPEC.md §14: "A per-service panel
 * shows what its recipe does: which strategy, which selector or endpoint,
 * what it reads.").
 *
 * `describe()` is called on a freshly parsed `URL`, never anything carrying
 * agent runtime state — this module never imports from `agent/main.ts` or
 * `agent/core/*`, only the recipe registry itself. `gmail`'s `describe()`
 * happens to read its own module-private `mode` (design.md §2.2.14: it
 * starts `"unvalidated"`, which `describe()` treats the same as `"feed"`,
 * reporting `strategy: "fetch"`, and only becomes `"title"` after `read()`
 * observes a validation failure). The settings window imports
 * `agent/recipes/registry` as its own module instance — separate from
 * whatever agent bundle is (or isn't) running injected into a service
 * webview — and never calls any recipe's `read()`, so that shared instance
 * never leaves `"unvalidated"`. The row this module reports for Gmail is
 * therefore always that initial, unvalidated description, not whatever a
 * live agent may have fallen back to on the actual page.
 */

import { matchRecipe } from "../../agent/recipes/registry";
import type { RecipeDescription } from "../../agent/recipes/types";
import type { ServiceConfig } from "../ipc";

/** The subset of `ServiceConfig` this module needs (Task 1.10/1.12's shape). */
export type RecipeRowService = Pick<ServiceConfig, "id" | "name" | "url">;

export interface RecipeRowOk {
  readonly id: string;
  readonly name: string;
  readonly kind: "ok";
  readonly displayName: string;
  readonly strategy: RecipeDescription["strategy"];
  readonly reads: string;
}

export interface RecipeRowError {
  readonly id: string;
  readonly name: string;
  readonly kind: "error";
  readonly message: string;
}

export type RecipeRow = RecipeRowOk | RecipeRowError;

/**
 * Builds one row per service, in the same order `services` was given
 * (config order; design.md §2.2.13). A service whose `url` fails to parse
 * as a URL gets an error row — not a recipe row with a default/fallback
 * description (no fallback defaults, per this project's rules).
 */
export function buildRecipeRows(services: readonly RecipeRowService[]): readonly RecipeRow[] {
  return services.map((service): RecipeRow => {
    let url: URL;
    try {
      url = new URL(service.url);
    } catch {
      return {
        id: service.id,
        name: service.name,
        kind: "error",
        message: `Could not parse this service's URL: "${service.url}".`,
      };
    }

    const recipe = matchRecipe(url);
    const description = recipe.describe(url);
    return {
      id: service.id,
      name: service.name,
      kind: "ok",
      displayName: recipe.displayName,
      strategy: description.strategy,
      reads: description.reads,
    };
  });
}
