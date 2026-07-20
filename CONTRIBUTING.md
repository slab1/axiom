# Contributing to Axiom

Thanks for your interest in building the next-generation systems language!

Axiom is a **synthesis** of proven ideas, not a from-scratch experiment. We start from
Nova's working compiler front-end and add four layers (MLIR backend, automatic
parallelism, gradual ownership, MCP time-travel). That means **most contributions are
well-scoped and verifiable against an existing test suite** — you don't need to design a
language to help.

## Architecture at a glance

```
axiom/
├── compiler/            # Forked from Nova: parser, type-checker, effect inference
│   ├── src/
│   │   ├── parser/      # recursive-descent parser (keep)
│   │   ├── types/       # type-checker + effect inference (keep, extend)
│   │   ├── effect_surface.rs  # capability enforcement (keep)
│   │   ├── emit_c.rs    # C backend (keep as fallback)
│   │   ├── emit_mlir.rs # NEW: MLIR backend (Phase 1)
│   │   ├── parallelize.rs    # NEW: purity → parallelism pass (Phase 2)
│   │   └── ownership.rs  # NEW: gradual ownership (Phase 3)
│   └── nova_rt/         # C runtime: effects, fibers, GC (keep/extend)
├── axiom-trace/         # NEW: MCP time-travel server (Phase 4)
├── std/                 # standard library (.ax sources)
├── examples/            # .ax example programs
└── tests/               # EXPECT-marker test suite (from Nova)
```

## Development setup

```bash
# 1. Install Rust (edition 2024, MSRV 1.85+)
rustup toolchain install stable

# 2. For the MLIR backend (Phase 1+), install LLVM/MLIR:
#    see https://mlir.llvm.org/getting_started/  (LLVM 18+ with MLIR)
#    melior links against the MLIR C API.

# 3. Build
cargo build --release

# 4. Run the test suite (EXPECT-marker based, from Nova)
cargo test
# or via the CLI once wired:
./target/release/axiom test
```

## Workflow

1. **Pick a phased issue.** Each Phase (0–5) in [README.md](README.md) has tracking
   issues. Start with "good first issue" labels.
2. **Keep C backend green.** Until MLIR reaches parity, `emit_c` is the reference. Any
   change must not break C-backend tests.
3. **One logical change per PR.** Small, reviewable, independently verifiable.
4. **Tests required.** Add an `EXPECT`-marker test (`.ax` file) or a Rust unit test for
   any new construct or pass.
5. **Sign your commits** (`git commit -s`) — DCO is enforced.

## Contribution areas

| Area | Skill needed | Where to start |
|------|--------------|----------------|
| MLIR emission | Rust + basic MLIR dialects (`arith`, `scf`, `func`) | port one `emit_c` construct to `emit_mlir` |
| Parallelism pass | data-flow / purity analysis | write purity-detection tests for `parallelize.rs` |
| Ownership | type theory / regions | add `own`/`borrow` to one std type |
| MCP time-travel | Rust + `rmcp` | add one tool to `axiom-trace` |
| Docs / examples | writing | add an `.ax` example demonstrating an effect |

## Design principles (non-negotiable)

1. **Effects in the signature.** Side effects are always visible in the type. No hidden
   control flow, no hidden allocation (Nova/Zig rule).
2. **Capability by default.** `forbid = ["Net"]` at the dependency boundary is a compile
   error, not a runtime check.
3. **Purity is leverage.** An empty effect row means *parallelizable*. The compiler must
   exploit this; it must never ask the programmer to re-annotate it.
4. **AI-first surface.** No macros, no conditional compilation. Fully visible source so
   generated code is deterministic and reviewable.
5. **Verifiable, not magical.** Every new pass ships with a test that fails without it.

## Code of conduct

Be respectful, assume good faith, and keep discussions focused on the design principles
above. We are building a language meant to outlast us — argue for the long term.
