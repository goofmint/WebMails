// The agent bundle injected into service webviews, built by
// `pnpm build:agent` (see `vite.agent.config.ts`) into
// `agent-dist/agent.js`. `include_str!` makes the build fail at compile
// time if the agent has not been built.
//
// Unused until the multiwebview host injects it (task 2.2).
#[allow(dead_code)]
pub const AGENT_JS: &str = include_str!("../agent-dist/agent.js");
