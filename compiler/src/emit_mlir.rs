//! Phase 1 — MLIR emission via melior.
//!
//! Walks the Axiom IR (see `axiom_ir`) and emits MLIR operations in the
//! `func`, `arith`, and `scf` dialects using melior 0.14 bindings.
//! Feature-gated (`#[cfg(feature = "mlir")]`) — requires LLVM/MLIR 18+.
//!
//! See `TRACKING.md` Phase 1 and issues #3/#4/#7.

use melior::{
    Context,
    dialect::{arith, func, scf, DialectRegistry},
    ir::{
        attribute::{IntegerAttribute, StringAttribute, TypeAttribute},
        r#type::{FunctionType, IntegerType},
        Attribute, Block, Identifier, Location, Module, Region, Type, Value,
    },
    utility::register_all_dialects,
};


use crate::axiom_ir;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Emit a complete Axiom module to MLIR.
///
/// Returns a `melior::ir::Module` that can be dumped, verified, or lowered
/// further.
pub fn emit_module<'c>(context: &'c Context, module: &axiom_ir::AxiomModule, emit_c_interface: bool) -> Module<'c> {
    let registry = DialectRegistry::new();
    register_all_dialects(&registry);
    context.append_dialect_registry(&registry);
    context.load_all_available_dialects();

    let location = Location::unknown(context);
    let output = Module::new(location);

    for ext in &module.extern_functions {
        let operation = emit_extern_function(context, ext, location);
        output.body().append_operation(operation);
    }

    for func_def in &module.functions {
        let operation = emit_function(context, func_def, emit_c_interface, location);
        output.body().append_operation(operation);
    }

    output
}

fn emit_extern_function<'c>(
    context: &'c Context,
    ext: &axiom_ir::ExternFunctionDef,
    location: Location<'c>,
) -> melior::ir::Operation<'c> {
    let input_types: Vec<Type<'c>> = ext
        .params
        .iter()
        .map(|p| convert_type(context, &p.typ))
        .collect();
    let output_types: Vec<Type<'c>> = ext
        .return_types
        .iter()
        .map(|t| convert_type(context, t))
        .collect();

    let function_type = FunctionType::new(context, &input_types, &output_types);
    let region = Region::new(); // empty region = external function declaration

    let attrs = vec![];

    func::func(
        context,
        StringAttribute::new(context, &ext.name),
        TypeAttribute::new(function_type.into()),
        region,
        &attrs,
        location,
    )
}

// ---------------------------------------------------------------------------
// Type conversion
// ---------------------------------------------------------------------------

/// Convert an Axiom IR type to a melior `Type`.
fn convert_type<'c>(context: &'c Context, typ: &axiom_ir::Type) -> Type<'c> {
    match typ {
        axiom_ir::Type::I1 => IntegerType::new(context, 1).into(),
        axiom_ir::Type::I8 => IntegerType::new(context, 8).into(),
        axiom_ir::Type::I16 => IntegerType::new(context, 16).into(),
        axiom_ir::Type::I32 => IntegerType::new(context, 32).into(),
        axiom_ir::Type::I64 => IntegerType::new(context, 64).into(),
        axiom_ir::Type::F32 => Type::float32(context),
        axiom_ir::Type::F64 => Type::float64(context),
        axiom_ir::Type::Function(inputs, outputs) => {
            let ins: Vec<Type<'c>> = inputs.iter().map(|t| convert_type(context, t)).collect();
            let outs: Vec<Type<'c>> = outputs.iter().map(|t| convert_type(context, t)).collect();
            FunctionType::new(context, &ins, &outs).into()
        }
    }
}

// ---------------------------------------------------------------------------
// Value store
// ---------------------------------------------------------------------------

/// A value store that maps Axiom IR `ValueRef` entries to melior `Value`.
///
/// We erase the second lifetime with `'static` because all values live for
/// the duration of the MLIR Context (`'c`), which outlives all blocks.
struct ValueStore<'c> {
    /// Store for block arguments, indexed by param_index.
    block_args: Vec<Option<Value<'c, 'static>>>,
    /// Store for operation results, indexed by
    /// `op_index * MAX_RESULTS + result_index`.
    op_results: Vec<Option<Value<'c, 'static>>>,
}

const MAX_RESULTS: usize = 4;

impl<'c> ValueStore<'c> {
    fn new(num_params: usize, num_ops: usize) -> Self {
        ValueStore {
            block_args: vec![None; num_params],
            op_results: vec![None; num_ops * MAX_RESULTS],
        }
    }

    fn store_block_arg(&mut self, param_index: usize, value: &Value<'c, '_>) {
        let erased: Value<'c, 'static> = unsafe { std::mem::transmute_copy(value) };
        self.block_args[param_index] = Some(erased);
    }

    fn store_op_result(&mut self, op_index: usize, result_index: usize, value: &Value<'c, '_>) {
        let erased: Value<'c, 'static> = unsafe { std::mem::transmute_copy(value) };
        let s = op_index * MAX_RESULTS + result_index;
        self.op_results[s] = Some(erased);
    }

    fn try_resolve(&self, v: &axiom_ir::ValueRef) -> Option<Value<'c, 'static>> {
        match v {
            axiom_ir::ValueRef::BlockArg { param_index, .. } => {
                // Bounds-check so a function param referenced from a nested
                // scf region (whose local store has 0 block args) falls back
                // to the parent store instead of panicking.
                self.block_args.get(*param_index).copied().flatten()
            }
            axiom_ir::ValueRef::OpResult {
                op_index,
                result_index,
                ..
            } => {
                let s = op_index * MAX_RESULTS + result_index;
                self.op_results.get(s).copied().flatten()
            }
        }
    }

    fn resolve(&self, v: &axiom_ir::ValueRef) -> Value<'c, 'static> {
        match v {
            axiom_ir::ValueRef::BlockArg { param_index, .. } => {
                self.block_args[*param_index].expect("Block arg not stored")
            }
            axiom_ir::ValueRef::OpResult {
                op_index,
                result_index,
                ..
            } => {
                let s = op_index * MAX_RESULTS + result_index;
                self.op_results[s].expect("Op result not yet emitted — forward reference")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Function emission
// ---------------------------------------------------------------------------

/// Emit a single `func.func` operation.
fn emit_function<'c>(
    context: &'c Context,
    func_def: &axiom_ir::FunctionDef,
    emit_c_interface: bool,
    location: Location<'c>,
) -> melior::ir::Operation<'c> {
    let num_params = func_def.params.len();
    let num_ops = func_def.body.ops.len();

    // 1. Build the block with params matching function arguments
    let param_types: Vec<(Type<'c>, Location<'c>)> = func_def
        .params
        .iter()
        .map(|p| (convert_type(context, &p.typ), location))
        .collect();

    let block = Block::new(&param_types);
    let mut store = ValueStore::new(num_params, num_ops);

    // Store block arguments as values
    for i in 0..num_params {
        if let Ok(arg) = block.argument(i) {
            let v: Value<'c, '_> = arg.into();
            store.store_block_arg(i, &v);
        }
    }

    // 2. Emit each operation in order
    for (op_idx, op) in func_def.body.ops.iter().enumerate() {
        match op {
            axiom_ir::Operation::Constant(c) => {
                let melior_type = convert_type(context, &c.typ);
                let attr: melior::ir::Attribute = IntegerAttribute::new(c.value, melior_type).into();
                let op_ref = block.append_operation(arith::constant(context, attr, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Addi(a) => {
                let lhs = store.resolve(&a.lhs);
                let rhs = store.resolve(&a.rhs);
                let op_ref = block.append_operation(arith::addi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Subi(s) => {
                let lhs = store.resolve(&s.lhs);
                let rhs = store.resolve(&s.rhs);
                let op_ref = block.append_operation(arith::subi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Muli(m) => {
                let lhs = store.resolve(&m.lhs);
                let rhs = store.resolve(&m.rhs);
                let op_ref = block.append_operation(arith::muli(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Addf(a) => {
                let lhs = store.resolve(&a.lhs);
                let rhs = store.resolve(&a.rhs);
                let op_ref = block.append_operation(arith::addf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Subf(s) => {
                let lhs = store.resolve(&s.lhs);
                let rhs = store.resolve(&s.rhs);
                let op_ref = block.append_operation(arith::subf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Mulf(m) => {
                let lhs = store.resolve(&m.lhs);
                let rhs = store.resolve(&m.rhs);
                let op_ref = block.append_operation(arith::mulf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Cmpi(cmp) => {
                let lhs = store.resolve(&cmp.lhs);
                let rhs = store.resolve(&cmp.rhs);
                let predicate = match cmp.predicate {
                    axiom_ir::CmpiPredicate::Eq => arith::CmpiPredicate::Eq,
                    axiom_ir::CmpiPredicate::Ne => arith::CmpiPredicate::Ne,
                    axiom_ir::CmpiPredicate::Slt => arith::CmpiPredicate::Slt,
                    axiom_ir::CmpiPredicate::Sgt => arith::CmpiPredicate::Sgt,
                    axiom_ir::CmpiPredicate::Sle => arith::CmpiPredicate::Sle,
                    axiom_ir::CmpiPredicate::Sge => arith::CmpiPredicate::Sge,
                };
                let op_ref =
                    block.append_operation(arith::cmpi(context, predicate, lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Return(r) => {
                let operands: Vec<Value<'c, '_>> = r
                    .operands
                    .iter()
                    .map(|v| store.resolve(v))
                    .collect();
                block.append_operation(func::r#return(&operands, location));
            }

            axiom_ir::Operation::ScfIf(if_op) => {
                let condition = store.resolve(&if_op.condition);

                let result_types: Vec<Type<'c>> = if_op
                    .result_types
                    .iter()
                    .map(|t| convert_type(context, t))
                    .collect();

                // Emit then region — references values from parent store
                let then_region = build_scf_region(context, &if_op.then_block, &store, location);

                // Emit else region (or empty)
                let else_region = if let Some(ref else_body) = if_op.else_block {
                    build_scf_region(context, else_body, &store, location)
                } else {
                    Region::new()
                };

                let op_ref = if result_types.is_empty() {
                    block.append_operation(scf::r#if(
                        condition,
                        &[],
                        then_region,
                        else_region,
                        location,
                    ))
                } else {
                    block.append_operation(scf::r#if(
                        condition,
                        &result_types,
                        then_region,
                        else_region,
                        location,
                    ))
                };

                // Store results of scf.if
                for i in 0..if_op.result_types.len() {
                    if let Ok(val) = op_ref.result(i) {
                        let v: Value<'c, '_> = val.into();
                        store.store_op_result(op_idx, i, &v);
                    }
                }
            }

            axiom_ir::Operation::ScfFor(for_op) => {
                let lower_bound = store.resolve(&for_op.lower_bound);
                let upper_bound = store.resolve(&for_op.upper_bound);
                let step = store.resolve(&for_op.step);
                // melior's `scf::r#for` takes only (start, end, step); loop-carried
                // iter_args are resolved here for future support but not emitted yet.
                let _iter_args: Vec<Value<'c, '_>> = for_op
                    .iter_args
                    .iter()
                    .map(|v| store.resolve(v))
                    .collect();

                let loop_region = build_scf_loop_region(context, &for_op.body_block, &store, location);

                let op_ref = block.append_operation(scf::r#for(
                    lower_bound,
                    upper_bound,
                    step,
                    loop_region,
                    location,
                ));

                for i in 0..for_op.result_types.len() {
                    if let Ok(val) = op_ref.result(i) {
                        let v: Value<'c, '_> = val.into();
                        store.store_op_result(op_idx, i, &v);
                    }
                }
            }

            axiom_ir::Operation::ScfYield(y) => {
                let operands: Vec<Value<'c, '_>> = y
                    .operands
                    .iter()
                    .map(|v| store.resolve(v))
                    .collect();
                block.append_operation(scf::r#yield(&operands, location));
            }

            axiom_ir::Operation::Call(call) => {
                let args: Vec<Value<'c, '_>> = call
                    .args
                    .iter()
                    .map(|v| store.resolve(v))
                    .collect();
                let result_types: Vec<Type<'c>> = vec![convert_type(context, &call.typ)];
                let op_ref = block.append_operation(func::call(
                    context,
                    melior::ir::attribute::FlatSymbolRefAttribute::new(context, &call.name),
                    &args,
                    &result_types,
                    location,
                ));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    store.store_op_result(op_idx, 0, &v);
                }
            }
        }
    }

    // 3. Wrap the block into a region and create func.func
    let region = Region::new();
    region.append_block(block);

    let input_types: Vec<Type<'c>> = func_def
        .params
        .iter()
        .map(|p| convert_type(context, &p.typ))
        .collect();
    let output_types: Vec<Type<'c>> = func_def
        .return_types
        .iter()
        .map(|t| convert_type(context, t))
        .collect();

    let function_type = FunctionType::new(context, &input_types, &output_types);

    let mut attrs: Vec<(Identifier<'c>, Attribute<'c>)> = vec![];
    if emit_c_interface {
        attrs.push((
            Identifier::new(context, "llvm.emit_c_interface"),
            Attribute::unit(context),
        ));
    }

    func::func(
        context,
        StringAttribute::new(context, &func_def.name),
        TypeAttribute::new(function_type.into()),
        region,
        &attrs,
        location,
    )
}

// ---------------------------------------------------------------------------
// SCF region builder
// ---------------------------------------------------------------------------

/// Build an SCF region block (for `scf.if` regions).
///
/// `parent_store` provides access to values from the parent block.
fn build_scf_region<'c>(
    context: &'c Context,
    body: &axiom_ir::Block,
    parent_store: &ValueStore<'c>,
    location: Location<'c>,
) -> Region<'c> {
    let block = Block::new(&[]);
    let num_ops = body.ops.len();
    let mut local_store = ValueStore::new(0, num_ops);

    // Helper: resolve a value from the local scf region first, then fall back
    // to the parent block's store.
    let resolve_scoped =
        |v: &axiom_ir::ValueRef, local: &ValueStore<'c>, parent: &ValueStore<'c>|
         -> Value<'c, 'static> {
            local
                .try_resolve(v)
                .or_else(|| parent.try_resolve(v))
                .expect("Value not found in either scf region or parent scope")
        };

    for (op_idx, op) in body.ops.iter().enumerate() {
        match op {
            axiom_ir::Operation::Constant(c) => {
                let melior_type = convert_type(context, &c.typ);
                let attr: melior::ir::Attribute =
                    IntegerAttribute::new(c.value, melior_type).into();
                let op_ref = block.append_operation(arith::constant(context, attr, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::ScfYield(y) => {
                // Resolve operands from parent store or scf region.
                let operands: Vec<Value<'c, '_>> = y
                    .operands
                    .iter()
                    .map(|v| resolve_scoped(v, &local_store, parent_store))
                    .collect();
                block.append_operation(scf::r#yield(&operands, location));
            }
            axiom_ir::Operation::Addi(a) => {
                let lhs = resolve_scoped(&a.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&a.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::addi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::Subi(s) => {
                let lhs = resolve_scoped(&s.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&s.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::subi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Muli(m) => {
                let lhs = resolve_scoped(&m.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&m.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::muli(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Addf(a) => {
                let lhs = resolve_scoped(&a.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&a.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::addf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Subf(s) => {
                let lhs = resolve_scoped(&s.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&s.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::subf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Mulf(m) => {
                let lhs = resolve_scoped(&m.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&m.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::mulf(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Cmpi(cmp) => {
                let lhs = resolve_scoped(&cmp.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&cmp.rhs, &local_store, parent_store);
                let predicate = match cmp.predicate {
                    axiom_ir::CmpiPredicate::Eq => arith::CmpiPredicate::Eq,
                    axiom_ir::CmpiPredicate::Ne => arith::CmpiPredicate::Ne,
                    axiom_ir::CmpiPredicate::Slt => arith::CmpiPredicate::Slt,
                    axiom_ir::CmpiPredicate::Sgt => arith::CmpiPredicate::Sgt,
                    axiom_ir::CmpiPredicate::Sle => arith::CmpiPredicate::Sle,
                    axiom_ir::CmpiPredicate::Sge => arith::CmpiPredicate::Sge,
                };
                let op_ref =
                    block.append_operation(arith::cmpi(context, predicate, lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Call(call) => {
                let args: Vec<Value<'c, '_>> = call
                    .args
                    .iter()
                    .map(|v| resolve_scoped(v, &local_store, parent_store))
                    .collect();
                let result_types: Vec<Type<'c>> = vec![convert_type(context, &call.typ)];
                let op_ref = block.append_operation(func::call(
                    context,
                    melior::ir::attribute::FlatSymbolRefAttribute::new(context, &call.name),
                    &args,
                    &result_types,
                    location,
                ));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }

            axiom_ir::Operation::Return(r) => {
                // A bare `return` inside an scf region is invalid scf IR.
                // Convert it to an scf.yield so it terminates the block correctly.
                let operands: Vec<Value<'c, '_>> = r
                    .operands
                    .iter()
                    .map(|v| resolve_scoped(v, &local_store, parent_store))
                    .collect();
                block.append_operation(scf::r#yield(&operands, location));
            }

            axiom_ir::Operation::ScfIf(if_op) => {
                let condition = resolve_scoped(&if_op.condition, &local_store, parent_store);

                let result_types: Vec<Type<'c>> = if_op
                    .result_types
                    .iter()
                    .map(|t| convert_type(context, t))
                    .collect();

                let then_region = build_scf_region(context, &if_op.then_block, parent_store, location);
                let else_region = if let Some(ref else_body) = if_op.else_block {
                    build_scf_region(context, else_body, parent_store, location)
                } else {
                    Region::new()
                };

                let op_ref = if result_types.is_empty() {
                    block.append_operation(scf::r#if(
                        condition,
                        &[],
                        then_region,
                        else_region,
                        location,
                    ))
                } else {
                    block.append_operation(scf::r#if(
                        condition,
                        &result_types,
                        then_region,
                        else_region,
                        location,
                    ))
                };

                for i in 0..if_op.result_types.len() {
                    if let Ok(val) = op_ref.result(i) {
                        let v: Value<'c, '_> = val.into();
                        local_store.store_op_result(op_idx, i, &v);
                    }
                }
            }

            axiom_ir::Operation::ScfFor(for_op) => {
                let lower_bound = resolve_scoped(&for_op.lower_bound, &local_store, parent_store);
                let upper_bound = resolve_scoped(&for_op.upper_bound, &local_store, parent_store);
                let step = resolve_scoped(&for_op.step, &local_store, parent_store);
                // melior's `scf::r#for` takes only (start, end, step); loop-carried
                // iter_args are resolved here for future support but not emitted yet.
                let _iter_args: Vec<Value<'c, '_>> = for_op
                    .iter_args
                    .iter()
                    .map(|v| resolve_scoped(v, &local_store, parent_store))
                    .collect();

                let loop_region = build_scf_loop_region(context, &for_op.body_block, parent_store, location);

                let op_ref = block.append_operation(scf::r#for(
                    lower_bound,
                    upper_bound,
                    step,
                    loop_region,
                    location,
                ));

                for i in 0..for_op.result_types.len() {
                    if let Ok(val) = op_ref.result(i) {
                        let v: Value<'c, '_> = val.into();
                        local_store.store_op_result(op_idx, i, &v);
                    }
                }
            }
        }
    }

    let region = Region::new();
    region.append_block(block);
    region
}

/// Build an SCF loop region block (for `scf.for` regions).
fn build_scf_loop_region<'c>(
    context: &'c Context,
    body: &axiom_ir::Block,
    parent_store: &ValueStore<'c>,
    location: Location<'c>,
) -> Region<'c> {
    let index_type: Type = Type::index(context);
    let block = Block::new(&[(index_type, location)]);
    let num_ops = body.ops.len();
    let mut local_store = ValueStore::new(0, num_ops);

    let resolve_scoped =
        |v: &axiom_ir::ValueRef, local: &ValueStore<'c>, parent: &ValueStore<'c>|
         -> Value<'c, 'static> {
            local
                .try_resolve(v)
                .or_else(|| parent.try_resolve(v))
                .expect("Value not found in either scf region or parent scope")
        };

    for (op_idx, op) in body.ops.iter().enumerate() {
        match op {
            axiom_ir::Operation::Constant(c) => {
                let melior_type = convert_type(context, &c.typ);
                let attr: melior::ir::Attribute =
                    IntegerAttribute::new(c.value, melior_type).into();
                let op_ref = block.append_operation(arith::constant(context, attr, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::Addi(a) => {
                let lhs = resolve_scoped(&a.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&a.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::addi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::Subi(s) => {
                let lhs = resolve_scoped(&s.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&s.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::subi(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::Muli(m) => {
                let lhs = resolve_scoped(&m.lhs, &local_store, parent_store);
                let rhs = resolve_scoped(&m.rhs, &local_store, parent_store);
                let op_ref = block.append_operation(arith::muli(lhs, rhs, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
            axiom_ir::Operation::ScfYield(y) => {
                let operands: Vec<Value<'c, '_>> = y
                    .operands
                    .iter()
                    .map(|v| resolve_scoped(v, &local_store, parent_store))
                    .collect();
                block.append_operation(scf::r#yield(&operands, location));
            }
            _ => {
                let i64_type: Type = IntegerType::new(context, 64).into();
                let attr: melior::ir::Attribute = IntegerAttribute::new(0, i64_type).into();
                let op_ref = block.append_operation(arith::constant(context, attr, location));
                if let Ok(val) = op_ref.result(0) {
                    let v: Value<'c, '_> = val.into();
                    local_store.store_op_result(op_idx, 0, &v);
                }
            }
        }
    }

    let region = Region::new();
    region.append_block(block);
    region
}

// ---------------------------------------------------------------------------
// Convenience: emit the built-in examples from axiom_ir
// ---------------------------------------------------------------------------

/// Emit the "constant seven" example.
pub fn emit_constant_seven<'c>(context: &'c Context) -> Module<'c> {
    emit_module(context, &axiom_ir::build_constant_seven(), false)
}

/// Emit the "add two numbers" example.
pub fn emit_add_example<'c>(context: &'c Context) -> Module<'c> {
    emit_module(context, &axiom_ir::build_add_example(), false)
}

/// Emit the "max" conditional example.
pub fn emit_max_example<'c>(context: &'c Context) -> Module<'c> {
    emit_module(context, &axiom_ir::build_max_example(), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set up a context with all dialects loaded.
    fn create_context() -> Context {
        let context = Context::new();
        let registry = DialectRegistry::new();
        register_all_dialects(&registry);
        context.append_dialect_registry(&registry);
        context.load_all_available_dialects();
        context
    }

    #[test]
    fn emit_constant_seven_roundtrip() {
        let context = create_context();
        let module = emit_constant_seven(&context);
        assert!(module.as_operation().verify());
        let mlir_text = format!("{}", module.as_operation());
        assert!(mlir_text.contains("@seven"));
        assert!(mlir_text.contains("arith.constant"));
        assert!(mlir_text.contains("return"));
    }

    #[test]
    fn emit_add_example_roundtrip() {
        let context = create_context();
        let module = emit_add_example(&context);
        assert!(module.as_operation().verify());
        let mlir_text = format!("{}", module.as_operation());
        assert!(mlir_text.contains("@add"));
        assert!(mlir_text.contains("arith.addi"));
    }

    #[test]
    fn emit_max_example_roundtrip() {
        let context = create_context();
        let module = emit_max_example(&context);
        assert!(module.as_operation().verify());
        let mlir_text = format!("{}", module.as_operation());
        assert!(mlir_text.contains("@max"));
        assert!(mlir_text.contains("arith.cmpi"));
        assert!(mlir_text.contains("scf.if"));
    }

    #[test]
    fn emit_empty_module() {
        let context = create_context();
        let module = emit_module(&context, &axiom_ir::AxiomModule::new(vec![]), false);
        assert!(module.as_operation().verify());
    }

    #[test]
    fn emit_multiple_functions() {
        let context = create_context();
        let mut functions = vec![];
        if let Some(f) = axiom_ir::build_constant_seven().functions.into_iter().next() {
            functions.push(f);
        }
        if let Some(f) = axiom_ir::build_add_example().functions.into_iter().next() {
            functions.push(f);
        }
        let module = axiom_ir::AxiomModule::new(functions);
        let module = emit_module(&context, &module, false);
        assert!(module.as_operation().verify());
        let mlir_text = format!("{}", module.as_operation());
        assert!(mlir_text.contains("@seven"));
        assert!(mlir_text.contains("@add"));
    }
}
