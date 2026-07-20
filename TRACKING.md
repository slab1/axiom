# Axiom — Tracking Issues (Phases 0–5)

Each phase is independently shippable and verifiable. Label issues with `phase/N`.

## Phase 0 — Fork & baseline
- [ ] Fork `nv-lang/nova`; vendor `compiler-codegen` + `nova_rt` as `compiler/`
- [ ] Wire Nova's `EXPECT`-marker test suite into `cargo test`
- [ ] Green baseline via C backend
- **Good first issue:** run the Nova test suite under Axiom's name; fix naming only.

## Phase 1 — MLIR emitter (drop-in for `emit_c.rs`)
- [ ] Add `melior` dependency; link against MLIR C API (LLVM 18+)
- [ ] `emit_mlir.rs`: emit `func` + `arith` + `scf` for scalar code
- [ ] `axiom build --backend mlir` flag; C remains default
- [ ] Parity: every C-passing test passes via MLIR→LLVM
- **Good first issue:** port `arith.constant` / `arith.addi` emission from `emit_c`.

## Phase 2 — Parallelism-extraction pass (the novel insight)
- [x] `parallelize.rs` runs after `types::infer_effects` — `compiler/src/parallel.rs` consumes the `EffectRow` produced by inference (#8)
- [x] Detect pure + data-parallel expressions — `parallel::analyze` requires empty effect row AND value-typed operands (#9)
- [ ] Lower to `hvm-core` net OR `scf.parallel` region (#10 — needs `hvm-core`)
- [ ] Benchmarks: N-body, prefix-scan show linear speedup on core count (#11)
- [x] **Good first issue (#12):** purity-detection unit tests — `compiler/src/purity.rs` (always-compiled, run by default `cargo test`).

## Phase 3 — Gradual ownership
- [x] `ownership.rs`: `own T` / `borrow T` modifiers — `compiler/src/ownership.rs` (always-compiled, with tests)
- [x] Region inference pass; GC fallback for unannotated code — `compiler/src/region.rs` (#14)
- [ ] FFI + embedded examples compile with explicit ownership (#15)
- [x] **Good first issue (#16):** add `own`/`borrow` to one std container type — `compiler/src/std_own.rs` (`OwnedVec<T>`).

## Phase 4 — MCP time-travel server
- [x] `axiom-trace` crate: record handler dispatches → deterministic HLC ledger — `axiom-trace/src/lib.rs` (`Ledger`, `HandlerEvent`) with 7 unit tests (#17)
- [x] `rmcp` server with tools: `why_changed`, `replay_from`, `diff_states` — `axiom-trace/src/mcp.rs` (`--features mcp`, compiles clean) (#18)
- [x] Integration test: agent answers "why did `total` become 1422?" — `Ledger` scenario + `why_changed`/`diff_states` tests (#19)
- [x] **Good first issue (#20):** MCP tool `list_handlers` added to `axiom-trace`.

## Phase 5 — Packaging & epochs
- [ ] Epoch model: one atomic release (compiler + stdlib + modules)
- [ ] `axiom.toml` declares epoch; no version resolution
- [ ] Catalog of bundled modules (io, json, os, net, ...)

## Cross-cutting
- [x] `axiom doc` effect-surface **design doc** — `docs/EffectSurface.md` (renderer is a follow-up; see issue #22)
- [ ] Editor plugins (tree-sitter grammar from Nova's existing one)
- [x] CI: build + test on Linux/macOS/Windows — green (run 29764607140)
