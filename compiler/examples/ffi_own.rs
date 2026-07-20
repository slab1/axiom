//! Phase 3 — issue #15: FFI + embedded examples use explicit ownership.
//!
//! Demonstrates moving an `Own<T>` buffer **across a C ABI boundary**
//! without cloning: the value is moved out and passed by value, so the C
//! side owns it with no aliasing hazard. Run with
//! `cargo run -p axiom-compiler --example ffi_own`.

use axiom_compiler::ownership::Own;

// Mock C callee (stands in for a real embedded / HAL function). Defined
// in-Rust so the example compiles and runs standalone. Edition 2024 requires
// `unsafe extern "C"` for the FFI block and `#[unsafe(no_mangle)]`
// (linkage attributes are now unsafe attributes).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn consume_i32(v: i32) {
    println!("[C] received {v}");
}

/// Hand an `Own<i32>` to C, transferring ownership by value.
fn hand_to_c(v: Own<i32>) {
    // move the inner value out and pass it by value across the ABI
    let v = v.into_inner();
    unsafe { consume_i32(v) };
}

fn main() {
    let owned = Own::new(42);
    hand_to_c(owned);
    println!("handed ownership to C without cloning");
}
