//! Axiom compiler front-end.
//!
//! Phase 0: forked from Nova's `compiler-codegen`. The parser, type-checker,
//! and effect-inference modules are kept; `emit_c.rs` remains the reference
//! backend until `emit_mlir.rs` reaches parity.
//!
//! See crate root README / TRACKING.md for the phased plan.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

// Phase 1: MLIR backend (feature-gated; requires LLVM/MLIR 18+).
#[cfg(feature = "mlir")]
pub mod emit_mlir;

// Phase 2: automatic parallelism pass (feature-gated; requires hvm-core).
#[cfg(feature = "parallel")]
pub mod parallelize;
