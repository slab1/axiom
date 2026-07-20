//! Axiom purity / effect-row analysis (Phase 2 core).
//!
//! This module is **always compiled** (no external dependencies) so the purity
//! logic is unit-tested in the default `cargo test` run — it is locked down
//! before the heavier `parallel` pass (which needs `hvm-core`) consumes it.
//!
//! The key idea (issue #9/#12): Nova's type-checker already computes an *effect
//! row* per expression — the set of effects it may perform (`Net`, `Fs`,
//! `Audit`, ...). An **empty** row means the expression is pure and therefore a
//! candidate for automatic parallel extraction. Nova never acts on this; Axiom
//! does. See `TRACKING.md` Phase 2 and issues #8/#9/#10/#11/#12.

use std::collections::BTreeSet;

/// Effects an Axiom expression may perform.
///
/// Mirrors Nova's effect vocabulary (`Net`, `Fs`, `Audit`, `State`) and leaves
/// room for user/extension effects via [`Effect::Other`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Effect {
    Net,
    Fs,
    Audit,
    State,
    Io,
    /// Any effect not in the built-in vocabulary.
    Other(String),
}

impl Effect {
    /// Parse an effect name into the vocabulary. Unknown names map to
    /// [`Effect::Other`] so the analysis never silently drops information.
    pub fn parse(name: &str) -> Effect {
        match name {
            "Net" => Effect::Net,
            "Fs" => Effect::Fs,
            "Audit" => Effect::Audit,
            "State" => Effect::State,
            "Io" => Effect::Io,
            other => Effect::Other(other.to_string()),
        }
    }
}

/// A row of effects attached to an expression.
///
/// An empty row is the pure case. Rows are unioned when combining
/// sub-expressions (see [`EffectRow::extend`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EffectRow {
    effects: BTreeSet<Effect>,
}

impl EffectRow {
    /// The empty (pure) row.
    pub fn new() -> Self {
        Self::default()
    }

    /// The pure row — no effects.
    pub fn pure() -> Self {
        Self::default()
    }

    /// Insert an effect; returns `true` if it was newly added.
    pub fn insert(&mut self, effect: Effect) -> bool {
        self.effects.insert(effect)
    }

    /// Remove an effect; returns `true` if it was present.
    pub fn remove(&mut self, effect: &Effect) -> bool {
        self.effects.remove(effect)
    }

    /// Union this row with `other` (used when combining sub-expressions).
    pub fn extend(&mut self, other: &EffectRow) {
        self.effects.extend(other.effects.iter().cloned());
    }

    /// True when the row is empty — i.e. no observable side effects.
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }

    /// True when the expression is safe to auto-parallelize.
    ///
    /// Currently equivalent to [`EffectRow::is_pure`]. The full rule (issue #9)
    /// also requires every operand to be a value type so the region pass can
    /// split work without aliasing hazards; that needs type info from
    /// `types::`, which is wired in Phase 2's later sub-issues.
    pub fn is_parallel_safe(&self) -> bool {
        self.is_pure()
    }

    /// Number of distinct effects in the row.
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// True when the row carries no effects.
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Iterate the effects in the row (sorted, because the backing set is a `BTreeSet`).
    pub fn effects(&self) -> impl Iterator<Item = &Effect> {
        self.effects.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_row_is_pure_and_parallel_safe() {
        let row = EffectRow::default();
        assert!(row.is_pure());
        assert!(row.is_parallel_safe());
        assert_eq!(row.len(), 0);
        assert!(row.is_empty());
    }

    #[test]
    fn row_with_effect_is_not_pure() {
        let mut row = EffectRow::new();
        assert!(row.insert(Effect::Net));
        assert!(!row.is_pure());
        assert!(!row.is_parallel_safe());
        assert_eq!(row.len(), 1);
        // inserting the same effect again is a no-op
        assert!(!row.insert(Effect::Net));
        assert_eq!(row.len(), 1);
    }

    #[test]
    fn parse_known_and_unknown_effects() {
        assert_eq!(Effect::parse("Fs"), Effect::Fs);
        assert_eq!(Effect::parse("Audit"), Effect::Audit);
        assert_eq!(Effect::parse("State"), Effect::State);
        assert_eq!(Effect::parse("Net"), Effect::Net);
        assert_eq!(Effect::parse("custom"), Effect::Other("custom".to_string()));
    }

    #[test]
    fn extend_unions_two_rows() {
        let mut a = EffectRow::pure();
        a.insert(Effect::Net);
        let mut b = EffectRow::pure();
        b.insert(Effect::Fs);
        a.extend(&b);
        assert_eq!(a.len(), 2);
        assert!(!a.is_pure());
        assert!(a.effects().any(|e| *e == Effect::Fs));
    }

    #[test]
    fn removing_effect_can_restore_purity() {
        let mut row = EffectRow::pure();
        row.insert(Effect::Io);
        assert!(!row.is_pure());
        assert!(row.remove(&Effect::Io));
        assert!(row.is_pure());
        assert_eq!(row.len(), 0);
    }

    #[test]
    fn determinism_rows_are_sorted_for_stable_comparison() {
        let mut row = EffectRow::pure();
        row.insert(Effect::Io);
        row.insert(Effect::Audit);
        row.insert(Effect::Net);
        let names: Vec<String> = row
            .effects()
            .map(|e| format!("{:?}", e))
            .collect();
        // derive(Ord) orders enum variants by declaration order:
        // Net(0) < Fs(1) < Audit(2) < State(3) < Io(4), so the BTreeSet
        // iteration is Net, Audit, Io.
        assert_eq!(names, vec!["Net".to_string(), "Audit".to_string(), "Io".to_string()]);
    }

    #[test]
    fn parallel_safety_requires_purity() {
        // two independent pure branches are each parallel-safe
        assert!(EffectRow::pure().is_parallel_safe());
        assert!(EffectRow::pure().is_parallel_safe());
        // a single impure effect disqualifies the whole expression
        let mut mixed = EffectRow::pure();
        mixed.insert(Effect::State);
        assert!(!mixed.is_parallel_safe());
    }
}
