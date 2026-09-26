// The agent bundle injected into service webviews, built by
// `pnpm build:agent` (see `vite.agent.config.ts`) into
// `agent-dist/agent.js`. `include_str!` makes the build fail at compile
// time if the agent has not been built.
//
// Used by `agent_bridge::capability::build_injection_script` (task 2.2)
// to build each service's injection script.
pub const AGENT_JS: &str = include_str!("../agent-dist/agent.js");
