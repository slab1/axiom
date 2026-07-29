//! Phase 1 — issue #4: Axiom IR for MLIR emission.
//!
//! Defines the intermediate representation that the MLIR backend walks.
//! This is a minimal, flat IR that maps directly to `func`, `arith`, and `scf`
//! dialects in MLIR. It is always compiled (pure types, no deps) so it can be
//! unit-tested in the default `cargo test` run, but the emission pass itself
//! is feature-gated behind `#[cfg(feature = "mlir")]` in `emit_mlir.rs`.
//!
//! See `TRACKING.md` Phase 1 and issue #4.

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The types expressible in the func/arith/scf dialects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    I1,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    /// Function type: `(input_types) -> output_types`
    Function(Vec<Type>, Vec<Type>),
}

impl Type {
    /// Return the bit width for integer types.
    pub fn integer_width(&self) -> Option<u32> {
        match self {
            Type::I1 => Some(1),
            Type::I8 => Some(8),
            Type::I16 => Some(16),
            Type::I32 => Some(32),
            Type::I64 => Some(64),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// Identifies which SSA value to reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueRef {
    /// A block argument (function parameter) at `param_index`.
    BlockArg { param_index: usize, typ: Type },
    /// The result of an operation at `op_index` (within the Block's ops list).
    OpResult {
        op_index: usize,
        result_index: usize,
        typ: Type,
    },
}

impl ValueRef {
    pub fn block_arg(param_index: usize, typ: Type) -> Self {
        ValueRef::BlockArg { param_index, typ }
    }

    pub fn op_result(op_index: usize, typ: Type) -> Self {
        ValueRef::OpResult {
            op_index,
            result_index: 0,
            typ,
        }
    }

    /// The type of this value.
    pub fn typ(&self) -> &Type {
        match self {
            ValueRef::BlockArg { typ, .. } => typ,
            ValueRef::OpResult { typ, .. } => typ,
        }
    }
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// A constant integer value: `arith.constant` with `IntegerAttr`.
#[derive(Debug, Clone)]
pub struct ConstantOp {
    pub value: i64,
    pub typ: Type,
}

/// Integer addition: `arith.addi`.
#[derive(Debug, Clone)]
pub struct AddiOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Integer subtraction: `arith.subi`.
#[derive(Debug, Clone)]
pub struct SubiOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Integer multiplication: `arith.muli`.
#[derive(Debug, Clone)]
pub struct MuliOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Float addition: `arith.addf`.
#[derive(Debug, Clone)]
pub struct AddfOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Float subtraction: `arith.subf`.
#[derive(Debug, Clone)]
pub struct SubfOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Float multiplication: `arith.mulf`.
#[derive(Debug, Clone)]
pub struct MulfOp {
    pub lhs: ValueRef,
    pub rhs: ValueRef,
    pub typ: Type,
}

/// Integer comparison: `arith.cmpi`.
#[derive(Debug, Clone)]
pub struct CmpiOp {
    pub predicate: CmpiPredicate,
    pub lhs: ValueRef,
    pub rhs: ValueRef,
}

/// Available integer comparison predicates (maps to `arith::CmpiPredicate`).
#[derive(Debug, Clone)]
pub enum CmpiPredicate {
    Eq,
    Ne,
    Slt,
    Sgt,
    Sle,
    Sge,
}

/// `func.return` — return from a function.
#[derive(Debug, Clone)]
pub struct ReturnOp {
    pub operands: Vec<ValueRef>,
}

/// `scf.if` — conditional region.
#[derive(Debug, Clone)]
pub struct ScfIfOp {
    pub condition: ValueRef,
    pub then_block: Block,
    pub else_block: Option<Block>,
    /// Result types of the if op (empty for terminator-style if).
    pub result_types: Vec<Type>,
}

/// `scf.for` — structured for loop.
#[derive(Debug, Clone)]
pub struct ScfForOp {
    pub lower_bound: ValueRef,
    pub upper_bound: ValueRef,
    pub step: ValueRef,
    pub iter_args: Vec<ValueRef>,
    pub body_block: Block,
    pub result_types: Vec<Type>,
}

/// `scf.yield` — yield a value back to the enclosing SCF op.
#[derive(Debug, Clone)]
pub struct ScfYieldOp {
    pub operands: Vec<ValueRef>,
}

/// `func.call` — call another function.
#[derive(Debug, Clone)]
pub struct CallOp {
    pub name: String,
    pub args: Vec<ValueRef>,
    pub typ: Type,
}

/// A single operation in a block.
#[derive(Debug, Clone)]
pub enum Operation {
    Constant(ConstantOp),
    Addi(AddiOp),
    Subi(SubiOp),
    Muli(MuliOp),
    Addf(AddfOp),
    Subf(SubfOp),
    Mulf(MulfOp),
    Cmpi(CmpiOp),
    Return(ReturnOp),
    ScfIf(ScfIfOp),
    ScfFor(ScfForOp),
    ScfYield(ScfYieldOp),
    Call(CallOp),
}

// ---------------------------------------------------------------------------
// Blocks and Functions
// ---------------------------------------------------------------------------

/// A basic block: a linear sequence of operations.
/// The last operation should be a terminator (e.g. `Return`, `ScfYield`).
#[derive(Debug, Clone)]
pub struct Block {
    pub ops: Vec<Operation>,
}

impl Block {
    pub fn new(ops: Vec<Operation>) -> Self {
        Block { ops }
    }

    /// Count how many values an operation produces (for SSA indexing).
    pub fn op_result_count(op: &Operation) -> usize {
        match op {
            Operation::Constant(_)
            | Operation::Addi(_)
            | Operation::Subi(_)
            | Operation::Muli(_)
            | Operation::Addf(_)
            | Operation::Subf(_)
            | Operation::Mulf(_)
            | Operation::Cmpi(_)
            | Operation::Call(_) => 1, // all produce one value
            Operation::Return(_) => 0,
            Operation::ScfIf(if_op) => if_op.result_types.len(),
            Operation::ScfFor(for_op) => for_op.result_types.len(),
            Operation::ScfYield(_) => 0,
        }
    }

    /// Return the type of the value produced at index `op_index` (result 0).
    /// Panics if the op produces no results.
    pub fn result_type(&self, op_index: usize) -> &Type {
        let op = &self.ops[op_index];
        match op {
            Operation::Constant(c) => &c.typ,
            Operation::Addi(a) => &a.typ,
            Operation::Subi(s) => &s.typ,
            Operation::Muli(m) => &m.typ,
            Operation::Addf(a) => &a.typ,
            Operation::Subf(s) => &s.typ,
            Operation::Mulf(m) => &m.typ,
            Operation::Cmpi(_) => &Type::I1,
            Operation::Call(c) => &c.typ,
            Operation::Return(_) => panic!("return has no result"),
            Operation::ScfIf(if_op) => {
                if if_op.result_types.is_empty() {
                    panic!("scf.if has no results");
                }
                &if_op.result_types[0]
            }
            Operation::ScfFor(for_op) => {
                if for_op.result_types.is_empty() {
                    panic!("scf.for has no results");
                }
                &for_op.result_types[0]
            }
            Operation::ScfYield(_) => panic!("scf.yield has no result"),
        }
    }
}

/// A function definition — the top-level unit in the module.
#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_types: Vec<Type>,
    pub body: Block,
}

/// A function parameter.
#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub typ: Type,
}

// ---------------------------------------------------------------------------
// Top-level Module
// ---------------------------------------------------------------------------

/// The top-level Axiom IR unit, ready for MLIR emission.
#[derive(Debug, Clone)]
pub struct AxiomModule {
    pub functions: Vec<FunctionDef>,
}

impl AxiomModule {
    pub fn new(functions: Vec<FunctionDef>) -> Self {
        AxiomModule { functions }
    }
}

// ---------------------------------------------------------------------------
// IR helper: build examples programmatically
// ---------------------------------------------------------------------------

/// Build a simple "add two numbers" function:
///   func.func @add(%a: i64, %b: i64) -> i64 {
///     %0 = arith.addi %a, %b : i64
///     func.return %0 : i64
///   }
pub fn build_add_example() -> AxiomModule {
    let a = ValueRef::block_arg(0, Type::I64);
    let b = ValueRef::block_arg(1, Type::I64);

    // add %a, %b → value at operation 0
    let add = Operation::Addi(AddiOp {
        lhs: a,
        rhs: b,
        typ: Type::I64,
    });

    // return %add → value at operation 0
    let ret = Operation::Return(ReturnOp {
        operands: vec![ValueRef::op_result(0, Type::I64)],
    });

    let body = Block::new(vec![add, ret]);

    let func = FunctionDef {
        name: "add".to_string(),
        params: vec![
            Param {
                name: "a".to_string(),
                typ: Type::I64,
            },
            Param {
                name: "b".to_string(),
                typ: Type::I64,
            },
        ],
        return_types: vec![Type::I64],
        body,
    };

    AxiomModule::new(vec![func])
}

/// Build a "constant seven" function:
///   func.func @seven() -> i64 {
///     %0 = arith.constant 7 : i64
///     func.return %0 : i64
///   }
pub fn build_constant_seven() -> AxiomModule {
    let body = Block::new(vec![
        Operation::Constant(ConstantOp {
            value: 7,
            typ: Type::I64,
        }),
        Operation::Return(ReturnOp {
            operands: vec![ValueRef::op_result(0, Type::I64)],
        }),
    ]);

    let func = FunctionDef {
        name: "seven".to_string(),
        params: vec![],
        return_types: vec![Type::I64],
        body,
    };

    AxiomModule::new(vec![func])
}

/// Build a conditional max function:
///   func.func @max(%a: i64, %b: i64) -> i64 {
///     %0 = arith.cmpi "sgt", %a, %b : i64
///     %1 = scf.if %0 -> i64 {
///       scf.yield %a : i64
///     } else {
///       scf.yield %b : i64
///     }
///     func.return %1 : i64
///   }
pub fn build_max_example() -> AxiomModule {
    let a = ValueRef::block_arg(0, Type::I64);
    let b = ValueRef::block_arg(1, Type::I64);

    // %0 = arith.cmpi "sgt", %a, %b : i64
    let cmp = Operation::Cmpi(CmpiOp {
        predicate: CmpiPredicate::Sgt,
        lhs: a.clone(),
        rhs: b.clone(),
    });

    // then block: yield %a
    let then_block = Block::new(vec![Operation::ScfYield(ScfYieldOp {
        operands: vec![a],
    })]);

    // else block: yield %b
    let else_block = Block::new(vec![Operation::ScfYield(ScfYieldOp {
        operands: vec![b],
    })]);

    // %1 = scf.if %0 -> i64 { ... } else { ... }
    let if_op = Operation::ScfIf(ScfIfOp {
        condition: ValueRef::op_result(0, Type::I1), // cmp result at op_index 0
        then_block,
        else_block: Some(else_block),
        result_types: vec![Type::I64],
    });

    let body = Block::new(vec![cmp, if_op, Operation::Return(ReturnOp {
        operands: vec![ValueRef::op_result(1, Type::I64)], // scf.if result at op_index 1
    })]);

    let func = FunctionDef {
        name: "max".to_string(),
        params: vec![
            Param { name: "a".to_string(), typ: Type::I64 },
            Param { name: "b".to_string(), typ: Type::I64 },
        ],
        return_types: vec![Type::I64],
        body,
    };

    AxiomModule::new(vec![func])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_seven_ir_structure() {
        let module = build_constant_seven();
        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[0];
        assert_eq!(func.name, "seven");
        assert!(func.params.is_empty());
        assert_eq!(func.return_types, vec![Type::I64]);
        assert_eq!(func.body.ops.len(), 2);
    }

    #[test]
    fn add_example_ir_structure() {
        let module = build_add_example();
        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[0];
        assert_eq!(func.name, "add");
        assert_eq!(func.params.len(), 2);
        assert_eq!(func.params[0].name, "a");
        assert_eq!(func.params[0].typ, Type::I64);
        assert_eq!(func.return_types, vec![Type::I64]);
    }

    #[test]
    fn max_example_ir_structure() {
        let module = build_max_example();
        assert_eq!(module.functions.len(), 1);
        let func = &module.functions[0];
        assert_eq!(func.name, "max");
        // ops: cmpi, scf.if, return
        assert_eq!(func.body.ops.len(), 3);
    }

    #[test]
    fn integer_width_query() {
        assert_eq!(Type::I64.integer_width(), Some(64));
        assert_eq!(Type::I1.integer_width(), Some(1));
        assert_eq!(Type::F64.integer_width(), None);
        assert_eq!(Type::Function(vec![], vec![]).integer_width(), None);
    }
}
