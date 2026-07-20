//! Axiom MCP time-travel debugger (Phase 4).
//!
//! Records effect-handler dispatches to a deterministic ledger and exposes
//! `why_changed` / `replay_from` / `diff_states` tools to any MCP client
//! (Claude Code, Cursor, ...). See `TRACKING.md` Phase 4 and issues #17/#18/#19/#20.
//!
//! Build with: `cargo build -p axiom-trace --features mcp`
//! (rmcp is feature-gated so the default workspace builds without it).
#![cfg(feature = "mcp")]

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
