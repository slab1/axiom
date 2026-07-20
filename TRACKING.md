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
- [ ] `parallelize.rs` runs after `types::infer_effects`
- [ ] Detect pure + data-parallel expressions (empty effect row + value types)
- [ ] Lower to `hvm-core` net OR `scf.parallel` region
- [ ] Benchmarks: N-body, prefix-scan show linear speedup on core count
- [x] **Good first issue (#12):** purity-detection unit tests — `compiler/src/purity.rs` (always-compiled, run by default `cargo test`).

## Phase 3 — Gradual ownership
- [x] `ownership.rs`: `own T` / `borrow T` modifiers — `compiler/src/ownership.rs` (always-compiled, with tests)
- [ ] Region inference pass; GC fallback for unannotated code
- [ ] FFI + embedded examples compile with explicit ownership
- **Good first issue:** add `own`/`borrow` to one std container type.

## Phase 4 — MCP time-travel server
- [ ] `axiom-trace` crate: record handler dispatches → deterministic HLC ledger
- [ ] `rmcp` server with tools: `why_changed`, `replay_from`, `diff_states`
- [ ] Integration test: agent answers "why did `total` become 1422?"
- **Good first issue:** add one MCP tool (`list_handlers`) to `axiom-trace`.

## Phase 5 — Packaging & epochs
- [ ] Epoch model: one atomic release (compiler + stdlib + modules)
- [ ] `axiom.toml` declares epoch; no version resolution
- [ ] Catalog of bundled modules (io, json, os, net, ...)

## Cross-cutting
- [x] `axiom doc` effect-surface **design doc** — `docs/EffectSurface.md` (renderer is a follow-up; see issue #22)
- [ ] Editor plugins (tree-sitter grammar from Nova's existing one)
- [x] CI: build + test on Linux/macOS/Windows — green (run 29764607140)
