//! Axiom MCP time-travel debugger (Phase 4).
//!
//! Records effect-handler dispatches into a **deterministic, content-addressed**
//! ledger so an agent (or human) can ask *"why did `total` become 1422?"*
//! and replay / inspect state. The ledger logic here is pure Rust and
//! unit-tested by the default `cargo test` — it has no MCP dependency.
//!
//! The MCP server wrapper lives in `mcp.rs` behind `--features mcp` (using
//! the `rmcp` SDK) and exposes the ledger as the tools in issues
//! #17 / #18 / #19 / #20. See `TRACKING.md` Phase 4.

#[cfg(feature = "mcp")]
pub mod mcp;

use std::collections::BTreeMap;

/// A single effect-handler dispatch recorded in the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerEvent {
    /// Name of the handler that ran (e.g. `audit_log`).
    pub handler: String,
    /// Effect it performed (e.g. `Audit`, `Net`).
    pub effect: String,
    /// Variable this dispatch produced / updated.
    pub produced: String,
    /// Value bound to `produced`.
    pub value: i64,
    /// Hybrid-logical-clock tick — gives a total, deterministic order.
    pub hlc: u64,
}

/// Deterministic ledger of handler dispatches.
///
/// Insertion order + `hlc` give a stable timeline; all queries are pure
/// functions over the event list, so the same ledger always yields the same
/// answer (this is what makes `diff_states` a trustworthy supply-chain /
/// debugging artifact).
#[derive(Debug, Clone, Default)]
pub struct Ledger {
    events: Vec<HandlerEvent>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append one dispatch event.
    pub fn record(&mut self, e: HandlerEvent) {
        self.events.push(e);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Issue #20 (`list_handlers`): distinct handler names in the ledger.
    pub fn list_handlers(&self) -> Vec<String> {
        let mut v: Vec<String> = self.events.iter().map(|e| e.handler.clone()).collect();
        v.sort();
        v.dedup();
        v
    }

    /// Issue #19 (`replay_from`): the value bound to `var` as of `tick`
    /// (inclusive). Returns the last producer at or before `tick`, or `None`
    /// if the variable had no binding by then.
    pub fn replay_from(&self, var: &str, tick: u64) -> Option<i64> {
        self.events
            .iter()
            .filter(|e| e.hlc <= tick && e.produced == var)
            .last()
            .map(|e| e.value)
    }

    /// Issue #18 (`why_changed`): the event that produced `var`'s final
    /// value. The core answer to *"why did `total` become 1422?"*.
    pub fn why_changed(&self, var: &str) -> Option<&HandlerEvent> {
        self.events.iter().filter(|e| e.produced == var).last()
    }

    /// Issue #18 (`diff_states`): value of `var` at two ticks, so a caller
    /// can see what changed between them.
    pub fn diff_states(&self, var: &str, from: u64, to: u64) -> (Option<i64>, Option<i64>) {
        (self.replay_from(var, from), self.replay_from(var, to))
    }

    /// Content-addressed digest of the whole ledger (for supply-chain diffing
    /// across releases — feeds the epoch model, issue #21).
    pub fn digest(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        for e in &self.events {
            e.hash(&mut h);
        }
        format!("{:x}", h.finish())
    }

    /// Export as an ordered map of `var -> final value` (handy for agents).
    pub fn final_state(&self) -> BTreeMap<String, i64> {
        let mut m = BTreeMap::new();
        for e in &self.events {
            m.insert(e.produced.clone(), e.value);
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(handler: &str, effect: &str, produced: &str, value: i64, hlc: u64) -> HandlerEvent {
        HandlerEvent {
            handler: handler.to_string(),
            effect: effect.to_string(),
            produced: produced.to_string(),
            value,
            hlc,
        }
    }

    /// Builds the canonical "total became 1422" scenario from the issue.
    fn scenario() -> Ledger {
        let mut l = Ledger::new();
        l.record(ev("price_fetch", "Net", "price", 1000, 1));
        l.record(ev("tax_calc", "Pure", "tax", 400, 2));
        l.record(ev("audit_log", "Audit", "total", 1400, 3));
        l.record(ev("discount", "Pure", "total", 1422, 4)); // final total
        l
    }

    #[test]
    fn record_and_len() {
        let l = scenario();
        assert_eq!(l.len(), 4);
        assert!(!l.is_empty());
    }

    #[test]
    fn list_handlers_distinct_sorted() {
        let l = scenario();
        assert_eq!(l.list_handlers(), vec!["audit_log", "discount", "price_fetch", "tax_calc"]);
    }

    #[test]
    fn why_changed_returns_final_producer() {
        let l = scenario();
        let e = l.why_changed("total").expect("total was produced");
        assert_eq!(e.handler, "discount");
        assert_eq!(e.value, 1422);
    }

    #[test]
    fn replay_from_clamps_to_tick() {
        let l = scenario();
        // "total" is first produced at tick 3 (audit_log -> 1400);
        // at tick 2 it has no binding yet
        assert_eq!(l.replay_from("total", 2), None);
        // at tick 3 the latest producer of total is audit_log (1400)
        assert_eq!(l.replay_from("total", 3), Some(1400));
        // at tick 4 it is the discount (1422)
        assert_eq!(l.replay_from("total", 4), Some(1422));
        // at tick 0 nothing yet
        assert_eq!(l.replay_from("total", 0), None);
    }

    #[test]
    fn diff_states_before_after() {
        let l = scenario();
        let (before, after) = l.diff_states("total", 3, 4);
        assert_eq!(before, Some(1400));
        assert_eq!(after, Some(1422));
    }

    #[test]
    fn digest_is_deterministic() {
        assert_eq!(scenario().digest(), scenario().digest());
    }

    #[test]
    fn final_state_collects_latest_per_var() {
        let s = scenario();
        let f = s.final_state();
        assert_eq!(f.get("total"), Some(&1422));
        assert_eq!(f.get("price"), Some(&1000));
    }
}
