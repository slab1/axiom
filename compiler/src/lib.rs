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

// Always-compiled, unit-tested core analyses (no external deps):
pub mod purity; // Phase 2: effect-row purity analysis
pub mod ownership; // Phase 3: opt-in own T / borrow T markers
pub mod std_own; // Phase 3 (#16): own/bgborrow on a std container
pub mod parallel; // Phase 2 (#8/#9): auto-parallelism analysis core
pub mod region; // Phase 3 (#14): region inference + GC fallback
pub mod epoch; // Phase 5 (#21): epoch model resolver
pub mod typecheck; // Phase 6: type-checking and effect inference

// Phase 1: Axiom IR (always compiled, pure types).
pub mod axiom_ir;

// Phase 1 (#5): High-level Axiom AST (expression-based, always compiled).
pub mod ast;

// Phase 1 (#5): Axiom source text parser (always compiled, no deps).
// Parses `.ax` text into `ast::Module`.
pub mod ax_parser;

// Phase 1 (#5): Lower high-level AST to Axiom IR (always compiled).
pub mod lower;

// Phase 1: MLIR backend (feature-gated; requires LLVM/MLIR 18+).
#[cfg(feature = "mlir")]
pub mod emit_mlir;

// Phase 1: JIT compilation engine (feature-gated; requires LLVM/MLIR 18+).
#[cfg(feature = "mlir")]
pub mod jit;

// Phase 2: automatic parallelism pass (feature-gated; requires hvm-core).
#[cfg(feature = "parallel")]
pub mod parallelize;
