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

/// Where a value is stored: a local, or an element reached from one.
#[derive(Clone, Debug)]
pub struct Place {
    pub local: Local,
    pub proj: Vec<Proj>,
}

#[derive(Clone, Debug)]
pub enum Proj {
    /// An element of a list.
    Index(Operand),
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `local = value`.
    Let(Local, Expr),
    /// `place = value`, the value it held dropped.
    Store(Place, Operand),
    /// A runtime function that changes the value at `place`, given a pointer to its slot.
    Mutate {
        name: &'static str,
        place: Place,
        args: Vec<Arg>,
        at: bool,
    },
    /// Evaluate for its effect.
    Do(Expr),
    If(Operand, Block, Block),
    /// Repeat until a `Break`.
    Loop(Block),
    Break,
    Continue,
    Return(Option<Operand>),
    /// Walk `range`: `local` takes each value in turn; the body runs once per value, and `exit`
    /// once the range is spent.
    ForRange {
        var: Local,
        start: Operand,
        stop: Operand,
        step: Operand,
        body: Block,
        exit: Block,
    },
    /// Walk a string: `var` takes each character in turn; `exit` runs once it is spent.
    ForStr {
        var: Local,
        over: Operand,
        body: Block,
        exit: Block,
    },
    /// Stop the program.
    Panic(Panic),
    /// Add a count to the value of a local.
    Inc(Local),
    /// Take a count off the value of a local, freeing it when it was the last.
    Dec(Local),
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

/// An argument of a runtime function.
#[derive(Clone, Debug)]
pub enum Arg {
    Value(Operand),
    /// A pointer to the operand's value, of the given type: how the runtime takes an element.
    Address(Operand, Ty),
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
        args: Vec<Arg>,
        at: bool,
    },
    /// A runtime function returning a pointer to a value of `ty`, read.
    RtValue {
        name: &'static str,
        args: Vec<Arg>,
        at: bool,
        ty: Ty,
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
    /// A new list of `elem` holding `items`.
    ListNew {
        elem: Ty,
        items: Vec<Operand>,
    },
    /// An element of a list, read.
    ListGet {
        list: Operand,
        index: Operand,
        elem: Ty,
    },
    TupleNew {
        ty: Ty,
        items: Vec<Operand>,
    },
    TupleGet {
        tuple: Operand,
        index: usize,
    },
}

#[derive(Clone, Debug)]
pub enum FormatPart {
    Text(String),
    Value { value: Operand, ty: Ty, conversion: Option<char>, spec: Vec<FormatPart> },
}

impl Arg {
    pub fn operand(&self) -> Option<&Operand> {
        match self {
            Arg::Value(o) | Arg::Address(o, _) => Some(o),
        }
    }
}

impl Expr {
    /// Every operand the expression reads, in order.
    pub fn operands(&self, f: &mut impl FnMut(&Operand)) {
        match self {
            Expr::Use(a) | Expr::Unary(_, a, _) | Expr::Convert(a, ..) | Expr::ToStr(a, _) | Expr::Len(a, _) => f(a),
            Expr::TupleGet { tuple, .. } => f(tuple),
            Expr::Binary(_, a, b, _) | Expr::Compare(_, a, b, _) | Expr::MinMax { a, b, .. } => {
                f(a);
                f(b);
            }
            Expr::ListGet { list, index, .. } => {
                f(list);
                f(index);
            }
            Expr::Call(_, args) | Expr::ListNew { items: args, .. } | Expr::TupleNew { items: args, .. } => {
                args.iter().for_each(f);
            }
            Expr::Rt { args, .. } | Expr::RtValue { args, .. } => args.iter().filter_map(Arg::operand).for_each(f),
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

fn place_operands(place: &Place, f: &mut impl FnMut(&Operand)) {
    for proj in &place.proj {
        let Proj::Index(i) = proj;
        f(i);
    }
}

/// Every operand a block reads, nested blocks included.
pub fn block_operands(block: &Block, f: &mut impl FnMut(&Operand)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let(_, e) | StmtKind::Do(e) => e.operands(f),
            StmtKind::Store(place, value) => {
                place_operands(place, f);
                f(value);
            }
            StmtKind::Mutate { place, args, .. } => {
                place_operands(place, f);
                args.iter().filter_map(Arg::operand).for_each(&mut *f);
            }
            StmtKind::If(test, then, otherwise) => {
                f(test);
                block_operands(then, f);
                block_operands(otherwise, f);
            }
            StmtKind::Loop(body) => block_operands(body, f),
            StmtKind::Return(Some(v)) => f(v),
            StmtKind::ForRange { start, stop, step, body, exit, .. } => {
                f(start);
                f(stop);
                f(step);
                block_operands(body, f);
                block_operands(exit, f);
            }
            StmtKind::ForStr { over, body, exit, .. } => {
                f(over);
                block_operands(body, f);
                block_operands(exit, f);
            }
            StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Return(None)
            | StmtKind::Panic(_)
            | StmtKind::Inc(_)
            | StmtKind::Dec(_) => {}
        }
    }
}

/// Every type named by an expression of a block, nested blocks included: what the C needs a
/// definition and a descriptor for.
pub fn block_types(block: &Block, f: &mut impl FnMut(&Ty)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let(_, e) | StmtKind::Do(e) => expr_types(e, f),
            StmtKind::Mutate { args, .. } => arg_types(args, f),
            StmtKind::If(_, then, otherwise) => {
                block_types(then, f);
                block_types(otherwise, f);
            }
            StmtKind::Loop(body) | StmtKind::ForRange { body, .. } | StmtKind::ForStr { body, .. } => {
                block_types(body, f);
            }
            _ => {}
        }
    }
}

fn arg_types(args: &[Arg], f: &mut impl FnMut(&Ty)) {
    for a in args {
        match a {
            Arg::Address(_, ty) => f(ty),
            Arg::Value(_) => {}
        }
    }
}

fn expr_types(e: &Expr, f: &mut impl FnMut(&Ty)) {
    match e {
        Expr::ListNew { elem, .. } | Expr::ListGet { elem, .. } => f(&Ty::List(Box::new(elem.clone()))),
        Expr::TupleNew { ty, .. } | Expr::ToStr(_, ty) | Expr::Contains { ty, .. } => f(ty),
        Expr::Rt { args, .. } => arg_types(args, f),
        Expr::RtValue { args, ty, .. } => {
            arg_types(args, f);
            f(ty);
        }
        Expr::Print { args, .. } => args.iter().for_each(|(_, ty)| f(ty)),
        _ => {}
    }
}

/// Whether a value of `ty` holds a count: a cell, or a struct holding one.
pub fn counted(ty: &Ty) -> bool {
    match ty {
        Ty::Str | Ty::List(_) | Ty::Set(_) | Ty::Dict(..) | Ty::Heap(_) => true,
        Ty::Tuple(items) => items.iter().any(counted),
        _ => false,
    }
}
