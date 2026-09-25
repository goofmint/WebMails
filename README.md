# Eluma

Eluma is a Tauri desktop app that hosts webmail services, each in its own resident webview.
See `SPEC.md` and `design.md` for the product spec and technical design.

## Prerequisites

- [Node.js](https://nodejs.org/) (LTS) and [pnpm](https://pnpm.io/) (`packageManager` in `package.json` pins the version used)
- [Rust](https://www.rust-lang.org/tools/install), stable toolchain (see `rust-toolchain.toml`), with the `rustfmt` and `clippy` components
- Platform-specific Tauri build dependencies — follow the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/) for your OS:
  - **macOS:** Xcode Command Line Tools
  - **Windows:** Microsoft Visual Studio C++ Build Tools and WebView2 (bundled with recent Windows 11)
  - **Linux:** the packages listed in the Tauri prerequisites guide for your distribution (WebKitGTK, etc.)

## Scripts

Run from the repository root:

| Script             | Description                                                                        |
| ------------------ | ---------------------------------------------------------------------------------- |
| `pnpm dev`         | Start the Vite dev server for the shell UI                                         |
| `pnpm build`       | Build the agent bundle, then the shell (`dist/` + `src-tauri/agent-dist/agent.js`) |
| `pnpm build:shell` | Type-check and build the shell UI only                                             |
| `pnpm build:agent` | Build the injected agent bundle only                                               |
| `pnpm lint`        | ESLint + Prettier check                                                            |
| `pnpm format`      | Prettier write                                                                     |
| `pnpm typecheck`   | `tsc -b` across all TypeScript projects                                            |
| `pnpm test`        | Run the Vitest suite                                                               |
| `pnpm lint:rust`   | `cargo fmt --check` + `cargo clippy --all-targets -- -D warnings`                  |
| `pnpm format:rust` | `cargo fmt`                                                                        |

The Tauri desktop app itself (`pnpm tauri dev` / `pnpm tauri build`) is run by the project owner manually; it is not run as part of CI or by automated agents.

Rust embeds the agent bundle with `include_str!` (`src-tauri/src/agent.rs`), so run `pnpm build:agent` at least once before any `cargo` command (`cargo build`, `cargo clippy`, `cargo test`, …) — otherwise the Rust build fails because `src-tauri/agent-dist/agent.js` does not exist.
