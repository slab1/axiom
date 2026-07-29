//! End-to-end integration tests for the Axiom compiler.
//!
//! Tests parsing expression-based Axiom source code, lowering to Axiom IR,
//! purity/parallelism analysis, and (when `--features mlir` is enabled) emitting,
//! verifying, and JIT-executing code.

use axiom_compiler::ax_parser;
use axiom_compiler::lower;

#[test]
fn test_parse_and_lower_expressions() {
    let sources = [
        "fn seven() -> I64 { 7 }",
        "fn add(a: I64, b: I64) -> I64 { a + b }",
        "fn max(a: I64, b: I64) -> I64 { if a > b { a } else { b } }",
        "fn fib(n: I64) -> I64 { if n <= 1 { n } else { fib(n - 1) + fib(n - 2) } }",
        "fn example(x: I64) -> I64 { let y = x + 1; y }",
        "fn compute(a: I64, b: I64) -> I64 { a + b * 2 }",
    ];

    for source in &sources {
        let ast_module = ax_parser::parse_source(source)
            .unwrap_or_else(|e| panic!("failed to parse source '{source}': {e}"));

        assert!(!ast_module.functions.is_empty(), "module should have functions for source '{source}'");

        let ir_module = lower::lower_module(&ast_module)
            .unwrap_or_else(|e| panic!("failed to lower source '{source}': {e}"));

        assert!(!ir_module.functions.is_empty(), "IR module should have functions for source '{source}'");
    }
}

#[test]
fn test_module_parallelism_analysis() {
    let source = "fn add(a: I64, b: I64) -> I64 { a + b }";
    let ast_module = ax_parser::parse_source(source).unwrap();
    let ir_module = lower::lower_module(&ast_module).unwrap();
    let parallelism = lower::analyze_module_parallelism(&ir_module);
    assert_eq!(parallelism, vec![("add".to_string(), true)]);
}

#[cfg(feature = "mlir")]
#[test]
fn test_mlir_emission_expressions() {
    use melior::{
        Context,
        dialect::{DialectRegistry},
        utility::register_all_dialects,
    };
    use axiom_compiler::emit_mlir;

    let context = Context::new();
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let sources = [
        "fn seven() -> I64 { 7 }",
        "fn add(a: I64, b: I64) -> I64 { a + b }",
        "fn max(a: I64, b: I64) -> I64 { if a > b { a } else { b } }",
        "fn example(x: I64) -> I64 { let y = x + 1; y }",
    ];

    for source in &sources {
        let ast_module = ax_parser::parse_source(source).unwrap();
        let ir_module = lower::lower_module(&ast_module).unwrap();

        let mlir_module = emit_mlir::emit_module(&context, &ir_module, false);
        assert!(mlir_module.as_operation().verify(), "MLIR module verification failed for source '{source}'");
    }
}

#[cfg(feature = "mlir")]
#[test]
fn test_jit_execution_source() {
    use melior::{
        Context,
        dialect::{DialectRegistry},
        utility::register_all_dialects,
    };
    use axiom_compiler::jit;

    let context = Context::new();
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let source = "fn add(a: I64, b: I64) -> I64 { a + b }";
    let ast_module = ax_parser::parse_source(source).unwrap();
    let ir_module = lower::lower_module(&ast_module).unwrap();

    let engine = unsafe { jit::jit_compile(&context, &ir_module, 2).unwrap() };
    let results = unsafe { jit::exec_fn_i64(&engine, "add", &[15, 27], 1).unwrap() };
    assert_eq!(results, vec![42]);
}
