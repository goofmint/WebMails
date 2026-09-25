import { defineConfig } from "vite";

// Builds the agent that gets injected into service webviews into a single
// IIFE file. Rust embeds it with `include_str!` (wired up in task 0.2).
export default defineConfig({
  // The agent has no HTML entry and needs none of the shell's public assets.
  publicDir: false,
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  build: {
    outDir: "src-tauri/agent-dist",
    emptyOutDir: true,
    lib: {
      entry: "agent/main.ts",
      name: "ElumaAgent",
      formats: ["iife"],
      fileName: () => "agent.js",
    },
  },
});
