//! The typed intermediate form between the checked program and C: monomorphic functions whose
//! statements work on named locals, so that counting can follow each value's last use
//! (specs/c-backend/design.md).

use lotml_check::ty::{IntKind, Ty};

/// A local of a function: a parameter, a variable or an intermediate value.
pub type Local = usize;

#[derive(Clone, Debug)]
pub struct LocalInfo {
    pub ty: Ty,
    /// The lotml name, when the local is one; the C name is made from it.
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Function {
    /// The C name.
    pub name: String,
    /// The name a panic reports: `in check`.
    pub source_name: String,
    pub params: Vec<Local>,
    pub ret: Ty,
    pub locals: Vec<LocalInfo>,
    pub body: Block,
    /// The line of the declaration.
    pub line: u32,
}

pub type Block = Vec<Stmt>;

#[derive(Clone, Debug)]
pub struct Stmt {
    /// The `.lotml` line the statement came from.
    pub line: u32,
    pub kind: StmtKind,
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `local = value`.
    Let(Local, Expr),
    /// Evaluate for its effect.
    Do(Expr),
    If(Operand, Block, Block),
    /// Repeat until a `Break`.
    Loop(Block),
    Break,
    Continue,
    Return(Option<Operand>),
    /// Walk `range`: `local` takes each value in turn; the body runs once per value.
    ForRange {
        var: Local,
        start: Operand,
        stop: Operand,
        step: Operand,
        body: Block,
    },
    /// Stop the program.
    Panic(Panic),
}

#[derive(Clone, Debug)]
pub enum Panic {
    Todo,
    Assert(String),
}

#[derive(Clone, Debug)]
pub enum Operand {
    Local(Local),
    Const(Const),
}

#[derive(Clone, Debug)]
pub enum Const {
    Int(i128, IntKind),
    Float(f64),
    Bool(bool),
    Unit,
    /// A string literal's text.
    Str(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    TrueDiv,
    FloorDiv,
    Mod,
    Pow,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Invert,
    Not,
    Abs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Clone, Debug)]
pub enum Expr {
    Use(Operand),
    /// An arithmetic operation on two values of `ty`, checked as its type requires.
    Binary(BinOp, Operand, Operand, Ty),
    Unary(UnOp, Operand, Ty),
    Compare(CmpOp, Operand, Operand, Ty),
    /// A number converted from the operand's type to `to`, checked against its range.
    Convert(Operand, Ty, Ty),
    MinMax {
        max: bool,
        a: Operand,
        b: Operand,
        ty: Ty,
    },
    Call(String, Vec<Operand>),
    Print {
        args: Vec<(Operand, Ty)>,
        sep: Option<Operand>,
        end: Option<Operand>,
    },
}
