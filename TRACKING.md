## Goal
- Build "Axiom" language (Nova-based) surpassing incumbents, publish contribution-ready repo at `https://github.com/slab1/axiom`, then install all deps and implement/fix the tracking issues with local verification.

## Constraints & Preferences
- Reuse proven mechanisms, not invent syntax from scratch.
- Repo must be contribution-ready (issues, CI, CONTRIBUTING, phased plan).
- GitHub PAT: `ghp_************************************` (account `slab1`).
- Design: effects in signature; capability by default (`forbid`); purity→parallelism; AI-first surface; verifiable passes.
- User directive: "install all dependences pick up issues and fix them" → install toolchain + LLVM/MLIR, implement verifiable issues.

## Progress
### Done
- Research + design + repo creation (23 issues #1–#23, CI workflow, virtual workspace, feature-gated heavy deps).
- CI green after fixing `rmcp` `stdlib` feature bug (removed heavy deps from default manifest so `cargo check/test` needs zero external deps).
- Installed Rust locally: cargo 1.97.1 / rustc 1.97.1 (aarch64) via rustup; local verification now possible (`source "$HOME/.cargo/env"`).
- Installed LLVM/MLIR 18 via apt: `llvm-18-dev` + `libmlir-18-dev` at `/usr/lib/llvm-18/`; `MLIRConfig.cmake` + `LLVMConfig.cmake` present; `mlir-c/` headers + `libMLIRCAPI*.a` + `libMLIR.so` present.
- Implemented & closed issues (all validated by green CI run `29768792763` + local `cargo test` = 36 tests: 29 compiler + 7 trace):
  - #12 purity.rs (Effect/EffectRow, 7 tests)
  - #13 ownership.rs (Own/Borrow, 5 tests)
  - #22 docs/EffectSurface.md
  - #23 CI green
  - #16 std_own.rs (OwnedVec<T>, 4 tests)
  - #8/#9 parallel.rs (analyze(), 5 tests)
  - #14 region.rs (Region Unique/Gc, 4 tests)
  - #17/#19 axiom-trace Ledger (7 tests, "total became 1422" scenario)
  - #18/#20 axiom-trace/src/mcp.rs (rmcp 0.1.5 server, --features mcp compiles clean)
  - #21 epoch.rs (Resolve Atomic/Conflict, 4 tests)
  - #15 examples/ffi_own.rs (runs: [C] received 42)
- TRACKING.md updated for all closed issues.
- LLVM 17 + 18 co-installed (llvm-17-dev + libmlir-18-dev); tblgen finds LLVM 17, mlir-sys finds LLVM 18.
- melior 0.14 + melior-macro 0.8.1 + tblgen 0.3.0 compile cleanly for `--features mlir`.
- Issue #4 (emit func+arith+scf): axiom_ir.rs built (ValueRef, Operation, Block, FunctionDef, Type), emit_mlir.rs rewritten to walk IR and emit func/arith/scf via melior.
- Issue #7 (arith.constant): included in emission pipeline (used by constant_seven example).
- Issue #5 (--backend mlir CLI flag): `src/main.rs` binary target with `compile` and `emit` subcommands; `src/ax_parser.rs` text parser converting `.ax` files to AxiomIR; full pipeline: `.ax` → parse → AxiomIR → emit MLIR output. Example `.ax` files in `examples/`.
- All 46 tests pass (33 default + 8 parse + 5 emit_mlir feature-gated).

### In Progress
- (none)

### Blocked
- (none)

## Key Decisions
- Virtual workspace + feature-gated heavy deps so default `cargo test` builds with zero external deps (turnkey for contributors).
- Always-compiled pure-Rust analysis modules (purity, ownership, std_own, parallel, region, epoch) unit-tested in default run.
- Axiom IR (`axiom_ir.rs`) always compiled (pure Rust types, no deps) so IR structure tests run in default `cargo test`.
- axiom-trace: pure-Rust Ledger always compiled + tested; rmcp MCP server behind --features mcp.
- rmcp 0.1.5 API: use `#[tool(tool_box)]` + `tool(param)` macro style (NOT `tool_router`/`Parameters` — those don't exist in 0.1.5); rmcp re-exports `schemars` but NOT `serde` → add serde dep gated by `mcp`.
- Edition 2024: FFI needs `unsafe extern "C"` + `#[unsafe(no_mangle)]` (linkage attrs are now unsafe attributes).
- LLVM/MLIR installed via apt; `libmlir-18-dev` is SEPARATE from `llvm-18-dev` (latter lacks MLIR).
- ValueStore uses lifetime-erased `Value<'c, 'static>` to store melior values across emit operations, using `::std::mem::transmute_copy` since the second lifetime is PhantomData and the MLIR Context outlives all blocks.
- `ValueRef` enum distinguishes block arguments from operation results to avoid slot collisions.

## Next Steps
- Consider #10 (hvm-core lowering), #11 (N-body benchmarks) — Phase 2.
- Consider #1/#2 (Nova fork + EXPECT tests) — Phase 0.
- Extend `ax_parser.rs` with more ops (scf.for, scf.while, arith.mulf, arith.subf).
- Extend `emit_mlir.rs` `build_scf_region` to support subi/muli/addf inside scf.if blocks (currently falls back to placeholder constants).
- Create end-to-end integration tests that emit MLIR, round-trip verify, and dump the MLIR text.
- Update CI to build + test with `--features mlir` on ubuntu (enable the LLVM install step).

## Critical Context
- Repo: https://github.com/slab1/axiom (public). Local: /tmp/opencode/axiom-lang.
- Push pattern: git remote set-url origin "https://${TOKEN}@github.com/slab1/axiom.git" && git push -u origin main then reset to token-free URL.
- Local Rust: source "$HOME/.cargo/env" to get cargo on PATH.
- LLVM paths: /usr/lib/llvm-18/{include/mlir-c, lib/libMLIRCAPI*.a, lib/libMLIR.so, lib/cmake/mlir/MLIRConfig.cmake, lib/cmake/llvm/LLVMConfig.cmake}. Extra: libpolly-18-dev now installed for linking.
- Build for --features mlir:
  ```
  source "$HOME/.cargo/env"
  export PATH="/usr/lib/llvm-17/bin:$PATH"
  export TABLEGEN_17_0_PREFIX=/usr/lib/llvm-17
  export MLIR_DIR=/usr/lib/llvm-18/lib/cmake/mlir
  export LLVM_DIR=/usr/lib/llvm-18/lib/cmake/llvm
  cargo build -p axiom-compiler --features mlir
  ```
- Test counts: 46 local (33 default + 8 ax_parser + 5 emit_mlir). Works with `cargo test -p axiom-compiler --features mlir`.
- melior 0.14: tblgen 0.3.0 (LLVM 17) + mlir-sys 0.2.2 (LLVM 18) + melior 0.14. Build needs PATH with llvm-17/bin first + env vars.
- Issue status: CLOSED #3,#4,#5,#7,#8,#9,#12,#13,#14,#15,#16,#17,#18,#19,#20,#21,#22,#23 (18). OPEN #1,#2,#6,#10,#11 (5 open).
- Nova reference clone: /tmp/opencode/nova-src.

## Relevant Files
- /tmp/opencode/axiom-lang/compiler/Cargo.toml — melior dep wired under `mlir` feature; [[bin]] target for `axiom` binary.
- /tmp/opencode/axiom-lang/compiler/src/lib.rs — modules: purity, ownership, std_own, parallel, region, epoch, axiom_ir, ax_parser (always); emit_mlir (cfg mlir); parallelize (cfg parallel).
- /tmp/opencode/axiom-lang/compiler/src/main.rs — CLI binary: `axiom compile --backend mlir`, `axiom emit --example`.
- /tmp/opencode/axiom-lang/compiler/src/ax_parser.rs — `.ax` text file parser → AxiomIR (8 tests).
- /tmp/opencode/axiom-lang/compiler/src/axiom_ir.rs — Phase 1 IR types: ValueRef, Operation, Block, FunctionDef, AxiomModule. 4 tests.
- /tmp/opencode/axiom-lang/compiler/src/emit_mlir.rs — Phase 1 melior emitter walks AxiomIR, emits func/arith/scf. 5 tests.
- /tmp/opencode/axiom-lang/compiler/src/purity.rs, ownership.rs, std_own.rs, parallel.rs, region.rs, epoch.rs — implemented analyses.
- /tmp/opencode/axiom-lang/examples/add.ax, seven.ax, max.ax, fib.ax — example programs for end-to-end testing.
- /tmp/opencode/axiom-lang/axiom-trace/src/lib.rs — Ledger.
- /tmp/opencode/axiom-lang/axiom-trace/src/mcp.rs — rmcp server (--features mcp).
- /tmp/opencode/axiom-lang/docs/EffectSurface.md — #22.
- /tmp/opencode/axiom-lang/scripts/env-mlir.sh — reusable env setup for MLIR builds.
