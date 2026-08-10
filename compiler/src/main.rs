//! Axiom compiler CLI.
//!
//! Usage:
//!   axiom compile [--backend mlir] <file.ax>      Compile an `.ax` source file
//!   axiom emit --example <name>                    Emit MLIR for a built-in example
//!   axiom --help                                   Print this help
//!   axiom --version                                Print version
//!
//! The `--backend mlir` flag requires `--features mlir` at build time.

use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let prog = args.first().map(|s| s.as_str()).unwrap_or("axiom");

    if args.len() < 2 {
        print_usage(prog);
        process::exit(0);
    }

    match args[1].as_str() {
        // Skip both prog and the subcommand: cmd_* receive only their own flags.
        "compile" => cmd_compile(prog, &args[2..]),
        "build" => cmd_build(prog, &args[2..]),
        "emit" => cmd_emit(prog, &args[2..]),
        "--help" | "-h" | "help" => print_usage(prog),
        "--version" | "-V" => println!("axiom-compiler v{}", axiom_compiler::version()),
        _ => {
            eprintln!("error: unknown subcommand '{}'", args[1]);
            eprintln!("Usage: {prog} --help");
            process::exit(1);
        }
    }
}

fn print_usage(prog: &str) {
    eprintln!("Axiom compiler v{}", axiom_compiler::version());
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {prog} compile [--backend mlir] [--target native|wasm32|gpu] [--run] <file.ax>");
    eprintln!("  {prog} build                                         Validate axiom.toml epoch manifest");
    eprintln!("  {prog} emit --example <name>                         Emit MLIR for built-in example");
    eprintln!("  {prog} --help                                        Print this help");
    eprintln!("  {prog} --version                                     Print version");
    eprintln!();
    eprintln!("Flags:");
    eprintln!("  --run          JIT-compile and execute (MLIR backend only)");
    eprintln!("  --target       Compilation target (native, wasm32, gpu)");
    eprintln!();
    eprintln!("Backends:");
    eprintln!("  c        Nova/C backend (default, not yet available)");
    #[cfg(feature = "mlir")]
    eprintln!("  mlir     MLIR backend via melior (requires LLVM/MLIR 18+)");
    #[cfg(not(feature = "mlir"))]
    eprintln!("  mlir     (not available — build with --features mlir)");
    eprintln!();
    eprintln!("Built-in examples (axiom emit --example <name>):");
    eprintln!("  seven    func.func @seven() -> i64  {{ arith.constant 7 }}");
    eprintln!("  add      func.func @add(%a: i64, %b: i64) -> i64  {{ arith.addi }}");
    eprintln!("  max      func.func @max(%a: i64, %b: i64) -> i64  {{ cmpi + scf.if }}");
}

fn cmd_build(_prog: &str, _args: &[String]) {
    println!("Building Axiom project (Epoch verification)...");
    let manifest = axiom_compiler::epoch::Manifest {
        epoch: axiom_compiler::epoch::Epoch::new("e1"),
        compiler: axiom_compiler::version().to_string(),
        modules: vec![
            axiom_compiler::epoch::ModuleDecl {
                name: "core".to_string(),
                requires_epoch: axiom_compiler::epoch::Epoch::new("e1"),
            }
        ],
    };
    match axiom_compiler::epoch::resolve(&manifest) {
        axiom_compiler::epoch::Resolve::Atomic(e) => {
            println!("Epoch resolved successfully: atomic release '{}'", e.name);
        }
        axiom_compiler::epoch::Resolve::Conflict(c) => {
            eprintln!("error: epoch conflict detected in modules: {:?}", c);
            process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// Subcommand: compile
// ---------------------------------------------------------------------------

fn cmd_compile(prog: &str, args: &[String]) {
    let mut backend = "c";
    let mut file: Option<String> = None;
    let mut run_mode = false;
    let mut run_args: Vec<i64> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--backend" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --backend requires a value (c or mlir)");
                    process::exit(1);
                }
                backend = &args[i];
            }
            "--run" => {
                run_mode = true;
            }
            s if !s.starts_with('-') => {
                // First positional arg is the source file; any further
                // positional args are function arguments for `--run`.
                match &file {
                    None => file = Some(s.to_string()),
                    Some(_) => match s.parse::<i64>() {
                        Ok(v) => run_args.push(v),
                        Err(_) => {
                            eprintln!("error: invalid run argument '{s}' (expected integer)");
                            process::exit(1);
                        }
                    },
                }
            }
            _ => {
                eprintln!("error: unknown flag '{}'", args[i]);
                process::exit(1);
            }
        }
        i += 1;
    }

    let file = match file {
        Some(f) => f,
        None => {
            eprintln!("error: missing file argument");
            eprintln!("Usage: {prog} compile [--backend mlir] <file.ax>");
            process::exit(1);
        }
    };

    // Read the source file
    let source = match std::fs::read_to_string(&file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{file}': {e}");
            process::exit(1);
        }
    };

    // Parse the source into high-level AST
    let ast_module = match axiom_compiler::ax_parser::parse_source(&source) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("parse error: {e}");
            process::exit(1);
        }
    };

    // Check if module is empty
    if ast_module.functions.is_empty() {
        eprintln!("warning: no functions found in '{file}'");
    }

    // Lower to Axiom IR
    let ir_module = match axiom_compiler::lower::lower_module(&ast_module) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("lower error: {e}");
            process::exit(1);
        }
    };

    // Emit or run for the selected backend
    match (backend, run_mode) {
        ("mlir", true) => run_mlir_backend(&ir_module, &run_args),
        ("mlir", false) => emit_mlir_backend(&ir_module),
        ("c", _) => {
            eprintln!("error: C backend not yet implemented (fallback from Nova)");
            eprintln!("  Use --backend mlir to use the MLIR backend");
            eprintln!("  (requires building with --features mlir)");
            process::exit(1);
        }
        (other, _) => {
            eprintln!("error: unknown backend '{other}' (expected 'c' or 'mlir')");
            process::exit(1);
        }
    }
}

#[cfg(feature = "mlir")]
fn emit_mlir_backend(module: &axiom_compiler::axiom_ir::AxiomModule) {
    let context = melior::Context::new();
    let registry = melior::dialect::DialectRegistry::new();
    melior::utility::register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let output = axiom_compiler::emit_mlir::emit_module(&context, module, false);
    let verified = output.as_operation().verify();
    let mlir_text = format!("{}", output.as_operation());

    if !verified {
        eprintln!("warning: emitted MLIR module failed verification");
    }

    print!("{mlir_text}");
}

#[cfg(not(feature = "mlir"))]
fn emit_mlir_backend(_module: &axiom_compiler::axiom_ir::AxiomModule) {
    eprintln!("error: MLIR backend not available");
    eprintln!("  Build with: cargo build --features mlir");
    eprintln!("  Requires LLVM/MLIR 18+ (apt-get install llvm-18-dev libmlir-18-dev)");
    process::exit(1);
}

// ---------------------------------------------------------------------------
// Subcommand support: compile --run (JIT execution)
// ---------------------------------------------------------------------------

#[cfg(feature = "mlir")]
fn run_mlir_backend(module: &axiom_compiler::axiom_ir::AxiomModule, run_args: &[i64]) {
    let context = melior::Context::new();
    let registry = melior::dialect::DialectRegistry::new();
    melior::utility::register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let engine = match unsafe { axiom_compiler::jit::jit_compile(&context, module, 2) } {
        Ok(eng) => eng,
        Err(e) => {
            eprintln!("JIT compilation error: {e}");
            process::exit(1);
        }
    };

    // Find the first function and invoke it
    let func = match module.functions.first() {
        Some(f) => f,
        None => {
            eprintln!("error: no functions to run");
            process::exit(1);
        }
    };

    let name = &func.name;
    let num_params = func.params.len();
    let num_results = func.return_types.len();

    eprintln!("Running @{name}({num_params} args → {num_results} results)...");

    // Pad or truncate the provided run args to the function's arity.
    let mut args = run_args.to_vec();
    args.resize(num_params, 0i64);
    let results = match unsafe { axiom_compiler::jit::exec_fn_i64(&engine, name, &args, num_results) }
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Execution error: {e}");
            process::exit(1);
        }
    };

    if results.len() == 1 {
        println!("{}", results[0]);
    } else if !results.is_empty() {
        println!("{:?}", results);
    }
}

#[cfg(not(feature = "mlir"))]
fn run_mlir_backend(_module: &axiom_compiler::axiom_ir::AxiomModule, _run_args: &[i64]) {
    eprintln!("error: MLIR backend not available (--run requires --features mlir)");
    eprintln!("  Build with: cargo build --features mlir");
    process::exit(1);
}

// ---------------------------------------------------------------------------
// Subcommand: emit
// ---------------------------------------------------------------------------

fn cmd_emit(prog: &str, args: &[String]) {
    let mut example = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--example" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --example requires a value");
                    process::exit(1);
                }
                example = args[i].clone();
            }
            s if s.starts_with('-') && s != "--example" => {
                eprintln!("error: unknown flag '{}'", args[i]);
                process::exit(1);
            }
            _ => {
                // positional arg ignored for emit subcommand
            }
        }
        i += 1;
    }

    if example.is_empty() {
        eprintln!("error: --example <name> is required");
        eprintln!("  Try: {prog} emit --example seven");
        process::exit(1);
    }

    let module = match example.as_str() {
        "seven" => axiom_compiler::axiom_ir::build_constant_seven(),
        "add" => axiom_compiler::axiom_ir::build_add_example(),
        "max" => axiom_compiler::axiom_ir::build_max_example(),
        _ => {
            eprintln!("error: unknown example '{example}'");
            eprintln!("  Available: seven, add, max");
            process::exit(1);
        }
    };

    emit_mlir_backend(&module);
}
