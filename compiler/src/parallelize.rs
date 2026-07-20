//! Axiom automatic-parallelism pass (Phase 2).
//!
//! This is the **starter** for `parallelize.rs` — the novel insight that makes
//! Axiom surpass Nova. Nova's type-checker *already computes* which code is pure
//! (empty effect row) but never parallelizes it. This pass consumes that info.
//!
//! Build with: `cargo build -p axiom-compiler --features parallel`
//! See `TRACKING.md` Phase 2 and issues #8/#9/#10/#11/#12.
#![cfg(feature = "parallel")]

/// A minimal purity predicate over an effect row (the unit this pass operates on).
///
/// In Nova, an effect row is the set of effects a function may perform
/// (e.g. `Net`, `Fs`, `Audit`). An **empty** row means the expression is pure
/// and therefore a candidate for automatic parallel extraction.
///
/// This is the exact logic the parallelism pass will use: pure + data-parallel
/// (value-typed operands) => lower to `hvm-core` net or `scf.parallel`.
///
/// See issue #12 (good first issue): extend this with unit tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectRow {
    pub effects: Vec<String>,
}

impl EffectRow {
    /// True when the row is empty — i.e. the expression has no observable side
    /// effects and is safe to parallelize.
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }

    /// True when pure AND every operand is a value type (no references / aliases
    /// that would force sequential evaluation). Placeholder: real implementation
    /// reads type info from the type-checker.
    pub fn is_data_parallel(&self) -> bool {
        self.is_pure()
        // && operands are all value types (TODO: wire to types::)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_row_is_pure() {
        assert!(EffectRow::default().is_pure());
        assert!(EffectRow::default().is_data_parallel());
    }

    #[test]
    fn row_with_effect_is_not_pure() {
        let row = EffectRow {
            effects: vec!["Net".to_string()],
        };
        assert!(!row.is_pure());
        assert!(!row.is_data_parallel());
    }
}
