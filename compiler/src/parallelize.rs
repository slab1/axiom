//! Axiom automatic-parallelism pass (Phase 2).
//!
//! This pass consumes the purity analysis from [`crate::purity`] — the exact
//! information Nova's type-checker computes via `infer_effects` but never acts
//! on. This starter shows the decision point; the real lowering to `hvm-core`
//! (issue #10) lands when the `parallel` feature + `hvm-core` dependency are
//! wired in (Phase 2, after issue #3-style backend setup).
//!
//! Build with: `cargo build -p axiom-compiler --features parallel`
//! See issues #8/#9/#10/#11/#12 and `TRACKING.md` Phase 2.
#![cfg(feature = "parallel")]

use crate::purity::{Effect, EffectRow};

/// Decide whether the expression named `name` with effect `row` should be
/// auto-parallelized.
///
/// Current rule (issue #9/#12): pure rows are parallel-safe. The full rule
/// also requires value-typed operands so the region pass can split work
/// without aliasing — that needs type info from `types::`, wired later.
pub fn should_parallelize(name: &str, row: &EffectRow) -> bool {
    // `name` is currently informational (used in diagnostics / ledger entries).
    let _ = name;
    row.is_parallel_safe()
}

/// Convenience: is a row free of the effects that forbid parallelism?
/// (All effects currently forbid it; this helper exists so the allow-list can
/// grow in later sub-issues without touching call sites.)
pub fn forbids_parallel(row: &EffectRow) -> bool {
    row.effects().any(|e| matches!(e, Effect::Net | Effect::Fs | Effect::Audit | Effect::State | Effect::Io | Effect::Other(_)))
}
