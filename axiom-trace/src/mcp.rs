#![cfg(feature = "mcp")]
//! MCP server wrapper exposing the Axiom ledger as tools (issues #17/#18/#20).
//!
//! The ledger logic lives in the always-compiled `crate::Ledger`. Here we wrap
//! it as an MCP `ServerHandler` so any MCP client (Claude Code, Cursor,
//! ...) can call `why_changed` / `replay_from` / `diff_states` /
//! `list_handlers`. This module is gated behind `--features mcp` because it
//! pulls in `rmcp` (and its async runtime).
//!
//! Usage (from a binary):
//! ```rust,ignore
//! let service = axiom_trace::mcp::AxiomTrace::new()
//!     .serve(rmcp::transport::stdio())
//!     .await?;
//! service.waiting().await?;
//! ```

use std::sync::{Arc, Mutex};

use rmcp::{model::ServerInfo, tool, ServerHandler};

use crate::{HandlerEvent, Ledger};

/// The Axiom trace server: holds a shared ledger and serves it as MCP tools.
#[derive(Clone)]
pub struct AxiomTrace {
    ledger: Arc<Mutex<Ledger>>,
}

impl AxiomTrace {
    /// Empty ledger.
    pub fn new() -> Self {
        Self {
            ledger: Arc::new(Mutex::new(Ledger::new())),
        }
    }

    /// Pre-populated ledger (e.g. the canonical "total became 1422" scenario).
    pub fn with_ledger(l: Ledger) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(l)),
        }
    }

    /// Shared access to the underlying ledger (for recording new dispatches).
    pub fn ledger(&self) -> Arc<Mutex<Ledger>> {
        self.ledger.clone()
    }
}

fn fmt_val(o: Option<i64>) -> String {
    match o {
        Some(v) => v.to_string(),
        None => "∅ (no binding)".to_string(),
    }
}

// Tool methods. The `#[tool(tool_box)]` attribute registers them in a static
// toolbox that the `ServerHandler` impl below dispatches to.
#[tool(tool_box)]
impl AxiomTrace {
    /// Issue #20: list all effect-handler names recorded in the ledger.
    #[tool(description = "List all effect-handler names recorded in the Axiom ledger")]
    fn list_handlers(&self) -> String {
        let l = self.ledger.lock().expect("ledger lock poisoned");
        let hs = l.list_handlers();
        if hs.is_empty() {
            "no handlers recorded".to_string()
        } else {
            hs.join(", ")
        }
    }

    /// Issue #18: why did `var` become its final value?
    #[tool(description = "Explain why a variable became its final value (issue #18)")]
    fn why_changed(
        &self,
        #[tool(param)]
        #[schemars(description = "the variable to explain, e.g. \"total\"")]
        var: String,
    ) -> String {
        let l = self.ledger.lock().expect("ledger lock poisoned");
        match l.why_changed(&var) {
            Some(HandlerEvent { handler, effect, produced, value, hlc }) => format!(
                "handler `{}` set `{}` = {} (effect `{}`, hlc {})",
                handler, produced, value, effect, hlc
            ),
            None => format!("no producer recorded for `{}`", var),
        }
    }

    /// Issue #19: replay the value of `var` as of `tick`.
    #[tool(description = "Replay the value a variable had at a given HLC tick (issue #19)")]
    fn replay_from(
        &self,
        #[tool(param)]
        #[schemars(description = "the variable to replay")]
        var: String,
        #[tool(param)]
        #[schemars(description = "hybrid-logical-clock tick (inclusive)")]
        tick: u64,
    ) -> String {
        let l = self.ledger.lock().expect("ledger lock poisoned");
        format!(
            "{} = {} @ tick {}",
            var,
            fmt_val(l.replay_from(&var, tick)),
            tick
        )
    }

    /// Issue #18: diff `var` between two ticks.
    #[tool(description = "Diff a variable's value between two HLC ticks (issue #18)")]
    fn diff_states(
        &self,
        #[tool(param)]
        #[schemars(description = "the variable to diff")]
        var: String,
        #[tool(param)]
        #[schemars(description = "start tick (inclusive)")]
        from: u64,
        #[tool(param)]
        #[schemars(description = "end tick (inclusive)")]
        to: u64,
    ) -> String {
        let l = self.ledger.lock().expect("ledger lock poisoned");
        let (a, b) = l.diff_states(&var, from, to);
        format!(
            "{}: {} -> {}  (ticks {} -> {})",
            var,
            fmt_val(a),
            fmt_val(b),
            from,
            to
        )
    }
}

// Wire the toolbox into the MCP server handler.
#[tool(tool_box)]
impl ServerHandler for AxiomTrace {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Axiom time-travel trace debugger: explains effect-handler dispatches."
                    .into(),
            ),
            ..Default::default()
        }
    }
}
