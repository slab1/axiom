//! Axiom MLIR backend (Phase 1).
//!
//! This is the **starter** for `emit_mlir.rs` — the drop-in replacement for
//! Nova's `emit_c.rs`. It demonstrates the minimal viable emission path using
//! [`melior`] (Rust bindings to the MLIR C API) so contributors have a concrete
//! first target. See `TRACKING.md` Phase 1 and issue #3/#4/#7.
//!
//! Build with: `cargo build -p axiom-compiler --features mlir`
//! (requires LLVM/MLIR 18+ with the C API on the system).
//!
//! The design mirrors Nova's two-stage pipeline:
//!   parse -> type-check + effect inference (kept from Nova)
//!        -> emit_mlir (THIS) -> MLIR -> LLVM IR -> native binary
//!
//! We deliberately keep `emit_c` as the reference backend until every
//! construct emitted here also passes the EXPECT-marker test suite.
#![cfg(feature = "mlir")]

use melior::{
    Context,
    dialect::{arith, func, DialectRegistry},
    ir::{
        attribute::{StringAttribute, TypeAttribute},
        r#type::FunctionType,
        Block, Location, Module, Region, Type,
    },
    utility::register_all_dialects,
};

/// Emit a standalone Axiom function `add(a, b) -> a + b` as MLIR.
///
/// This is the **first construct** to port (issue #7). It proves the whole
/// chain works: context -> module -> func -> arith -> llvm translation.
///
/// Produces roughly:
/// ```mlir
/// module {
///   func.func @add(%arg0: i64, %arg1: i64) -> i64 {
///     %0 = arith.addi %arg0, %arg1 : i64
///     return %0 : i64
///   }
/// }
/// ```
pub fn emit_add_example() -> Module {
    let context = Context::new();
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let location = Location::unknown(&context);
    let module = Module::new(location);

    let i64 = Type::integer(&context, 64);

    module.body().append_operation(func::func(
        &context,
        StringAttribute::new(&context, "add"),
        TypeAttribute::new(FunctionType::new(&context, &[i64, i64], &[i64]).into()),
        {
            let block = Block::new(&[(i64, location), (i64, location)]);
            let sum = block.append_operation(arith::addi(
                block.argument(0).unwrap().into(),
                block.argument(1).unwrap().into(),
                location,
            ));
            block.append_operation(func::r#return(
                &[sum.result(0).unwrap().into()],
                location,
            ));
            let region = Region::new();
            region.append_block(block);
            region
        },
        &[],
        location,
    ));

    module
}

/// Verify the emitted module is well-formed (catches malformed IR early,
/// the APXM-style "validate before lowering" principle from the MLIR case study).
pub fn emit_and_verify() -> Result<String, String> {
    let module = emit_add_example();
    module
        .as_operation()
        .verify()
        .map_err(|e| format!("MLIR verify failed: {:?}", e))?;
    Ok(module.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_valid_add_function() {
        let mlir = emit_and_verify().expect("module should verify");
        assert!(mlir.contains("func.func @add"), "missing func def: {}", mlir);
        assert!(mlir.contains("arith.addi"), "missing add op: {}", mlir);
        assert!(mlir.contains("i64"), "missing i64 type: {}", mlir);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// TODO(issue #7 — good first issue): port `arith.constant` emission.
//
// Second construct to implement, mirroring `emit_add_example`:
//
//   fn emit_constant_example() -> Module { ... }
//
// emitting roughly:
//   module {
//     func.func @seven() -> i64 {
//       %0 = arith.constant 7 : i64
//       return %0 : i64
//     }
//   }
//
// Sketch (melior 0.14 API; verify against the linked MLIR C API):
//   let i64 = Type::integer(&context, 64);
//   let c = block.append_operation(arith::constant(
//       &context,
//       IntegerAttribute::new(i64, 7).into(),  // value attribute
//       i64,                                  // result type
//       location,
//   ));
//
// PREREQUISITE: issue #3 — add `melior` to `compiler/Cargo.toml`
// (`melior = { version = "0.14", optional = true }`, enable the `mlir`
// feature) and install LLVM/MLIR 18+ with the C API on the host. Until
// then this file stays feature-gated and is NOT compiled by the default
// `cargo check --workspace` (which is why CI is green without LLVM).
// ─────────────────────────────────────────────────────────────────────────────
