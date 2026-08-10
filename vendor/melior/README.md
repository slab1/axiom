# melior 0.14.0 (vendored, patched)

This is a vendored copy of `melior 0.14.0` from crates.io with minimal patches
so it compiles against `melior-macro 0.8.1` and the MLIR 18 C API. The root
workspace wires it in via `[patch.crates-io]` in `Cargo.toml`.

**Why vendored:** melior 0.14.0 as published on crates.io cannot compile:

1. `src/pass.rs` — declares `pub mod r#async; pub mod gpu; pub mod linalg;
   pub mod sparse_tensor; pub mod transform;` but those modules call
   `melior_macro::async_passes!` / `gpu_passes!` / `linalg_passes!` /
   `sparse_tensor_passes!` / `transform_passes!` — macros that **do not exist**
   in melior-macro 0.8.1. `pub mod conversion` also fails because mlir-sys
   bindings against MLIR 18 lack `mlirCreateConversionConvertLinalgToLLVMPass`.
2. `src/string_ref.rs` — MLIR 18 changed `MlirStringRef.data` from `*const i8`
   to `c_char` (which is `u8` on aarch64/Termux but `i8` on x86_64 Ubuntu).
   Patch casts to `c_char` so it compiles on both targets.
3. `src/dialect/llvm/type.rs` — MLIR 18 changed `mlirLLVMPointerTypeGet` to
   take a `MlirContext` (not `MlirType`); patched to the 18 signature.

## Patch manifest

| File | Change |
|------|--------|
| `src/pass.rs` | Comment out `async`, `conversion`, `gpu`, `linalg`, `sparse_tensor`, `transform` modules |
| `src/string_ref.rs` | `MlirStringRef.data` cast uses `c_char` (u8 on aarch64, i8 on x86_64) |
| `src/dialect/llvm/type.rs` | `pointer()` takes `&Context` + address_space; calls `mlirLLVMPointerTypeGet(context, ..)` |

## How to re-vendor (if upgrading)

```sh
# Compare against pristine:
curl -L -o /tmp/melior.crate https://crates.io/api/v1/crates/melior/0.14.0/download
tar xzf /tmp/melior.crate -C /tmp && diff -rq /tmp/melior-0.14.0 vendor/melior

# Apply the three patches above, then re-test:
cargo test --features mlir -p axiom-compiler
```

## Upstream

- https://github.com/raviqqe/melior
- Version vendored: 0.14.0 (checksum `d579dda794588eae5b4e470efb3acaa330c1b9af6148d40abbf4d683bf01922b`)
