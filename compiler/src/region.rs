//! Phase 3 — issue #14: region-inference pass + GC fallback.
//!
//! Axiom defaults to garbage collection. This pass assigns a *region* to each
//! value: `own` values become uniquely owned (bypassing the GC), while every
//! unannotated value **falls back to the GC region**. The decision is local
//! and total, so it is unit-tested without a runtime.
//!
//! (The full pass walks the type-checker's binding graph; this module is the
//! reusable core that decides a single value's region and the GC fallback.)
//! See `TRACKING.md` Phase 3 and issue #14.

use crate::ownership::Own;

/// Memory region a value lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// Uniquely-owned value — moved, never GC'd.
    Unique,
    /// Garbage-collected value — the default for unannotated code.
    Gc,
}

/// A value together with how it is owned, for inference input.
#[derive(Debug, Clone)]
pub enum Value<T> {
    /// Explicitly uniquely owned.
    Owned(Own<T>),
    /// Ordinary shared value (GC-default).
    Shared(T),
}

impl<T> Value<T> {
    pub fn region(&self) -> Region {
        match self {
            Value::Owned(_) => Region::Unique,
            Value::Shared(_) => Region::Gc,
        }
    }
}

/// Infer the region for an `own` value: uniquely owned.
pub fn region_of_own<T>(_v: &Own<T>) -> Region {
    Region::Unique
}

/// Infer the region for an ordinary (GC-default) value.
pub fn region_of<T>(_v: &T) -> Region {
    Region::Gc
}

/// Region-inference over a mixed collection: `own` => Unique, else Gc.
/// This is the GC **fallback** — anything not explicitly owned stays GC'd.
pub fn infer_regions<T>(values: &[Value<T>]) -> Vec<Region> {
    values.iter().map(|v| v.region()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::Own;

    #[test]
    fn own_is_unique() {
        let o = Own::new(vec![1, 2, 3]);
        assert_eq!(region_of_own(&o), Region::Unique);
    }

    #[test]
    fn plain_is_gc() {
        let v = vec![1, 2, 3];
        assert_eq!(region_of(&v), Region::Gc);
    }

    #[test]
    fn gc_fallback_for_unannotated() {
        let data = vec![
            Value::Owned(Own::new(1)),
            Value::Shared(2),
            Value::Shared(3),
        ];
        assert_eq!(
            infer_regions(&data),
            vec![Region::Unique, Region::Gc, Region::Gc]
        );
    }

    #[test]
    fn value_region_helper() {
        let owned = Value::Owned(Own::new(String::from("x")));
        let shared = Value::Shared(String::from("y"));
        assert_eq!(owned.region(), Region::Unique);
        assert_eq!(shared.region(), Region::Gc);
    }
}
