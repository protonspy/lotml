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
    /// No value, for an optional argument of the runtime left out: C's `NULL`.
    Null,
    /// A single ASCII character, for an option of the runtime: `'^'`.
    Char(char),
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
    /// A function of the runtime, given the place in the source when `at` is set.
    Rt {
        name: &'static str,
        args: Vec<Operand>,
        at: bool,
    },
    /// Whether `item` is in `container`, a value of `ty`.
    Contains {
        container: Operand,
        item: Operand,
        ty: Ty,
    },
    /// An f-string.
    Format(Vec<FormatPart>),
    /// `str(value)`.
    ToStr(Operand, Ty),
    /// `len(value)`.
    Len(Operand, Ty),
}

#[derive(Clone, Debug)]
pub enum FormatPart {
    Text(String),
    Value { value: Operand, ty: Ty, conversion: Option<char>, spec: Vec<FormatPart> },
}

impl Expr {
    /// Every operand the expression reads, in order.
    pub fn operands(&self, f: &mut impl FnMut(&Operand)) {
        match self {
            Expr::Use(a) | Expr::Unary(_, a, _) | Expr::Convert(a, ..) | Expr::ToStr(a, _) | Expr::Len(a, _) => f(a),
            Expr::Binary(_, a, b, _) | Expr::Compare(_, a, b, _) | Expr::MinMax { a, b, .. } => {
                f(a);
                f(b);
            }
            Expr::Call(_, args) | Expr::Rt { args, .. } => args.iter().for_each(f),
            Expr::Print { args, sep, end } => {
                args.iter().for_each(|(a, _)| f(a));
                sep.iter().chain(end).for_each(f);
            }
            Expr::Contains { container, item, .. } => {
                f(container);
                f(item);
            }
            Expr::Format(parts) => format_operands(parts, f),
        }
    }
}

fn format_operands(parts: &[FormatPart], f: &mut impl FnMut(&Operand)) {
    for part in parts {
        if let FormatPart::Value { value, spec, .. } = part {
            f(value);
            format_operands(spec, f);
        }
    }
}

/// Every operand a block reads, nested blocks included.
pub fn block_operands(block: &Block, f: &mut impl FnMut(&Operand)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let(_, e) | StmtKind::Do(e) => e.operands(f),
            StmtKind::If(test, then, otherwise) => {
                f(test);
                block_operands(then, f);
                block_operands(otherwise, f);
            }
            StmtKind::Loop(body) => block_operands(body, f),
            StmtKind::Return(Some(v)) => f(v),
            StmtKind::ForRange { start, stop, step, body, .. } => {
                f(start);
                f(stop);
                f(step);
                block_operands(body, f);
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) | StmtKind::Panic(_) => {}
        }
    }
}
