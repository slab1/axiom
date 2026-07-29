//! High-level Axiom AST — expression-based, human-readable.
//!
//! This AST is produced by the `.ax` surface parser (`parser`) and lowered
//! to `axiom_ir::AxiomModule` by the lowerer (`lower`). It supports:
//!
//! - Expressions: literals, variables, binary ops, if/else, calls, blocks
//! - Statements: let bindings, expression statements
//! - Function definitions with params, return types, and effect annotations
//! - Module-level file organization

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Surface-level types — the names users write.
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    I1,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Unit,
    /// Named types (user-defined, future: structs, enums).
    Named(String),
}

/// Map a surface type to a core IR type.
pub fn to_core_type(t: &Type) -> crate::axiom_ir::Type {
    match t {
        Type::I1 | Type::Bool => crate::axiom_ir::Type::I1,
        Type::I8 | Type::U8 => crate::axiom_ir::Type::I8,
        Type::I16 | Type::U16 => crate::axiom_ir::Type::I16,
        Type::I32 | Type::U32 => crate::axiom_ir::Type::I32,
        Type::I64 | Type::U64 => crate::axiom_ir::Type::I64,
        Type::F32 => crate::axiom_ir::Type::F32,
        Type::F64 => crate::axiom_ir::Type::F64,
        Type::Unit => crate::axiom_ir::Type::I1, // unit represented as i1
        Type::Named(_) => crate::axiom_ir::Type::I64, // placeholder
    }
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal.
    Int(i64, Type),
    /// Float literal.
    Float(f64, Type),
    /// Bool literal.
    Bool(bool),
    /// Variable reference.
    Var(String),
    /// Block expression: a sequence of statements ending with an expression.
    Block(Vec<Stmt>, Box<Expr>),
    /// Binary operation.
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// Unary operation.
    Unary(UnOp, Box<Expr>),
    /// If/else expression.
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    /// Function call.
    Call(String, Vec<Expr>),
    /// Effect handler expression: `with Effect = effect Effect { op(args) { body } } { expr }`
    Handle(String, Vec<EffectHandlerOp>, Box<Expr>),
    /// Match expression: `match expr { pat => body, ... }`
    Match(Box<Expr>, Vec<MatchArm>),
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct StructDef {
    pub name: String,
    pub fields: Vec<(String, Type)>,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub fields: Vec<Type>,
}

#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name: String,
    pub variants: Vec<EnumVariant>,
}

#[derive(Debug, Clone)]
pub struct EffectHandlerOp {
    pub name: String,
    pub params: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Stmt {
    /// `let name = expr`
    Let(String, Option<Type>, Expr),
    /// Expression statement (result discarded unless tail).
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// Function definitions
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub typ: Type,
}

#[derive(Debug, Clone)]
pub struct FunctionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    /// Effect annotation (e.g. ~Io, ~Pure).
    pub effects: Vec<String>,
    pub body: Expr,
}

#[derive(Debug, Clone)]
pub struct ExternFunctionDef {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub abi: String,
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

/// A parsed `.ax` source file.
#[derive(Debug, Clone)]
pub struct Module {
    pub functions: Vec<FunctionDef>,
    pub extern_functions: Vec<ExternFunctionDef>,
    pub structs: Vec<StructDef>,
    pub enums: Vec<EnumDef>,
}

impl Module {
    pub fn new(functions: Vec<FunctionDef>) -> Self {
        Module {
            functions,
            extern_functions: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
        }
    }
}
