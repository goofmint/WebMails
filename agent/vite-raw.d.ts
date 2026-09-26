// Ambient module declaration for Vite's `?raw` string imports, used only by
// agent tests to load fixture files (agent/recipes/gmail.test.ts). The
// agent's own tsconfig deliberately omits `"types": ["vite/client"]` (the
// agent bundle must stay free of Vite's browser globals such as
// `import.meta.env`), so this declares just the one module pattern the
// tests need instead of pulling in all of vite/client's ambient types.
declare module "*?raw" {
  const content: string;
  export default content;
}
