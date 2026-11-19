//! The tree `Parser.jjt` builds, with what each node's `init()` derives from its tokens already computed.

use std::sync::Arc;

use java_compat::lang::JValue;

/// A node's first token position (`getLine()` / `getColumn()`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: i32,
    pub column: i32,
}

/// A block of statements (`ASTBlock`, or the template root).
#[derive(Debug, Clone, Default)]
pub struct Block {
    /// The statements, in order.
    pub nodes: Vec<Node>,
    /// `getTemplate()` of the nodes in it: the template the block was parsed into.
    pub template: Option<Arc<crate::vm::TemplateInfo>>,
    /// `getPrefix()`: the whitespace before the block that space gobbling keeps.
    pub prefix: String,
    /// `getPostfix()`: the whitespace after the block that space gobbling keeps.
    pub postfix: String,
    /// `$` / `#` characters `MORE` put in front of the `#end`/`#else` that closes the block: always rendered.
    pub more_postfix: String,
    /// Where the block's first token is (`getLine()` / `getColumn()`).
    pub start: Pos,
}

#[derive(Debug, Clone)]
pub enum Node {
    /// Text, escapes, comments and text blocks: rendered verbatim.
    Text(String),
    Reference(Box<Reference>),
    If(Box<If>),
    Set(Box<Set>),
    Directive(Box<Directive>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
    Normal,
    Formal,
    Quiet,
    Runt,
    /// `getRoot()` returned early (a `$\!` reference): none of the above.
    Unset,
}

#[derive(Debug, Clone)]
pub enum RefPart {
    Identifier(String, Pos),
    Method(String, Vec<Expr>, Pos),
    Index(Expr, Pos),
}

/// `ASTReference`.
#[derive(Debug, Clone)]
pub struct Reference {
    pub pos: Pos,
    /// `ASTReference.init`'s strict-mode exemption: a bare reference tested by an `#if` (directly under `!`, `&&`,
    /// `||` or an expression, with an `#if` above).
    pub if_test: bool,
    pub ref_type: RefType,
    pub root: String,
    pub null_string: String,
    pub escaped: bool,
    pub esc_prefix: String,
    pub more_prefix: String,
    pub computable: bool,
    pub parts: Vec<RefPart>,
    pub alternate: Option<Box<Expr>>,
    /// `literal()`: the token text, for messages and `+` with a null side.
    pub literal: String,
    /// The first token's image after `getRoot()` (macro argument names, `#define` keys).
    pub first_image: String,
}

#[derive(Debug, Clone)]
pub struct If {
    pub condition: Expr,
    pub then: Block,
    pub else_ifs: Vec<(Expr, Block)>,
    pub otherwise: Option<Block>,
    pub prefix: String,
    pub postfix: String,
    pub more_prefix: String,
}

#[derive(Debug, Clone)]
pub struct Set {
    pub left: Reference,
    pub right: Expr,
    /// `ASTExpression.lastImage` of the right-hand side, for `invalidSetMethod`.
    pub right_last_image: String,
    /// Where the `#set` starts (`uberInfo`).
    pub pos: Pos,
    pub prefix: String,
    pub postfix: String,
    pub more_prefix: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgType {
    Reference,
    Word,
    StringLiteral,
    IntegerLiteral,
    IntegerRange,
    FloatingPointLiteral,
    Map,
    ObjectArray,
    True,
    False,
    DirectiveAssign,
}

/// `ASTDirective`: a built-in directive, a macro call or an unknown `#word`.
#[derive(Debug, Clone)]
pub struct Directive {
    pub name: String,
    pub args: Vec<Expr>,
    pub arg_types: Vec<ArgType>,
    /// Each argument's first token position.
    pub arg_pos: Vec<Pos>,
    /// `RuntimeMacro.init`: the directive ends with `)` or `#end` (strict mode applies to the call).
    pub macro_call_closed: bool,
    pub block: Option<Arc<Block>>,
    /// `#foreach … #else … #end`.
    pub else_block: Option<Arc<Block>>,
    pub prefix: String,
    pub postfix: String,
    pub more_prefix: String,
    /// `RuntimeMacro.getLiteral()`: what an undefined macro call renders.
    pub macro_literal: String,
    /// A unique id: the directive instance a `StopCommand` targets.
    pub id: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone)]
pub enum Expr {
    /// `ASTStringLiteral`: `interpolated` holds the parsed template when it is interpolated.
    Str { image: String, literal: String, interpolated: Option<Block> },
    Integer(JValue, String),
    Float(JValue, String),
    True,
    False,
    Reference(Box<Reference>),
    Range(Box<Expr>, Box<Expr>),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    /// A directive's bare word argument (`in`, a macro name).
    Word(String),
    /// Where the node starts: the token after the operator.
    Binary(BinOp, Box<Expr>, Box<Expr>, Pos),
    Not(Box<Expr>),
    Negate(Box<Expr>),
    /// `ASTExpression` around an alternate value or a parenthesised expression; transparent.
    Group(Box<Expr>),
}

impl Expr {
    /// `literal()`.
    pub fn literal(&self) -> String {
        match self {
            Expr::Str { literal, .. } => literal.clone(),
            Expr::Integer(_, l) | Expr::Float(_, l) => l.clone(),
            Expr::True => "true".into(),
            Expr::False => "false".into(),
            Expr::Reference(r) => r.literal.clone(),
            Expr::Word(w) => w.clone(),
            // `ASTBinaryOperator.literal()` prints the left side and the operator only.
            Expr::Binary(op, l, _, _) => format!("{} {} ", l.literal(), op_literal(*op)),
            Expr::Not(e) => format!("!{}", e.literal()),
            Expr::Negate(e) => format!("-{}", e.literal()),
            Expr::Group(e) => e.literal(),
            Expr::Range(..) | Expr::List(_) | Expr::Map(_) => String::new(),
        }
    }
}

fn op_literal(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "||",
        BinOp::And => "&&",
        BinOp::Eq => "==",
        BinOp::Ne => "==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
    }
}
