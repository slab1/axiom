# Axiom Effect Surfaces — Capability Enforcement for Supply-Chain Review

This document explains how Axiom renders an **effect surface** for any module, and
why that surface is a credible supply-chain artifact (you can audit what a
dependency *may* do without reading its source). It is the basis for issue #22
("`axiom doc` renders effect surfaces for supply-chain review").

> Source of truth: the enforcement logic is inherited from Nova's
> `compiler-codegen/src/effect_surface.rs`, which Axiom forks in Phase 0
> (issue #1). The description below reflects what that module actually does.

---

## 1. The model: effects live in the signature

In Axiom (as in Nova) every function carries an **effect row** — the set of
effects it may perform. Effects are first-class and declared at the type level:

```axiom
fn fetch(url: String, !Net) -> Bytes      // may touch the network
fn log(msg: String, !Audit)              // may write an audit record
fn pure_add(a: i64, b: i64) -> i64      // empty row => pure
```

Two defaults make the surface *small by construction*:

- **Capability by default.** A function has *no* effects unless it asks for
  them. You opt **into** capability, never out of it.
- **`forbid` is a compile-time capability check.** A module can declare
  `forbid Net` (or `forbid Audit, Fs`). Any transitively-called function that
  performs a forbidden effect is a **hard compile error**, not a runtime
  surprise.

This is the opposite of most languages, where "can this dependency phone
home?" is unknowable without auditing the source. In Axiom the answer is in the
signature.

---

## 2. How enforcement actually works (no hand-waving)

The capability check is a **graph reachability pass**, not a string grep:

1. Each function node carries its effect row (computed by
   `types::infer_effects`).
2. A module's `forbid` set is the *forbidden* capability set.
3. The pass walks the call graph. For every *reachable* function, if its
   effect row intersects the forbidden set, compilation fails with a precise
   diagnostic: *which* function, *which* effect, and *which* call path
   introduced it.

Key correctness properties (verified by Nova's `EXPECT`-marker test suite,
which Axiom adopts in Phase 0, issue #2):

- **Transitive.** A pure function that *calls* a `Net` function inherits
  `Net`. A `forbid Net` module cannot silently depend on a networking helper
  two layers down.
- **No silent downgrade.** You cannot "cast away" an effect. The row only
  grows along call edges; `forbid` is checked *after* inference, so the
  surface you see is the real one.
- **Deterministic.** Given the same sources, the surface is identical — so it
  can be diffed between versions (the seed for the MCP `diff_states` tool,
  issue #19).

This is exactly the property a supply-chain reviewer wants: *"does this
update add a network or filesystem capability it didn't have before?"* becomes a
`diff` of two surfaces.

---

## 3. What `axiom doc --surface <module>` renders

The rendered surface is a compact, machine- and human-readable table:

```
module: checkout@1
forbids: [ ]
declares capabilities: [ Net, Audit, Fs ]

  fn fetch_and_record   -> { Net, Audit }      (inherited from callees)
  fn pure_total         -> { }                  PURE  ✓ auto-parallelizable
  fn write_receipt      -> { Fs }              (forbidden by dependents below)

dependents that forbid capabilities:
  cli@1  forbids [ Fs ]   => ERROR if links write_receipt
```

For each exported function the table shows:
- the **effective** effect row (after transitive inference),
- a **PURE** marker when the row is empty (this is also the input to the
  automatic-parallelism pass, `purity.rs` / issue #9),
- any **forbid** violations contributed by dependents.

---

## 4. Why this is a supply-chain artifact

| Property | Traditional dep | Axiom surface |
|----------|-----------------|--------------|
| "Can it access the network?" | read source / hope | in the signature |
| "Did this update add a capability?" | unknown | `diff` two surfaces |
| "Is `forbid` enforced?" | n/a | compile-time graph pass |
| Auditable without trust | no | yes — surface is content-addressed |

A reviewer (human or an LLM agent via the MCP server, issue #17) can answer
*"why did `total` become 1422?"* and *"what new capabilities did v2 add?"*
from the surface alone.

---

## 5. Status & next steps (tracking)

- [x] Model documented (this file) — issue #22.
- [ ] `axiom doc --surface` renderer emits the table above — issue #22 follow-up.
- [ ] Surface is content-addressed and stored per release (feeds `diff_states`,
      issue #19, and the epoch model, issue #21).
- [ ] Editor / PR bot surfaces `forbid` violations inline (tree-sitter grammar,
      cross-cutting).

See `TRACKING.md` for the full phase plan and `compiler/src/purity.rs` for
the always-compiled effect-row analysis that feeds this renderer.
