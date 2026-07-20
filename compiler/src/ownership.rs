//! Axiom gradual-ownership types (Phase 3 core).
//!
//! Axiom defaults to garbage collection. Ownership is **opt-in** via the
//! `own<T>` / `borrow<T>` markers, which the region-inference pass (issue #14)
//! reads to decide where to insert moves/frees vs. where to keep GC. These are
//! zero-cost newtypes so they compile and are unit-tested in the default
//! `cargo test` run. See `TRACKING.md` Phase 3 and issues #13/#14/#15/#16.
//!
//! Design note: unlike Rust, Axiom does not *require* ownership annotations.
//! They are a performance/FFI affordance layered on top of the GC default, so
//! most code simply never mentions them.

/// Opt-in unique ownership over `T`.
///
/// Analogous to a move-only `Box<T>`: the value is owned here and must be
/// moved out (via [`Own::into_inner`]) or explicitly cloned at a boundary.
/// The region pass treats `own` values as non-aliased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Own<T>(pub T);

impl<T> Own<T> {
    /// Wrap a value as uniquely owned.
    pub fn new(value: T) -> Self {
        Own(value)
    }

    /// Move the inner value out.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Clone> Own<T> {
    /// Explicit clone point. Under the GC default most values are shared; `own`
    /// forces a deep copy at this boundary so the moved-out value is independent.
    pub fn clone_value(&self) -> T {
        self.0.clone()
    }
}

/// Opt-in borrowed reference over `T`.
///
/// Axiom's GC model does not track Rust-style lifetimes; `borrow` is a
/// documentation/hint marker the region pass uses to confirm a value is not
/// uniquely owned at a use site. It is a thin reference newtype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Borrow<'a, T>(&'a T);

impl<'a, T> Borrow<'a, T> {
    /// Borrow `value` for the lifetime `'a`.
    pub fn new(value: &'a T) -> Self {
        Borrow(value)
    }

    /// Read through the borrowed reference.
    pub fn get(&self) -> &'a T {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_wraps_and_unwraps() {
        let o = Own::new(42);
        assert_eq!(o.into_inner(), 42);
    }

    #[test]
    fn own_clones_value_independently() {
        let o = Own::new(vec![1, 2, 3]);
        let cloned = o.clone_value();
        assert_eq!(cloned, vec![1, 2, 3]);
        // the original is still intact and movable
        assert_eq!(o.into_inner(), vec![1, 2, 3]);
    }

    #[test]
    fn borrow_reads_through() {
        let v = String::from("hi");
        let b = Borrow::new(&v);
        assert_eq!(b.get(), &String::from("hi"));
    }

    #[test]
    fn ownership_markers_compose() {
        let owned = Own::new(10);
        let inner = owned.into_inner();
        let b = Borrow::new(&inner);
        assert_eq!(*b.get(), 10);
    }

    #[test]
    fn own_is_clone_when_t_is_clone() {
        let a = Own::new(5);
        let b = a.clone();
        assert_eq!(a, b);
    }
}
