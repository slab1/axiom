#!/usr/bin/env bash
# Source this script before building with --features mlir
# Melior/melior-macro need LLVM 17 for tblgen, mlir-sys needs LLVM 18 for the MLIR C API
export PATH="/usr/lib/llvm-17/bin:$PATH"
export TABLEGEN_17_0_PREFIX=/usr/lib/llvm-17
export MLIR_DIR=/usr/lib/llvm-18/lib/cmake/mlir
export LLVM_DIR=/usr/lib/llvm-18/lib/cmake/llvm
echo "LLVM 17: $(llvm-config --version)"
echo "LLVM 18: $(llvm-config-18 --version)"
