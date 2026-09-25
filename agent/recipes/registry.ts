import type { Recipe } from "./types";
import { gmail } from "./gmail";
import { icloud } from "./icloud";
import { outlook } from "./outlook";
import { generic } from "./generic";

// Recipes that match a specific service, tried in order before falling back
// to `generic`. Keep `generic` out of this list: it always matches, so it
// must never be checked before a more specific recipe.
const specificRecipes: readonly Recipe[] = [gmail, icloud, outlook];

// Full registry, in the order the shell and recipe panel should display
// them. `generic` is last because it always matches.
export const recipes: readonly Recipe[] = [...specificRecipes, generic];

// Returns the first recipe whose `matches()` is true, trying the specific
// recipes in order and falling back to `generic` explicitly. `generic`
// always matches, so this never returns undefined.
export function matchRecipe(serviceUrl: URL): Recipe {
  for (const recipe of specificRecipes) {
    if (recipe.matches(serviceUrl)) {
      return recipe;
    }
  }
  return generic;
}
