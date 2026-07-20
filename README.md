# Axiom — A Programming Language That Surpasses the Field

> **Thesis:** The "revolutionary" language doesn't need new syntax. Nova already has
> the right syntax and a *working* algebraic-effect + capability system. The
> surpassing move is to (1) port its backend to **MLIR** for speed + GPU, (2) exploit
> its *already-computed* purity info for **automatic parallelism** (Bend/HVM's trick),
> (3) add **opt-in ownership** (Rust-lite), and (4) expose its replayable effects as
> an **MCP time-travel debugger** for AI agents. That combination does not exist in any
> shipping language today.

---

## 1. Why this beats every incumbent at its own game

| vs | What they win at | What Axiom does better |
|----|------------------|------------------------|
| **Rust** | memory safety | safety *without* the borrow-checker fight (gradual/inferred ownership, GC fallback) |
| **Mojo** | Python + MLIR + GPU | not Python-coupled; adds automatic parallelism beyond GPU kernels |
| **Bend/HVM** | automatic parallelism | multi-vendor GPU (not just NVIDIA) + a production memory model |
| **Nova** | effects + capabilities | adds ownership, verification, and auto-parallelism |
| **Go** | productivity | memory safety + capability security + determinism |
| **AI-built langs (Promise)** | AI-friendly surface | adds parallelism + effects + time-travel that make generated code *verifiable* |

---

## 2. The proven foundation (do not reinvent)

Axiom reuses **Nova's compiler front-end** (parser, type-checker, effect inference,
capability enforcement) almost verbatim. We verified this is real, not theoretical,
by reading Nova's source (`compiler-codegen/src/effect_surface.rs`):

```rust
// Nova computes a package's "effect surface" = union of all PUBLIC
// function effect-rows. Then at dependency resolution:
fn check_forbidden(entry_pkg_dir) {
    for dep in dependencies {
        if dep.forbid.is_empty() { continue; }
        let surface = surface_of_package(&dep_dir);
        for forbidden in &dep.forbid {
            // exact match OR parameterized: forbid=["Fail"] catches Fail[IoError]
            let hit = surface.effects.iter().filter(|s| violates(s, forbidden));
            if !hit.is_empty() { bail!("dependency violates capability"); }
        }
    }
}
```

This means in Nova you write `netlib = { path = "../netlib", forbid = ["Net"] }` and
the **compiler rejects the dependency** if `netlib`'s public API touches the network.
The capability boundary is in the type system, not runtime. **This is the single most
valuable idea in the modern language landscape** — and almost no mainstream language has it.

**Decision:** Axiom's effect + capability layer = Nova's design, ported to a faster backend.

---

## 3. The three gaps stopping Nova from "surpassing" everyone

We read Nova's runtime (`effects.c`, `fibers.c`) and compiler pipeline. Concrete weaknesses:

| Gap | Evidence in Nova | Why it matters |
|-----|-----------------|---------------|
| **C-backend ceiling** | `emit_c.rs` (~20k LOC) generates C, then GCC/Clang. No LLVM, no MLIR, no GPU. | Can't compete with Mojo's MLIR SIMD/GPU or Bend's auto-parallelism. |
| **No automatic parallelism** | Concurrency is explicit: `spawn`, `supervised`, `parallel for`. M:N fibers via minicoro. | Bend proves you can get GPU/CPU parallelism with *zero* annotations. Nova makes you do it by hand. |
| **Boehm GC by default** | `alloc_boehm.c` — conservative GC. | Rust/Zig win on predictability; no ownership story at all. |

---

## 4. The Axiom synthesis

Take Nova's effect+capability core and add four layers it lacks.

### Layer A — Backend: MLIR instead of C  ✅ *proven viable*
- **Mojo** (v1.0 beta, May 2026) proves an MLIR backend gives Python-like syntax +
  native speed + GPU/AI accelerator support in one shot.
- **`melior`** (Rust MLIR C-API bindings) lets us emit MLIR dialects (`arith`, `scf`,
  `func`, `llvm`) directly from Rust — no C++ toolchain required for the frontend.
- **Axiom plan:** Keep Nova's parser/type-checker/effect-inference almost verbatim.
  Replace `emit_c.rs` with an MLIR emitter (`emit_mlir.rs`). Reuse `types::infer_effects`
  and `effect_surface` unchanged — they are backend-agnostic.
- Benefit: SIMD auto-vectorization, GPU targets, path to HVM-style parallel lowering.

### Layer B — Automatic data-parallelism (Bend/HVM model)  ✅ *proven viable*
- **`hvm-core`** crate: a massively parallel Interaction-Combinator evaluator in Rust
  (up to 10B rewrites/sec on Apple M3 Max; 65k threads on RTX 4090). It is a *compile
  target* — you feed it nets and it parallelizes greedily.
- **Axiom plan:** Add a *parallelism-extraction pass* after type-checking. When an
  expression is pure and data-parallel (provable from the effect-row being empty +
  value-type analysis), lower it to HVM interaction nets or MLIR `scf.parallel`.
- **The killer insight:** Nova's effect system *already tells you which code is pure*
  (empty effect row = safe to parallelize). Nova computes this and then *ignores* it for
  parallelism. Axiom uses it. This is the novel ~50-line pass.

### Layer C — Gradual ownership (Rust-lite)
- Nova has no ownership; Rust's borrow checker is the #1 complaint.
- **Axiom plan:** Default to GC (like Nova) for 90% of code. Add `own T` / `borrow T`
  annotations *only where needed* (hot loops, FFI, embedded). The type-checker infers
  ownership regions where possible, falling back to GC.
- Result: Rust's safety without Rust's fight.

### Layer D — The moat: MCP time-travel debugging  ✅ *proven viable*
- This is what makes Axiom *the* language for AI agents (the actual 2026 differentiator).
- **`rmcp`** (Rust MCP SDK) is mature; working MCP time-travel servers already exist
  (korg, mcp-recorder, bevy_debugger_mcp) — proving the pattern.
- Nova's `with`-handler model already makes **every side effect replayable** — handlers
  are recorded dispatches.
- **Axiom plan:** Persist handler-call traces to a deterministic recording. Ship an **MCP
  server** that exposes "why did variable X change?" / "what caused this null?" queries to
  any coding agent (Claude Code, Cursor, etc.). Undo.io charges enterprise prices for this
  on C++. Axiom gets it *for free* from the effect architecture.

---

## 5. The build plan (phased, each independently shippable + verifiable)

### Phase 0 — Fork & baseline
- Fork `nv-lang/nova`. Keep `compiler-codegen` (parser, type-checker, `effect_surface`,
  `emit_c`) and `nova_rt` as the starting point.
- Wire Nova's existing `EXPECT`-marker test suite so we have a green baseline.

### Phase 1 — MLIR emitter (drop-in for `emit_c.rs`)
- Add `emit_mlir.rs` using `melior`. Start with scalar code (functions, int ops, control
  flow via `scf`). Keep `emit_c` as fallback.
- Verify: every `.nv` test that passes via C also passes via MLIR→LLVM.
- **Success metric:** `axiom build hello.ax` produces a native binary via MLIR.

### Phase 2 — Parallelism-extraction pass (the novel insight)
- New module `parallelize.rs`, runs after `types::infer_effects`.
- For each expression with an empty effect row + value-type operands, emit an HVM net
  (via `hvm-core`) or `scf.parallel` region.
- Verify: a pure `map`/`reduce` over an array runs multi-threaded with no annotations.
- **Success metric:** N-body / prefix-scan benchmarks show linear speedup on core count.

### Phase 3 — Gradual ownership
- Add `own`/`borrow` type modifiers + a region inference pass.
- Default GC; opt-in ownership only where annotated.
- **Success metric:** FFI and embedded examples compile with explicit ownership, rest stays GC.

### Phase 4 — MCP time-travel server
- New crate `axiom-trace`: records handler dispatches to a deterministic JSONL/HLC ledger.
- Expose via `rmcp` an MCP server with tools: `why_changed`, `replay_from`, `diff_states`.
- **Success metric:** Claude Code can answer "why did `total` become 1422?" by querying
  the trace.

### Phase 5 — Packaging & epochs
- Adopt Promise's "epoch" model: one atomic release of compiler + stdlib + modules. No
  version resolution, no lockfile hell. AI only needs the epoch number.

---

## 6. Concrete next step (first PR)

**Title:** *Add `emit_mlir.rs`: an MLIR backend alongside `emit_c.rs`*

Smallest high-value change that proves the architecture:
1. Add `melior` dependency.
2. Mirror `emit_c.rs`'s scalar emission into `emit_mlir.rs` (functions, arith, scf).
3. Add `axiom build --backend mlir`.
4. Gate behind `--backend mlir`; C remains default until parity is reached.

This is independently testable against Nova's test suite and unblocks every later phase.

---

## 7. How to contribute

See [CONTRIBUTING.md](CONTRIBUTING.md). The repo is structured so each Phase maps to a
crate and a tracking issue. **23 tracking issues** are open, labeled by phase
(`phase/0`…`phase/5`) with `good first issue` entry points — start at
<https://github.com/axiom-lang/axiom/issues>.

Good first issues:
- **#7** Port `arith.constant` / `arith.addi` emission into `emit_mlir.rs`.
- **#12** Write purity-detection unit tests for `parallelize.rs`.
- **#20** Add the `list_handlers` tool to `axiom-trace`.

### Repo layout (contribution-ready)
```
axiom/
├── README.md            design + build plan
├── CONTRIBUTING.md      setup, workflow, design principles
├── TRACKING.md          Phases 0–5 issue checklist
├── .github/workflows/ci.yml   build+test matrix (Linux/macOS/Windows)
├── compiler/            forked Nova front-end + emit_mlir.rs / parallelize.rs (feature-gated)
├── axiom-trace/         MCP time-travel server (feature-gated)
└── examples/checkout.ax demo: effects, capability, auto-parallel, handler tests
```
The workspace is **virtual** — `cargo check --workspace` builds without LLVM/MLIR.
Enable backends per-phase: `--features mlir`, `--features parallel`, `--features mcp`.

---

## 8. Prior art (what we learned from)

| Project | What we took |
|---------|--------------|
| **Nova** (nv-lang) | effect + capability core, handler test seam, `with`-replay |
| **Mojo** (Modular) | MLIR backend, ownership-lite, GPU targets |
| **Bend / HVM2 / hvm-core** | automatic parallelism via interaction nets |
| **Rust** | ownership/borrowing semantics (as opt-in) |
| **Promise** | epoch/mono-versioned deps, AI-first surface |
| **rmcp + korg/mcp-recorder** | MCP time-travel debugging pattern |
| **melior** | Rust MLIR emission without a C++ frontend |

---

*Status: design + plan complete. Implementation starts at Phase 1 (MLIR emitter).*
