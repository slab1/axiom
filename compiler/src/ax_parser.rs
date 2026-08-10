//! Expression-based `.ax` parser.
//!
//! Produces `ast::Module` from human-readable Axiom source text.
//! Supports: expressions, let-bindings, if/else, function calls, blocks.
//!
//! # Syntax
//!
//! ```text
//! fn add(a: I64, b: I64) -> I64 {
//!     a + b
//! }
//!
//! fn fib(n: I64) -> I64 {
//!     if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
//! }
//! ```

use crate::ast::{self, BinOp, Expr, Stmt, UnOp};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {} col {}: {}", self.line, self.col, self.message)
    }
}

impl std::error::Error for ParseError {}

type Result<T> = std::result::Result<T, ParseError>;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Parse an `.ax` source string into an `ast::Module`.
pub fn parse_source(source: &str) -> Result<ast::Module> {
    let mut c = Cursor::new(source);
    let mut functions = Vec::new();
    let mut extern_functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    c.skip_ws_and_comments();
    while c.pos < c.input.len() {
        c.skip_ws_and_comments();
        let kw = peek_keyword(&mut c);
        match kw.as_str() {
            "extern" => extern_functions.push(parse_extern_function(&mut c)?),
            "struct" => structs.push(parse_struct_def(&mut c)?),
            "enum" => enums.push(parse_enum_def(&mut c)?),
            _ => functions.push(parse_function(&mut c)?),
        }
        c.skip_ws_and_comments();
    }
    let mut m = ast::Module::new(functions);
    m.extern_functions = extern_functions;
    m.structs = structs;
    m.enums = enums;
    Ok(m)
}

fn parse_extern_function(c: &mut Cursor) -> Result<ast::ExternFunctionDef> {
    c.expect_keyword("extern")?;
    c.skip_ws_and_comments();
    let abi = if c.peek() == Some('"') {
        c.advance();
        let start = c.pos;
        while let Some(ch) = c.peek() {
            if ch == '"' { break; }
            c.advance();
        }
        let abi_str = c.input[start..c.pos].to_string();
        c.expect_char('"')?;
        abi_str
    } else {
        "C".to_string()
    };

    c.expect_keyword("fn")?;
    let name = c.parse_name()?;
    c.expect_char('(')?;

    let mut params = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some(')') { c.advance(); break; }
        if !params.is_empty() { c.expect_char(',')?; }
        let pname = c.parse_name()?;
        c.expect_char(':')?;
        let ptype = parse_type(c)?;
        params.push(ast::Param { name: pname, typ: ptype });
    }

    let return_type = if c.try_char('-') {
        c.expect_char('>')?;
        Some(parse_type(c)?)
    } else {
        None
    };

    c.expect_char(';')?;

    Ok(ast::ExternFunctionDef {
        name,
        params,
        return_type,
        abi,
    })
}

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

struct Cursor<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Cursor<'a> {
    fn new(input: &'a str) -> Self {
        Cursor { input, pos: 0, line: 1, col: 1 }
    }

    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) {
        if let Some(c) = self.input[self.pos..].chars().next() {
            self.pos += c.len_utf8();
            if c == '\n' { self.line += 1; self.col = 1; }
            else { self.col += 1; }
        }
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_ascii_whitespace() { self.advance(); }
            else { break; }
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            self.skip_ws();
            if self.rest().starts_with("//") {
                while let Some(c) = self.peek() {
                    if c == '\n' { self.advance(); break; }
                    self.advance();
                }
            } else { break; }
        }
    }

    fn expect_char(&mut self, ch: char) -> Result<()> {
        self.skip_ws_and_comments();
        if self.peek() == Some(ch) { self.advance(); Ok(()) }
        else {
            Err(self.err(format!("expected '{ch}', got {:?}", self.peek())))
        }
    }

    fn try_char(&mut self, ch: char) -> bool {
        self.skip_ws_and_comments();
        if self.peek() == Some(ch) { self.advance(); true }
        else { false }
    }

    fn parse_name(&mut self) -> Result<String> {
        self.skip_ws_and_comments();
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' { self.advance(); }
            else { break; }
        }
        if self.pos == start {
            return Err(self.err("expected identifier".into()));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_int(&mut self) -> Result<i64> {
        self.skip_ws_and_comments();
        let start = self.pos;
        if self.peek() == Some('-') { self.advance(); }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() { self.advance(); }
            else { break; }
        }
        let s = &self.input[start..self.pos];
        if s.is_empty() || s == "-" {
            return Err(self.err("expected integer literal".into()));
        }
        s.parse().map_err(|_| self.err(format!("invalid integer '{s}'")))
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<()> {
        self.skip_ws_and_comments();
        if self.rest().starts_with(kw) {
            // Check that the keyword is not followed by more alphanumeric chars
            let after = &self.rest()[kw.len()..];
            if after.chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_') {
                for _ in kw.chars() { self.advance(); }
                return Ok(());
            }
        }
        Err(self.err(format!("expected '{kw}'")))
    }

    fn err(&self, msg: String) -> ParseError {
        ParseError { line: self.line, col: self.col, message: msg }
    }
}

// ---------------------------------------------------------------------------
// Function parsing
// ---------------------------------------------------------------------------

fn parse_function(c: &mut Cursor) -> Result<ast::FunctionDef> {
    c.expect_keyword("fn")?;
    let name = c.parse_name()?;
    c.expect_char('(')?;

    let mut params = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some(')') { c.advance(); break; }
        if !params.is_empty() { c.expect_char(',')?; }
        let pname = c.parse_name()?;
        c.expect_char(':')?;
        let ptype = parse_type(c)?;
        params.push(ast::Param { name: pname, typ: ptype });
    }

    // Optional return type
    let return_type = if c.try_char('-') {
        c.expect_char('>')?;
        Some(parse_type(c)?)
    } else {
        None
    };

    // Optional effect annotation: ~ Effect1 + Effect2
    let mut effects = Vec::new();
    if c.try_char('~') {
        loop {
            let eff = c.parse_name()?;
            effects.push(eff);
            c.skip_ws_and_comments();
            if c.peek() == Some('+') { c.advance(); }
            else { break; }
        }
    }

    // Parse body (block expression)
    let body = parse_block_expr(c)?;

    Ok(ast::FunctionDef {
        name,
        params,
        return_type,
        effects,
        body,
    })
}

// ---------------------------------------------------------------------------
// Type parsing
// ---------------------------------------------------------------------------

fn parse_type(c: &mut Cursor) -> Result<ast::Type> {
    let name = c.parse_name()?;
    match name.as_str() {
        "I1" => Ok(ast::Type::I1),
        "I8" => Ok(ast::Type::I8),
        "I16" => Ok(ast::Type::I16),
        "I32" => Ok(ast::Type::I32),
        "I64" => Ok(ast::Type::I64),
        "U8" => Ok(ast::Type::U8),
        "U16" => Ok(ast::Type::U16),
        "U32" => Ok(ast::Type::U32),
        "U64" => Ok(ast::Type::U64),
        "F32" => Ok(ast::Type::F32),
        "F64" => Ok(ast::Type::F64),
        "Bool" => Ok(ast::Type::Bool),
        "Unit" => Ok(ast::Type::Unit),
        other => Ok(ast::Type::Named(other.to_string())),
    }
}

// ---------------------------------------------------------------------------
// Expression parsing with precedence climbing
// ---------------------------------------------------------------------------

/// Operator precedence table (higher = binds tighter).
fn precedence(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Eq | BinOp::Ne => 3,
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => 4,
        BinOp::Add | BinOp::Sub => 5,
        BinOp::Mul | BinOp::Div | BinOp::Mod => 6,
    }
}

/// Parse an expression with minimum precedence `min_prec`.
fn parse_expr(c: &mut Cursor, min_prec: u8) -> Result<Expr> {
    let mut lhs = parse_prefix(c)?;

    loop {
        c.skip_ws_and_comments();
        let op = match c.peek() {
            Some('+') => { c.advance(); BinOp::Add }
            Some('-') => { c.advance(); BinOp::Sub }
            Some('*') => { c.advance(); BinOp::Mul }
            Some('/') => { c.advance(); BinOp::Div }
            Some('%') => { c.advance(); BinOp::Mod }
            Some('=') => {
                if c.rest().starts_with("==") { c.advance(); c.advance(); BinOp::Eq }
                else { break }
            }
            Some('!') => {
                if c.rest().starts_with("!=") { c.advance(); c.advance(); BinOp::Ne }
                else { break }
            }
            Some('<') => {
                if c.rest().starts_with("<=") { c.advance(); c.advance(); BinOp::Le }
                else { c.advance(); BinOp::Lt }
            }
            Some('>') => {
                if c.rest().starts_with(">=") { c.advance(); c.advance(); BinOp::Ge }
                else { c.advance(); BinOp::Gt }
            }
            Some('&') => {
                if c.rest().starts_with("&&") { c.advance(); c.advance(); BinOp::And }
                else { break }
            }
            Some('|') => {
                if c.rest().starts_with("||") { c.advance(); c.advance(); BinOp::Or }
                else { break }
            }
            _ => break,
        };

        let prec = precedence(op);
        if prec < min_prec { break; }

        let rhs = parse_expr(c, prec + 1)?;
        lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
    }

    Ok(lhs)
}

/// Parse a prefix expression (literal, variable, if, let, block, unary, call).
fn parse_prefix(c: &mut Cursor) -> Result<Expr> {
    c.skip_ws_and_comments();

    match c.peek() {
        Some('0'..='9') => {
            let val = c.parse_int()?;
            Ok(Expr::Int(val, ast::Type::I64))
        }
        Some('-') => {
            // Could be unary minus or negative literal
            let saved = (c.pos, c.line, c.col);
            c.advance();
            c.skip_ws();
            if c.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                // Negative literal (restore '-' and re-parse as int)
                c.pos = saved.0; c.line = saved.1; c.col = saved.2;
                let val = c.parse_int()?;
                Ok(Expr::Int(val, ast::Type::I64))
            } else {
                // Unary minus
                let expr = parse_expr(c, 7)?;
                Ok(Expr::Unary(UnOp::Neg, Box::new(expr)))
            }
        }
        Some('!') => {
            c.advance();
            let expr = parse_expr(c, 7)?;
            Ok(Expr::Unary(UnOp::Not, Box::new(expr)))
        }
        Some('(') => {
            c.advance();
            let expr = parse_expr(c, 0)?;
            c.expect_char(')')?;
            Ok(expr)
        }
        Some('{') => {
            parse_block_expr(c)
        }
        _ => {
            // Keyword-starting or name-starting expression
            let kw = peek_keyword(c);
            match kw.as_str() {
                "if" => parse_if_expr(c),
                "let" => parse_let_expr(c),
                "with" => parse_with_expr(c),
                "match" => parse_match_expr(c),
                "true" => { consume_keyword(c); Ok(Expr::Bool(true)) }
                "false" => { consume_keyword(c); Ok(Expr::Bool(false)) }
                _ => {
                    // Variable reference or function call
                    let name = c.parse_name()?;
                    if c.try_char('(') {
                        parse_call_args(c, name)
                    } else {
                        Ok(Expr::Var(name))
                    }
                }
            }
        }
    }
}

/// Parse a let expression: `let name = expr ; tail`
fn parse_let_expr(c: &mut Cursor) -> Result<Expr> {
    consume_keyword(c); // "let"
    let name = c.parse_name()?;

    // Optional : Type
    let typ = if c.try_char(':') {
        Some(parse_type(c)?)
    } else {
        None
    };

    c.expect_char('=')?;

    let value = parse_expr(c, 0)?;

    // Semi-colon optional if followed by newline
    c.skip_ws_and_comments();
    // The tail is everything after the let
    // We parse the rest as a block's body
    let tail = parse_expr(c, 0)?;

    Ok(Expr::Block(
        vec![Stmt::Let(name, typ, value)],
        Box::new(tail),
    ))
}

/// Parse an if expression: `if cond { then } else { else }`
fn parse_if_expr(c: &mut Cursor) -> Result<Expr> {
    consume_keyword(c); // "if"
    let cond = parse_expr(c, 0)?;
    let then_branch = parse_block_expr(c)?;
    c.skip_ws_and_comments();
    let else_branch = if c.rest().starts_with("else") {
        consume_keyword(c); // "else"
        c.skip_ws_and_comments();
        if c.peek() == Some('{') || c.peek() == Some('i') {
            // Block or another if
            let else_expr = if c.peek() == Some('i') {
                parse_if_expr(c)?
            } else {
                parse_block_expr(c)?
            };
            Some(Box::new(else_expr))
        } else {
            return Err(c.err("expected block or if after 'else'".into()));
        }
    } else {
        None
    };
    Ok(Expr::If(Box::new(cond), Box::new(then_branch), else_branch))
}

fn parse_struct_def(c: &mut Cursor) -> Result<ast::StructDef> {
    consume_keyword(c); // "struct"
    let name = c.parse_name()?;
    c.expect_char('{')?;
    let mut fields = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some('}') { c.advance(); break; }
        let fname = c.parse_name()?;
        c.expect_char(':')?;
        let ftype = parse_type(c)?;
        fields.push((fname, ftype));
        c.try_char(',');
    }
    Ok(ast::StructDef { name, fields })
}

fn parse_enum_def(c: &mut Cursor) -> Result<ast::EnumDef> {
    consume_keyword(c); // "enum"
    let name = c.parse_name()?;
    if c.try_char('[') {
        while let Some(ch) = c.peek() {
            if ch == ']' { c.advance(); break; }
            c.advance();
        }
    }
    c.expect_char('{')?;
    let mut variants = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some('}') { c.advance(); break; }
        let vname = c.parse_name()?;
        let mut fields = Vec::new();
        if c.try_char('(') {
            loop {
                c.skip_ws_and_comments();
                if c.peek() == Some(')') { c.advance(); break; }
                if !fields.is_empty() { c.expect_char(',')?; }
                fields.push(parse_type(c)?);
            }
        }
        variants.push(ast::EnumVariant { name: vname, fields });
        c.try_char(',');
    }
    Ok(ast::EnumDef { name, variants })
}

fn parse_match_expr(c: &mut Cursor) -> Result<Expr> {
    consume_keyword(c); // "match"
    let scrutinee = parse_expr(c, 0)?;
    c.expect_char('{')?;
    let mut arms = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some('}') { c.advance(); break; }
        let pat = c.parse_name()?;
        c.expect_char('=')?;
        c.expect_char('>')?;
        let body = parse_expr(c, 0)?;
        arms.push(ast::MatchArm { pattern: pat, body });
        c.try_char(',');
    }
    Ok(Expr::Match(Box::new(scrutinee), arms))
}

/// Parse `with Effect = effect Effect { op(params) { body } } { expr }`
fn parse_with_expr(c: &mut Cursor) -> Result<Expr> {
    consume_keyword(c); // "with"
    let effect_name = c.parse_name()?;
    c.expect_char('=')?;
    c.expect_keyword("effect")?;
    let _effect_name_ref = c.parse_name()?;
    c.expect_char('{')?;

    let mut ops = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some('}') { c.advance(); break; }
        let op_name = c.parse_name()?;
        c.expect_char('(')?;
        let mut params = Vec::new();
        loop {
            c.skip_ws_and_comments();
            if c.peek() == Some(')') { c.advance(); break; }
            if !params.is_empty() { c.expect_char(',')?; }
            params.push(c.parse_name()?);
        }
        let body = parse_block_expr(c)?;
        ops.push(ast::EffectHandlerOp {
            name: op_name,
            params,
            body,
        });
    }

    c.skip_ws_and_comments();
    let body_expr = parse_block_expr(c)?;

    Ok(Expr::Handle(effect_name, ops, Box::new(body_expr)))
}

/// Parse a block expression: `{ stmt; stmt; expr }`
/// The last expression is the block's value.
fn parse_block_expr(c: &mut Cursor) -> Result<Expr> {
    c.expect_char('{')?;
    let mut stmts: Vec<Stmt> = Vec::new();

    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some('}') { c.advance(); break; }
        if c.peek().is_none() { return Err(c.err("unterminated block".into())); }

        // Try to parse a statement
        if let Some('l') = c.peek()
            && (c.rest().starts_with("let ") || c.rest().starts_with("let\n") || c.rest().starts_with("let\t") || c.rest().starts_with("let\r"))
        {
            // Actually check if it's the `let` keyword
            let _saved = (c.pos, c.line, c.col);
            consume_keyword(c); // "let"
            let name = c.parse_name()?;
            let typ = if c.try_char(':') { Some(parse_type(c)?) } else { None };
            c.expect_char('=')?;
            let value = parse_expr(c, 0)?;
            stmts.push(Stmt::Let(name, typ, value));
            // Optional semicolon
            c.skip_ws_and_comments();
            if c.peek() == Some(';') { c.advance(); }
            continue;
        }

        // It's an expression
        let expr = parse_expr(c, 0)?;

        // Check what follows: `;` or `}` or other
        c.skip_ws_and_comments();
        if c.peek() == Some(';') {
            stmts.push(Stmt::Expr(expr));
            c.advance(); // consume semicolon
            continue;
        }
        if c.peek() == Some('}') {
            stmts.push(Stmt::Expr(expr));
            continue; // let the loop handle `}`
        }

        stmts.push(Stmt::Expr(expr));
    }

    // Build the block: if the last statement is an expr, it's the return value
    if let Some(Stmt::Expr(tail)) = stmts.pop() {
        if stmts.is_empty() {
            Ok(tail)
        } else {
            Ok(Expr::Block(stmts, Box::new(tail)))
        }
    } else {
        // Block with only let statements (no value)
        // Re-push the last statement as the tail
        if let Some(last) = stmts.pop() {
            match last {
                Stmt::Let(n, t, v) => {
                    // A let with nothing after is a value-less block
                    // Return unit
                    Ok(Expr::Block(
                        vec![Stmt::Let(n, t, v)],
                        Box::new(Expr::Int(0, ast::Type::Unit)),
                    ))
                }
                Stmt::Expr(e) => {
                    if stmts.is_empty() { Ok(e) }
                    else { Ok(Expr::Block(stmts, Box::new(e))) }
                }
            }
        } else {
            // Empty block
            Ok(Expr::Int(0, ast::Type::Unit))
        }
    }
}

/// Parse function call arguments.
fn parse_call_args(c: &mut Cursor, name: String) -> Result<Expr> {
    let mut args = Vec::new();
    loop {
        c.skip_ws_and_comments();
        if c.peek() == Some(')') { c.advance(); break; }
        if !args.is_empty() { c.expect_char(',')?; }
        args.push(parse_expr(c, 0)?);
    }
    Ok(Expr::Call(name, args))
}

// ---------------------------------------------------------------------------
// Keyword helpers
// ---------------------------------------------------------------------------

fn peek_keyword(c: &mut Cursor) -> String {
    c.skip_ws_and_comments();
    let mut kw = String::new();
    for ch in c.rest().chars() {
        if ch.is_alphanumeric() || ch == '_' { kw.push(ch); }
        else { break; }
    }
    kw
}

fn consume_keyword(c: &mut Cursor) -> String {
    let kw = peek_keyword(c);
    for _ in 0..kw.len() {
        if c.peek().is_some() { c.advance(); }
    }
    kw
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_constant_seven() {
        let source = "fn seven() -> I64 { 7 }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "seven");
    }

    #[test]
    fn parse_add() {
        let source = "fn add(a: I64, b: I64) -> I64 { a + b }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].params.len(), 2);
    }

    #[test]
    fn parse_max() {
        let source = "fn max(a: I64, b: I64) -> I64 { if a > b { a } else { b } }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_fib() {
        let source = "\
fn fib(n: I64) -> I64 {
    if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "fib");
    }

    #[test]
    fn parse_let_binding() {
        let source = "fn example(x: I64) -> I64 { let y = x + 1; y }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_multiple_functions() {
        let source = "fn a() -> I64 { 1 } fn b() -> I64 { 2 }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn parse_empty_body() {
        let source = "fn unit() { }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_with_effect() {
        let source = "fn greet(n: I64) -> I64 ~ Io + Audit { n }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].effects, vec!["Io".to_string(), "Audit".to_string()]);
    }

    #[test]
    fn parse_unary_ops() {
        let source = "fn neg(x: I64) -> I64 { -x }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_not_op() {
        let source = "fn invert(b: Bool) -> Bool { !b }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_call() {
        let source = "fn caller(x: I64) -> I64 { callee(x) } fn callee(a: I64) -> I64 { a }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 2);
    }

    #[test]
    fn parse_comment() {
        let source = "// comment\nfn seven() -> I64 { 7 }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_extern_decl() {
        let source = "extern \"C\" fn puts(s: I64) -> I32;";
        let module = parse_source(source).unwrap();
        assert_eq!(module.extern_functions.len(), 1);
        assert_eq!(module.extern_functions[0].name, "puts");
        assert_eq!(module.extern_functions[0].abi, "C");
    }

    #[test]
    fn parse_complex_expression() {
        let source = "fn compute(a: I64, b: I64, c: I64) -> I64 { a + b * c }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
        // a + b * c should parse as a + (b * c) due to precedence
    }

    #[test]
    fn parse_multi_block() {
        let source = "\
fn example(x: I64) -> I64 {
    let y = x + 1
    let z = y * 2
    z
}";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn parse_effect_handler() {
        let source = "fn run() -> I64 { with Audit = effect Audit { record(amount) { 1 } } { 42 } }";
        let module = parse_source(source).unwrap();
        assert_eq!(module.functions.len(), 1);
    }
}
