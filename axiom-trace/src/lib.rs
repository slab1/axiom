//! Axiom MCP time-travel debugger.
//!
//! Phase 4: records effect-handler dispatches to a deterministic ledger and
//! exposes `why_changed` / `replay_from` / `diff_states` tools to any MCP
//! client (Claude Code, Cursor, ...).
pub fn version() -> &'static str { env!("CARGO_PKG_VERSION") }
