//! Phase 1 — JIT compilation and execution engine for Axiom.
//!
//! Takes an `axiom_ir::AxiomModule`, emits MLIR via `emit_mlir`,
//! lowers through the func/arith/scf → LLVM dialect conversion
//! pipeline, and JIT-compiles via melior's `ExecutionEngine`.
//!
//! Feature-gated (`#[cfg(feature = "mlir")]`) — requires LLVM/MLIR 18+.

use melior::{
    Context,
    pass::{Pass, PassManager},
    utility::{register_all_llvm_translations, register_all_passes},
    ExecutionEngine,
};
use mlir_sys::{
    mlirCreateConversionArithToLLVMConversionPass,
    mlirCreateConversionConvertControlFlowToLLVMPass,
    mlirCreateConversionConvertFuncToLLVMPass,
    mlirCreateConversionFinalizeMemRefToLLVMConversionPass,
    mlirCreateConversionReconcileUnrealizedCasts,
    mlirCreateConversionSCFToControlFlow,
};

use crate::axiom_ir;
use crate::emit_mlir;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during JIT compilation or execution.
#[derive(Debug)]
pub struct JitError {
    pub message: String,
}

impl std::fmt::Display for JitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JIT error: {}", self.message)
    }
}

impl std::error::Error for JitError {}

impl From<String> for JitError {
    fn from(msg: String) -> Self {
        JitError { message: msg }
    }
}

impl From<&str> for JitError {
    fn from(msg: &str) -> Self {
        JitError {
            message: msg.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compile an Axiom IR module and return an `ExecutionEngine`.
///
/// The returned engine can be used to invoke functions from the module
/// via [`ExecutionEngine::invoke_packed`].
///
/// The emitted functions are tagged with `llvm.emit_c_interface` so that
/// `invoke_packed` can call them.
///
/// # Safety
///
/// The caller must ensure the `context` outlives the returned engine.
pub unsafe fn jit_compile<'c>(
    context: &'c Context,
    module: &axiom_ir::AxiomModule,
    opt_level: usize,
) -> Result<ExecutionEngine, JitError> {
    // 1. Emit MLIR with llvm.emit_c_interface attribute
    let mut mlir_module = emit_mlir::emit_module(context, module, true);

    // 2. Verify the emitted module
    if !mlir_module.as_operation().verify() {
        return Err("MLIR module verification failed before lowering".into());
    }

    // 3. Register all passes and LLVM translations
    register_all_passes();
    register_all_llvm_translations(context);

    // 4. Build the LLVM conversion pass pipeline
    //
    // The ordering matters:
    //   a) SCF → ControlFlow       (module level)
    //   b) arith → LLVM            (func.func level)
    //   c) ControlFlow → LLVM      (module level)
    //   d) func → LLVM             (module level)
    //   e) ReconcileUnrealizedCasts (module level)
    //   f) FinalizeMemRefToLLVM    (module level)
    let pm = PassManager::new(context);

    // SCF → ControlFlow (module level — walks function bodies internally)
    pm.add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionSCFToControlFlow) });

    // arith → LLVM (runs on func.func bodies)
    pm.nested_under("func.func")
        .add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionArithToLLVMConversionPass) });

    // ControlFlow → LLVM (module level — runs on cf ops in function bodies)
    pm.add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionConvertControlFlowToLLVMPass) });

    // func → LLVM (module level — converts func.func to llvm.func)
    pm.add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionConvertFuncToLLVMPass) });

    // Remove any unrealized_conversion_cast ops
    pm.add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionReconcileUnrealizedCasts) });

    // Finalize any remaining memref descriptors
    pm.add_pass(unsafe { Pass::from_raw_fn(mlirCreateConversionFinalizeMemRefToLLVMConversionPass) });

    // 5. Run the pass pipeline
    pm.run(&mut mlir_module)
        .map_err(|e| JitError {
            message: format!("LLVM conversion pipeline failed: {e:?}"),
        })?;

    // 6. Create the ExecutionEngine
    let engine = ExecutionEngine::new(&mlir_module, opt_level, &[], false);

    Ok(engine)
}

// ---------------------------------------------------------------------------
// Convenience: invoke a function with i64 arguments/results
// ---------------------------------------------------------------------------

/// Invoke a function that uses (or can be coerced to) `i64` arguments and
/// results.
///
/// `args` provides input i64 values. `num_results` specifies how many
/// result pointers to append after the arguments.
///
/// Returns the result values as a `Vec<i64>`.
///
/// # Safety
///
/// This calls into JIT-compiled native code. The function **must** have
/// the `llvm.emit_c_interface` attribute. Misaligned or invalid pointers
/// cause undefined behaviour.
pub unsafe fn exec_fn_i64(
    engine: &ExecutionEngine,
    name: &str,
    args: &[i64],
    num_results: usize,
) -> Result<Vec<i64>, JitError> {
    let total = args.len() + num_results;
    let mut raw_args: Vec<*mut ()> = Vec::with_capacity(total);

    // Box each argument so we have stable pointers
    let mut arg_boxes: Vec<Box<i64>> = args.iter().map(|&a| Box::new(a)).collect();
    let mut result_boxes: Vec<Box<i64>> = (0..num_results).map(|_| Box::new(0i64)).collect();

    for arg in &mut arg_boxes {
        raw_args.push(arg.as_mut() as *mut i64 as *mut ());
    }
    for result in &mut result_boxes {
        raw_args.push(result.as_mut() as *mut i64 as *mut ());
    }

    // SAFETY: invoke_packed calls JIT'd native code; pointers are valid i64 boxes
    unsafe {
        engine
            .invoke_packed(name, &mut raw_args)
            .map_err(|e| JitError {
                message: format!("invoke `{name}` failed: {e:?}"),
            })?;
    }

    Ok(result_boxes.iter().map(|b| *b.clone()).collect())
}

// ---------------------------------------------------------------------------
// I/O symbol registration
// ---------------------------------------------------------------------------

unsafe extern "C" {
    fn putchar(c: i32) -> i32;
    fn getchar() -> i32;
    fn printf(fmt: *const u8, ...) -> i32;
}

/// Register C standard library I/O symbols with the execution engine so
/// that JIT-compiled code can call `putchar`, `getchar`, and `printf`.
///
/// # Safety
///
/// Registers raw function pointers. The engine will call them directly.
pub unsafe fn register_io_symbols(engine: &ExecutionEngine) {
    // SAFETY: raw function pointers to libc symbols; valid for entire engine lifetime
    unsafe {
        engine.register_symbol("putchar", putchar as *mut ());
        engine.register_symbol("getchar", getchar as *mut ());
        engine.register_symbol("printf", printf as *mut ());
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use melior::utility::register_all_dialects;
    use melior::dialect::DialectRegistry;

    fn create_context() -> Context {
        let context = Context::new();
        let registry = DialectRegistry::new();
        register_all_dialects(&registry);
        context.append_dialect_registry(&registry);
        context.load_all_available_dialects();
        context
    }

    #[test]
    fn jit_compile_constant_seven() {
        let context = create_context();
        let module = axiom_ir::build_constant_seven();

        let engine = unsafe { jit_compile(&context, &module, 2).unwrap() };

        // @seven() -> i64 returns 7
        let results = unsafe { exec_fn_i64(&engine, "seven", &[], 1).unwrap() };
        assert_eq!(results, vec![7]);
    }

    #[test]
    fn jit_compile_add() {
        let context = create_context();
        let module = axiom_ir::build_add_example();

        let engine = unsafe { jit_compile(&context, &module, 2).unwrap() };

        // @add(%a: i64, %b: i64) -> i64 returns a + b
        let results = unsafe { exec_fn_i64(&engine, "add", &[3, 7], 1).unwrap() };
        assert_eq!(results, vec![10]);
    }

    #[test]
    fn jit_compile_max() {
        let context = create_context();
        let module = axiom_ir::build_max_example();

        let engine = unsafe { jit_compile(&context, &module, 2).unwrap() };

        // @max(%a: i64, %b: i64) -> i64 returns the larger
        let results = unsafe { exec_fn_i64(&engine, "max", &[100, 42], 1).unwrap() };
        assert_eq!(results, vec![100]);

        let results = unsafe { exec_fn_i64(&engine, "max", &[7, 99], 1).unwrap() };
        assert_eq!(results, vec![99]);
    }
}
