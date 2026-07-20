//! Phase 5 — issue #21: epoch model (one atomic release, no version resolution).
//!
//! Axiom ships in **epochs**: a single atomic release bundles compiler +
//! stdlib + modules. There is **no semantic-version resolution** — every
//! module in an epoch is pinned to that epoch, so a build either fully
//! resolves to one epoch or fails outright. This module is the (pure-Rust,
//! unit-tested) resolver that enforces that invariant.
//! See `TRACKING.md` Phase 5 and issue #21.

/// An epoch: a named, atomic unit of release.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Epoch {
    pub name: String,
}

impl Epoch {
    pub fn new(name: &str) -> Self {
        Epoch {
            name: name.to_string(),
        }
    }
}

/// A module declared in `axiom.toml`, pinned to a required epoch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDecl {
    pub name: String,
    pub requires_epoch: Epoch,
}

/// A project manifest: the epoch it targets + its modules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub epoch: Epoch,
    /// Compiler version string (informational; not resolved against).
    pub compiler: String,
    pub modules: Vec<ModuleDecl>,
}

/// Outcome of resolving a manifest's epoch closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolve {
    /// Every module agrees on the epoch — the build is atomic.
    Atomic(Epoch),
    /// Version resolution is forbidden; these modules require other epochs.
    Conflict(Vec<String>),
}

impl Resolve {
    pub fn is_atomic(&self) -> bool {
        matches!(self, Resolve::Atomic(_))
    }
}

/// Resolve a manifest.
///
/// Because Axiom has **no version resolution**, every module must require
/// exactly the manifest's epoch; any mismatch is a hard `Conflict`
/// (no fallback, no backtracking). This is what makes an epoch a single,
/// coherent release.
pub fn resolve(m: &Manifest) -> Resolve {
    let mut conflicts = Vec::new();
    for module in &m.modules {
        if module.requires_epoch != m.epoch {
            conflicts.push(module.name.clone());
        }
    }
    if conflicts.is_empty() {
        Resolve::Atomic(m.epoch.clone())
    } else {
        Resolve::Conflict(conflicts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(epoch: &str, mods: &[(&str, &str)]) -> Manifest {
        Manifest {
            epoch: Epoch::new(epoch),
            compiler: "0.0.0".into(),
            modules: mods
                .iter()
                .map(|(n, e)| ModuleDecl {
                    name: n.to_string(),
                    requires_epoch: Epoch::new(e),
                })
                .collect(),
        }
    }

    #[test]
    fn single_epoch_is_atomic() {
        let m = manifest("e1", &[("json", "e1"), ("os", "e1")]);
        match resolve(&m) {
            Resolve::Atomic(e) => assert_eq!(e, Epoch::new("e1")),
            Resolve::Conflict(c) => panic!("unexpected conflict: {:?}", c),
        }
        assert!(resolve(&m).is_atomic());
    }

    #[test]
    fn mismatched_epoch_is_conflict() {
        // os requires e2 while the manifest targets e1 -> forbidden
        let m = manifest("e1", &[("json", "e1"), ("os", "e2")]);
        match resolve(&m) {
            Resolve::Conflict(c) => assert_eq!(c, vec!["os".to_string()]),
            Resolve::Atomic(e) => panic!("should conflict, got atomic {:?}", e),
        }
        assert!(!resolve(&m).is_atomic());
    }

    #[test]
    fn no_modules_is_atomic() {
        let m = manifest("e3", &[]);
        assert!(matches!(resolve(&m), Resolve::Atomic(_)));
    }

    #[test]
    fn all_wrong_epochs_listed() {
        let m = manifest("e1", &[("a", "e9"), ("b", "e9")]);
        match resolve(&m) {
            Resolve::Conflict(c) => assert_eq!(c.len(), 2),
            _ => panic!("expected conflict"),
        }
    }
}
