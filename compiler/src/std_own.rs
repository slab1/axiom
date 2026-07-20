//! Phase 3 — good-first issue #16: apply `own` / `borrow` to a std container.
//!
//! `OwnedVec<T>` is a growable container that *uniquely owns* its buffer
//! (`own<T>`-style) and hands out `borrow<T>` read-only views. Under Axiom's
//! GC default most containers are shared; this shows the **opt-in ownership**
//! path on a familiar std-like type. See `TRACKING.md` Phase 3 and issue #16.

use crate::ownership::{Borrow, Own};

/// A vector that owns its backing buffer uniquely.
///
/// Pushing moves values in; reading yields `borrow` views so callers can't
/// alias the buffer while it is uniquely owned.
pub struct OwnedVec<T> {
    buf: Own<Vec<T>>,
}

impl<T> OwnedVec<T> {
    pub fn new() -> Self {
        OwnedVec { buf: Own::new(Vec::new()) }
    }

    /// Move `value` into the uniquely-owned buffer.
    pub fn push(&mut self, value: T) {
        self.buf.0.push(value);
    }

    pub fn len(&self) -> usize {
        self.buf.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.0.is_empty()
    }

    /// Hand out a borrowed, read-only view. Safe: the buffer is owned here,
    /// so the borrow cannot race with mutation at this site.
    pub fn view(&self) -> Borrow<Vec<T>> {
        Borrow::new(&self.buf.0)
    }

    /// Consume and move the inner `Vec` out (ownership transfer).
    pub fn into_inner(self) -> Vec<T> {
        self.buf.into_inner()
    }
}

impl<T: Clone> OwnedVec<T> {
    /// Explicit deep copy at an ownership boundary (the GC default would
    /// otherwise share the buffer).
    pub fn clone_buf(&self) -> Vec<T> {
        self.buf.clone_value()
    }
}

impl<T> Default for OwnedVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_len() {
        let mut v = OwnedVec::new();
        v.push(1);
        v.push(2);
        assert_eq!(v.len(), 2);
        assert!(!v.is_empty());
    }

    #[test]
    fn view_reads_through() {
        let mut v = OwnedVec::new();
        v.push("a");
        let view = v.view();
        assert_eq!(view.get()[0], "a");
    }

    #[test]
    fn into_inner_moves_ownership() {
        let mut v = OwnedVec::new();
        v.push(7);
        assert_eq!(v.into_inner(), vec![7]);
    }

    #[test]
    fn clone_buf_is_independent() {
        let mut v = OwnedVec::new();
        v.push(1);
        let c = v.clone_buf();
        assert_eq!(c, vec![1]);
        // original is intact and still usable
        assert_eq!(v.len(), 1);
    }
}
