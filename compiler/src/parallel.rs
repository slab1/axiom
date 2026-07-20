//! Phase 2 — parallelism analysis core (always compiled).
//!
//! This is the decision logic behind `parallelize.rs`. Nova's type-checker
//! produces an effect row per expression (via `infer_effects`); Axiom's
//! novel step is to *act* on it. This module consumes those rows and decides
//! which expressions are safe to auto-parallelize. It has no external
//! dependencies, so it is unit-tested by the default `cargo test`.
//!
//! Covers issues #8 (runs after `infer_effects`) and #9 (detect pure +
//! data-parallel: empty effect row AND value-typed operands).
//! See `TRACKING.md` Phase 2.

use crate::purity::EffectRow;

/// A single expression under analysis.
#[derive(Debug, Clone)]
pub struct Expr {
    pub name: String,
    /// Effect row produced by `infer_effects`.
    pub row: EffectRow,
    /// Whether every operand is a value type (no references/aliases that
    /// would force sequential evaluation). Wired from the type-checker.
    pub value_typed_operands: bool,
}

/// Outcome of the analysis for one expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParallelPlan {
    /// Safe to extract into `hvm-core` net / `scf.parallel`.
    Parallel(String),
    /// Must stay sequential (impure, or has aliased operands).
    Sequential(String),
}

impl ParallelPlan {
    pub fn is_parallel(&self) -> bool {
        matches!(self, ParallelPlan::Parallel(_))
    }
}

/// Decide the parallelization plan for a batch of expressions.
///
/// Rule (issues #8/#9): an expression is parallel-safe iff it is **pure**
/// (empty effect row) **and** every operand is value-typed. Anything else
/// stays sequential.
pub fn analyze(exprs: &[Expr]) -> Vec<ParallelPlan> {
    exprs
        .iter()
        .map(|e| {
            if e.row.is_pure() && e.value_typed_operands {
                ParallelPlan::Parallel(e.name.clone())
            } else {
                ParallelPlan::Sequential(e.name.clone())
            }
        })
        .collect()
}

/// Convenience: the names of the expressions chosen for parallelism.
pub fn parallelizable(exprs: &[Expr]) -> Vec<String> {
    analyze(exprs)
        .into_iter()
        .filter_map(|p| match p {
            ParallelPlan::Parallel(n) => Some(n),
            ParallelPlan::Sequential(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::purity::EffectRow;

    fn pure_value(name: &str) -> Expr {
        Expr {
            name: name.to_string(),
            row: EffectRow::pure(),
            value_typed_operands: true,
        }
    }

    fn impure(name: &str) -> Expr {
        let mut row = EffectRow::new();
        row.insert(crate::purity::Effect::Net);
        Expr {
            name: name.to_string(),
            row,
            value_typed_operands: true,
        }
    }

    fn aliased(name: &str) -> Expr {
        // pure but has reference-typed operands -> not data-parallel
        Expr {
            name: name.to_string(),
            row: EffectRow::pure(),
            value_typed_operands: false,
        }
    }

    #[test]
    fn pure_value_typed_is_parallel() {
        let plans = analyze(&[pure_value("map")]);
        assert_eq!(plans, vec![ParallelPlan::Parallel("map".into())]);
        assert!(plans[0].is_parallel());
    }

    #[test]
    fn impure_is_sequential() {
        let plans = analyze(&[impure("fetch")]);
        assert_eq!(plans, vec![ParallelPlan::Sequential("fetch".into())]);
    }

    #[test]
    fn aliased_pure_is_sequential() {
        // pure but not data-parallel (reference operands)
        let plans = analyze(&[aliased("fold_ref")]);
        assert_eq!(plans, vec![ParallelPlan::Sequential("fold_ref".into())]);
    }

    #[test]
    fn mixed_batch_keeps_only_parallel() {
        let exprs = vec![
            pure_value("map"),
            impure("fetch"),
            aliased("fold_ref"),
            pure_value("scan"),
        ];
        assert_eq!(parallelizable(&exprs), vec!["map".to_string(), "scan".to_string()]);
    }

    #[test]
    fn empty_batch_is_empty() {
        assert!(analyze(&[]).is_empty());
        assert!(parallelizable(&[]).is_empty());
    }
}
