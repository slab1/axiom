//! Lower high-level AST to Axiom IR (SSA form).
//!
//! Walks the expression-based `ast::Module` and produces
//! `axiom_ir::AxiomModule` by:
//! - Creating SSA temporaries for each intermediate value
//! - Desugaring binary operations to individual ops
//! - Converting if/else expressions to `scf.if` blocks
//! - Handling let bindings as sequential operations

use crate::ast;
use crate::axiom_ir;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct LowerError {
    pub message: String,
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lower error: {}", self.message)
    }
}

impl std::error::Error for LowerError {}

type Result<T> = std::result::Result<T, LowerError>;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Lower a high-level AST module to AxiomIR.
pub fn lower_module(module: &ast::Module) -> Result<axiom_ir::AxiomModule> {
    let mut functions = Vec::new();
    for f in &module.functions {
        functions.push(lower_function(f)?);
    }
    let mut extern_functions = Vec::new();
    for ext in &module.extern_functions {
        extern_functions.push(lower_extern_function(ext));
    }
    let mut ir_mod = axiom_ir::AxiomModule::new(functions);
    ir_mod.extern_functions = extern_functions;
    Ok(ir_mod)
}

/// Analyze all functions in the lowered module for auto-parallelism (Phase 2 integration).
pub fn analyze_module_parallelism(module: &axiom_ir::AxiomModule) -> Vec<(String, bool)> {
    module.functions.iter().map(|f| {
        let is_pure = f.body.ops.iter().all(|op| match op {
            axiom_ir::Operation::Constant(_)
            | axiom_ir::Operation::Addi(_)
            | axiom_ir::Operation::Subi(_)
            | axiom_ir::Operation::Muli(_)
            | axiom_ir::Operation::Addf(_)
            | axiom_ir::Operation::Subf(_)
            | axiom_ir::Operation::Mulf(_)
            | axiom_ir::Operation::Cmpi(_)
            | axiom_ir::Operation::Return(_)
            | axiom_ir::Operation::ScfIf(_)
            | axiom_ir::Operation::ScfFor(_)
            | axiom_ir::Operation::ScfYield(_) => true,
            _ => false,
        });
        (f.name.clone(), is_pure)
    }).collect()
}

fn lower_extern_function(ext: &ast::ExternFunctionDef) -> axiom_ir::ExternFunctionDef {
    let params: Vec<axiom_ir::Param> = ext.params.iter()
        .map(|p| axiom_ir::Param {
            name: p.name.clone(),
            typ: ast::to_core_type(&p.typ),
        })
        .collect();

    let return_types = match &ext.return_type {
        Some(t) => vec![ast::to_core_type(t)],
        None => vec![],
    };

    axiom_ir::ExternFunctionDef {
        name: ext.name.clone(),
        params,
        return_types,
        abi: ext.abi.clone(),
    }
}

// ---------------------------------------------------------------------------
// Function lowering
// ---------------------------------------------------------------------------

fn lower_function(f: &ast::FunctionDef) -> Result<axiom_ir::FunctionDef> {
    let params: Vec<axiom_ir::Param> = f.params.iter()
        .map(|p| axiom_ir::Param {
            name: p.name.clone(),
            typ: ast::to_core_type(&p.typ),
        })
        .collect();

    let return_types = match &f.return_type {
        Some(t) => vec![ast::to_core_type(t)],
        None => vec![],
    };

    let mut ctx = LowerCtx::new(&params);
    let body_expr = lower_expr(&f.body, &mut ctx)?;

    // Emit return with the body's result
    let mut ops = ctx.ops;
    ops.push(axiom_ir::Operation::Return(axiom_ir::ReturnOp {
        operands: vec![body_expr],
    }));

    Ok(axiom_ir::FunctionDef {
        name: f.name.clone(),
        params,
        return_types,
        body: axiom_ir::Block::new(ops),
    })
}

// ---------------------------------------------------------------------------
// Lowering context
// ---------------------------------------------------------------------------

struct LowerCtx {
    /// The ops being built for the current block.
    ops: Vec<axiom_ir::Operation>,
    /// Variable name → ValueRef map.
    vars: HashMap<String, axiom_ir::ValueRef>,
    /// Next temporary index.
    temp_counter: usize,
    /// Next op index for this block.
    next_op_index: usize,
}

impl LowerCtx {
    fn new(params: &[axiom_ir::Param]) -> Self {
        let mut vars = HashMap::new();
        for (i, p) in params.iter().enumerate() {
            vars.insert(p.name.clone(), axiom_ir::ValueRef::block_arg(i, p.typ.clone()));
        }
        LowerCtx {
            ops: Vec::new(),
            vars,
            temp_counter: 0,
            next_op_index: 0,
        }
    }

    /// Generate a fresh temporary name.
    fn fresh_temp(&mut self) -> String {
        let n = self.temp_counter;
        self.temp_counter += 1;
        format!("%{n}")
    }

    /// Allocate the next op index without emitting anything.
    fn reserve_op(&mut self) -> usize {
        let idx = self.next_op_index;
        self.next_op_index += 1;
        idx
    }

    /// Emit an operation that produces a result, returning its ValueRef.
    fn emit_op(&mut self, op: axiom_ir::Operation, typ: axiom_ir::Type) -> axiom_ir::ValueRef {
        let idx = self.next_op_index;
        self.next_op_index += 1;
        self.ops.push(op);
        axiom_ir::ValueRef::op_result(idx, typ)
    }

    /// Emit an operation that produces no result (e.g. scf.yield).
    fn emit_void(&mut self, op: axiom_ir::Operation) {
        self.next_op_index += 1;
        self.ops.push(op);
    }
}

// ---------------------------------------------------------------------------
// Expression lowering
// ---------------------------------------------------------------------------

fn lower_expr(expr: &ast::Expr, ctx: &mut LowerCtx) -> Result<axiom_ir::ValueRef> {
    match expr {
        ast::Expr::Int(val, typ) => {
            let core_typ = ast::to_core_type(typ);
            Ok(ctx.emit_op(
                axiom_ir::Operation::Constant(axiom_ir::ConstantOp { value: *val, typ: core_typ.clone() }),
                core_typ,
            ))
        }

        ast::Expr::Float(val, typ) => {
            // Float constants as i64 bit patterns (simplified)
            let bits = if *typ == ast::Type::F64 {
                val.to_bits() as i64
            } else {
                (*val as f32).to_bits() as i64
            };
            let core_typ = ast::to_core_type(typ);
            Ok(ctx.emit_op(
                axiom_ir::Operation::Constant(axiom_ir::ConstantOp { value: bits, typ: core_typ.clone() }),
                core_typ,
            ))
        }

        ast::Expr::Bool(val) => {
            let core_typ = axiom_ir::Type::I1;
            Ok(ctx.emit_op(
                axiom_ir::Operation::Constant(axiom_ir::ConstantOp { value: if *val { 1 } else { 0 }, typ: core_typ.clone() }),
                core_typ,
            ))
        }

        ast::Expr::Var(name) => {
            ctx.vars.get(name).cloned()
                .ok_or_else(|| LowerError {
                    message: format!("undefined variable '{name}'"),
                })
        }

        ast::Expr::Block(stmts, tail) => {
            for stmt in stmts {
                lower_stmt(stmt, ctx)?;
            }
            lower_expr(tail, ctx)
        }

        ast::Expr::Binary(op, lhs, rhs) => {
            lower_binary(*op, lhs, rhs, ctx)
        }

        ast::Expr::Unary(op, expr) => {
            lower_unary(*op, expr, ctx)
        }

        ast::Expr::If(cond, then_expr, else_expr) => {
            lower_if(cond, then_expr, else_expr, ctx)
        }

        ast::Expr::Call(name, args) => {
            lower_call(name, args, ctx)
        }

        ast::Expr::Handle(_effect_name, _ops, body) => {
            lower_expr(body, ctx)
        }
    }
}

// ---------------------------------------------------------------------------
// Statement lowering
// ---------------------------------------------------------------------------

fn lower_stmt(stmt: &ast::Stmt, ctx: &mut LowerCtx) -> Result<()> {
    match stmt {
        ast::Stmt::Let(name, typ, value) => {
            let vr = lower_expr(value, ctx)?;
            let _core_typ = typ.as_ref().map(|t| ast::to_core_type(t))
                .unwrap_or_else(|| vr.typ().clone());
            ctx.vars.insert(name.clone(), vr);
            Ok(())
        }
        ast::Stmt::Expr(expr) => {
            lower_expr(expr, ctx)?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Binary operations
// ---------------------------------------------------------------------------

fn lower_binary(op: ast::BinOp, lhs: &ast::Expr, rhs: &ast::Expr, ctx: &mut LowerCtx) -> Result<axiom_ir::ValueRef> {
    let lhs_vr = lower_expr(lhs, ctx)?;
    let rhs_vr = lower_expr(rhs, ctx)?;
    let typ = lhs_vr.typ().clone();

    match op {
        ast::BinOp::Add => {
            if matches!(typ, axiom_ir::Type::F32 | axiom_ir::Type::F64) {
                Ok(ctx.emit_op(axiom_ir::Operation::Addf(axiom_ir::AddfOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            } else {
                Ok(ctx.emit_op(axiom_ir::Operation::Addi(axiom_ir::AddiOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            }
        }
        ast::BinOp::Sub => {
            if matches!(typ, axiom_ir::Type::F32 | axiom_ir::Type::F64) {
                Ok(ctx.emit_op(axiom_ir::Operation::Subf(axiom_ir::SubfOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            } else {
                Ok(ctx.emit_op(axiom_ir::Operation::Subi(axiom_ir::SubiOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            }
        }
        ast::BinOp::Mul => {
            if matches!(typ, axiom_ir::Type::F32 | axiom_ir::Type::F64) {
                Ok(ctx.emit_op(axiom_ir::Operation::Mulf(axiom_ir::MulfOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            } else {
                Ok(ctx.emit_op(axiom_ir::Operation::Muli(axiom_ir::MuliOp {
                    lhs: lhs_vr, rhs: rhs_vr, typ: typ.clone(),
                }), typ))
            }
        }
        ast::BinOp::Eq => {
            emit_cmpi(axiom_ir::CmpiPredicate::Eq, lhs_vr, rhs_vr, ctx)
        }
        ast::BinOp::Ne => {
            emit_cmpi(axiom_ir::CmpiPredicate::Ne, lhs_vr, rhs_vr, ctx)
        }
        ast::BinOp::Lt => {
            emit_cmpi(axiom_ir::CmpiPredicate::Slt, lhs_vr, rhs_vr, ctx)
        }
        ast::BinOp::Gt => {
            emit_cmpi(axiom_ir::CmpiPredicate::Sgt, lhs_vr, rhs_vr, ctx)
        }
        ast::BinOp::Le => {
            emit_cmpi(axiom_ir::CmpiPredicate::Sle, lhs_vr, rhs_vr, ctx)
        }
        ast::BinOp::Ge => {
            emit_cmpi(axiom_ir::CmpiPredicate::Sge, lhs_vr, rhs_vr, ctx)
        }
        _ => Err(LowerError {
            message: format!("binary op {:?} not yet implemented", op),
        })
    }
}

fn emit_cmpi(pred: axiom_ir::CmpiPredicate, lhs: axiom_ir::ValueRef, rhs: axiom_ir::ValueRef, ctx: &mut LowerCtx) -> Result<axiom_ir::ValueRef> {
    Ok(ctx.emit_op(
        axiom_ir::Operation::Cmpi(axiom_ir::CmpiOp { predicate: pred, lhs, rhs }),
        axiom_ir::Type::I1,
    ))
}

// ---------------------------------------------------------------------------
// Unary operations
// ---------------------------------------------------------------------------

fn lower_unary(op: ast::UnOp, expr: &ast::Expr, ctx: &mut LowerCtx) -> Result<axiom_ir::ValueRef> {
    let vr = lower_expr(expr, ctx)?;
    let typ = vr.typ().clone();

    match op {
        ast::UnOp::Neg => {
            // 0 - expr
            let zero = ctx.emit_op(
                axiom_ir::Operation::Constant(axiom_ir::ConstantOp { value: 0, typ: typ.clone() }),
                typ.clone(),
            );
            Ok(ctx.emit_op(
                axiom_ir::Operation::Subi(axiom_ir::SubiOp { lhs: zero, rhs: vr, typ: typ.clone() }),
                typ,
            ))
        }
        ast::UnOp::Not => {
            // xor with 1 (for bool/i1)
            let one = ctx.emit_op(
                axiom_ir::Operation::Constant(axiom_ir::ConstantOp { value: 1, typ: axiom_ir::Type::I1 }),
                axiom_ir::Type::I1,
            );
            Ok(ctx.emit_op(
                axiom_ir::Operation::Subi(axiom_ir::SubiOp {
                    lhs: one, rhs: vr, typ: axiom_ir::Type::I1,
                }),
                axiom_ir::Type::I1,
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// If/else lowering (scf.if)
// ---------------------------------------------------------------------------

fn lower_if(
    cond: &ast::Expr,
    then_expr: &ast::Expr,
    else_expr: &Option<Box<ast::Expr>>,
    ctx: &mut LowerCtx,
) -> Result<axiom_ir::ValueRef> {
    let cond_vr = lower_expr(cond, ctx)?;

    // Lower then and else branches (in their own contexts)
    let (then_block, result_type) = lower_branch(then_expr, ctx)?;
    let (else_block, else_type) = if let Some(ee) = else_expr {
        let (eb, et) = lower_branch(ee, ctx)?;
        (Some(eb), et)
    } else {
        (None, axiom_ir::Type::I1) // default unit type
    };

    let result_types = if result_type != axiom_ir::Type::I1 || else_type != axiom_ir::Type::I1 {
        // Decide which type — pick the then type
        vec![result_type.clone()]
    } else {
        vec![]
    };

    let op = axiom_ir::Operation::ScfIf(axiom_ir::ScfIfOp {
        condition: cond_vr,
        then_block,
        else_block,
        result_types: result_types.clone(),
    });

    let idx = ctx.next_op_index;
    ctx.next_op_index += 1;
    ctx.ops.push(op);

    if result_types.is_empty() {
        // No results — scf.if for side effects, return a dummy
        Ok(axiom_ir::ValueRef::op_result(idx, axiom_ir::Type::I1))
    } else {
        Ok(axiom_ir::ValueRef::op_result(idx, result_types[0].clone()))
    }
}

/// Lower a branch expression into a block with scf.yield.
fn lower_branch(
    expr: &ast::Expr,
    parent_ctx: &LowerCtx,
) -> Result<(axiom_ir::Block, axiom_ir::Type)> {
    let mut branch_ctx = LowerCtx {
        ops: Vec::new(),
        vars: parent_ctx.vars.clone(), // inherit parent scope
        temp_counter: 0,
        next_op_index: 0,
    };

    let result = lower_expr(expr, &mut branch_ctx)?;
    let result_typ = result.typ().clone();
    branch_ctx.emit_void(axiom_ir::Operation::ScfYield(axiom_ir::ScfYieldOp {
        operands: vec![result],
    }));

    Ok((axiom_ir::Block::new(branch_ctx.ops), result_typ))
}

// ---------------------------------------------------------------------------
// Function call lowering
// ---------------------------------------------------------------------------

fn lower_call(name: &str, args: &[ast::Expr], ctx: &mut LowerCtx) -> Result<axiom_ir::ValueRef> {
    let mut arg_refs = Vec::new();
    for arg in args {
        arg_refs.push(lower_expr(arg, ctx)?);
    }

    // Determine return type from context (default I64)
    // In a full implementation, we'd look up the function signature
    let typ = axiom_ir::Type::I64;

    Ok(ctx.emit_op(
        axiom_ir::Operation::Call(axiom_ir::CallOp {
            name: name.to_string(),
            args: arg_refs,
            typ: typ.clone(),
        }),
        typ,
    ))
}
