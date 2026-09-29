/**
 * The settings screen's recipe panel (Task 5.1; design.md §2.2.13: "Recipe
 * panel (§14): recipe name, strategy, and what it reads, from
 * `recipe.describe()`."; SPEC.md §14). Read-only: no recipe selection or
 * override, no panel-specific state or IPC (out of scope for this task).
 * Row derivation is the pure `buildRecipeRows` (`recipeRows.ts`), so this
 * component only renders whatever it returns.
 */

import type { ServiceConfig } from "../ipc";
import { buildRecipeRows } from "./recipeRows";

export interface RecipePanelProps {
  readonly services: readonly ServiceConfig[];
}

export function RecipePanel({ services }: RecipePanelProps) {
  const rows = buildRecipeRows(services);

  return (
    <section className="recipe-panel settings-section" aria-labelledby="recipe-panel-heading">
      <h2 id="recipe-panel-heading" className="settings-section__title">
        Recipes
      </h2>
      {rows.length === 0 ? (
        <p className="recipe-panel__empty">No services to show a recipe for.</p>
      ) : (
        <ul className="recipe-panel__list">
          {rows.map((row) => (
            <li key={row.id} className="recipe-panel__item">
              <h3 className="recipe-panel__name">{row.name}</h3>
              {row.kind === "error" ? (
                <p className="recipe-panel__error" role="alert">
                  {row.message}
                </p>
              ) : (
                <dl className="recipe-panel__details">
                  <div className="recipe-panel__field">
                    <dt>Recipe</dt>
                    <dd>{row.displayName}</dd>
                  </div>
                  <div className="recipe-panel__field">
                    <dt>Strategy</dt>
                    <dd>{row.strategy}</dd>
                  </div>
                  <div className="recipe-panel__field">
                    <dt>Reads</dt>
                    <dd>
                      <code>{row.reads}</code>
                    </dd>
                  </div>
                </dl>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
