//! The syntax tree. Every node keeps the span of the text it came from.
//!
//! A part the parser could not read is an `Error` node in place, so a file with mistakes
//! still yields every item, statement and expression around them.

use crate::span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Fn(FnDef),
    Record(RecordDef),
    Sum(SumDef),
    Impl(ImplDef),
    Trait(TraitDef),
    Import(Import),
    Test(TestDef),
    /// A Python class, which only an interface declares (adr:0034).
    Class(ClassDef),
    Error(Span),
}

impl Item {
    /// Where the item's own text ends: its last token, before the blank lines and comments that
    /// the span of a closing block takes in.
    pub fn end(&self) -> u32 {
        match self {
            Item::Fn(f) => f.end(),
            Item::Impl(imp) => imp.methods.last().map_or(imp.span.end, FnDef::end),
            Item::Trait(t) => t.methods.last().map_or(t.span.end, FnDef::end),
            Item::Class(c) => {
                c.methods.last().map_or(c.span.end, FnDef::end).max(c.attributes.last().map_or(0, |a| a.span.end))
            }
            Item::Test(t) => t.body.end(),
            other => other.span().end,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Item::Fn(f) => f.span,
            Item::Record(r) => r.span,
            Item::Sum(s) => s.span,
            Item::Impl(i) => i.span,
            Item::Trait(t) => t.span,
            Item::Import(i) => i.span,
            Item::Test(t) => t.span,
            Item::Class(c) => c.span,
            Item::Error(span) => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FnDef {
    pub span: Span,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<Param>,
    pub returns: Option<TypeExpr>,
    /// The error type after `!`: `-> int ! ParseErr`.
    pub error: Option<TypeExpr>,
    /// None for a trait method's signature.
    pub body: Option<Block>,
    /// The string on the body's first line.
    pub doc: Option<String>,
}

impl FnDef {
    /// Where the function's own text ends: its last token.
    pub fn end(&self) -> u32 {
        self.body.as_ref().map_or(self.span.end, Block::end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParam {
    pub name: Ident,
    pub bound: Option<Ident>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convention {
    Default,
    Inout,
    Sink,
    /// The pilot's `var self` and `var x`: a local, mutable copy.
    Var,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Param {
    pub span: Span,
    pub convention: Convention,
    pub name: Ident,
    pub ty: Option<TypeExpr>,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordDef {
    pub span: Span,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<Field>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub span: Span,
    /// None for a positional field of a variant: `Num(int)`.
    pub name: Option<Ident>,
    pub ty: TypeExpr,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SumDef {
    pub span: Span,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<Variant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub span: Span,
    pub name: Ident,
    /// None for a variant written without parentheses: `Empty`.
    pub fields: Option<Vec<Field>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImplDef {
    pub span: Span,
    pub trait_name: Option<TypeExpr>,
    pub target: TypeExpr,
    pub methods: Vec<FnDef>,
}

/// A Python class an interface declares (adr:0034): the bases it names, its attributes, and the
/// signatures of its constructor, methods and static methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDef {
    pub span: Span,
    pub name: Ident,
    /// Its type parameters, `class Pattern[AnyStr]:` (adr:0036).
    pub type_params: Vec<TypeParam>,
    pub bases: Vec<Ident>,
    pub attributes: Vec<Field>,
    pub methods: Vec<FnDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitDef {
    pub span: Span,
    pub name: Ident,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<FnDef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub span: Span,
    pub module: Vec<Ident>,
    /// The names of `from m import a, b`; empty for `import m`.
    pub names: Vec<Ident>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestDef {
    pub span: Span,
    pub name: String,
    pub name_span: Span,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

impl Block {
    /// Where the block's last statement ends.
    pub fn end(&self) -> u32 {
        self.stmts.last().map_or(self.span.start, Stmt::end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

impl Stmt {
    /// Where the statement's own text ends: its last token, not the end of the block that
    /// closes a compound statement.
    pub fn end(&self) -> u32 {
        match &self.kind {
            StmtKind::If { branches, orelse } => match orelse {
                Some(body) => body.end(),
                None => branches.last().map_or(self.span.end, |(_, b)| b.end()),
            },
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => body.end(),
            StmtKind::Match { arms, .. } => arms.last().map_or(self.span.end, |a| a.body.end()),
            _ => self.span.end,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StmtKind {
    Expr(Expr),
    /// `var x: T = value`.
    Var {
        name: Ident,
        ty: Option<TypeExpr>,
        value: Expr,
    },
    /// `target = value`, where the target is a name, a field, an element or a tuple of them.
    Assign {
        target: Expr,
        value: Expr,
    },
    /// `x: T = value`.
    Annotated {
        target: Ident,
        ty: TypeExpr,
        value: Expr,
    },
    AugAssign {
        target: Expr,
        op: BinOp,
        value: Expr,
    },
    Return(Option<Expr>),
    Assert {
        test: Expr,
        message: Option<Expr>,
    },
    Pass,
    Break,
    Continue,
    If {
        branches: Vec<(Expr, Block)>,
        orelse: Option<Block>,
    },
    While {
        test: Expr,
        body: Block,
    },
    For {
        target: Target,
        iter: Expr,
        body: Block,
    },
    Match {
        subject: Expr,
        arms: Vec<Arm>,
    },
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Name(Ident),
    Tuple(Vec<Target>, Span),
}

impl Target {
    pub fn span(&self) -> Span {
        match self {
            Target::Name(ident) => ident.span,
            Target::Tuple(_, span) => *span,
        }
    }

    pub fn names(&self) -> Vec<&Ident> {
        match self {
            Target::Name(ident) => vec![ident],
            Target::Tuple(items, _) => items.iter().flat_map(Target::names).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm {
    pub span: Span,
    pub pattern: Pattern,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub span: Span,
    pub kind: PatternKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    /// A lowercase name, which binds; or a capitalized one, a variant without fields.
    Name(Ident),
    Variant {
        name: Ident,
        args: Vec<Pattern>,
    },
    Tuple(Vec<Pattern>),
    Literal(Expr),
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    LShift,
    RShift,
    BitOr,
    BitXor,
    BitAnd,
}

impl BinOp {
    pub fn text(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::FloorDiv => "//",
            BinOp::Mod => "%",
            BinOp::Pow => "**",
            BinOp::LShift => "<<",
            BinOp::RShift => ">>",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::BitAnd => "&",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Lt,
    Gt,
    Le,
    Ge,
    Eq,
    NotEq,
    In,
    NotIn,
    Is,
    IsNot,
}

impl CmpOp {
    pub fn text(self) -> &'static str {
        match self {
            CmpOp::Lt => "<",
            CmpOp::Gt => ">",
            CmpOp::Le => "<=",
            CmpOp::Ge => ">=",
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::In => "in",
            CmpOp::NotIn => "not in",
            CmpOp::Is => "is",
            CmpOp::IsNot => "is not",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Pos,
    Invert,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoolOp {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrPart {
    Text(String),
    /// `{expr!r:spec}` in an f-string.
    Expr {
        expr: Box<Expr>,
        conversion: Option<char>,
        spec: Vec<StrPart>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrLit {
    /// The literal as written, prefix and quotes included.
    pub raw: String,
    pub bytes: bool,
    /// The value: text for a plain string, text and expressions for an f-string.
    pub parts: Vec<StrPart>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comprehension {
    pub target: Target,
    pub iter: Expr,
    pub conditions: Vec<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Arg {
    Positional(Expr),
    Keyword(Ident, Expr),
    /// `&place`: an argument for an `inout` parameter.
    Inout(Expr, Span),
}

impl Arg {
    pub fn expr(&self) -> &Expr {
        match self {
            Arg::Positional(e) | Arg::Keyword(_, e) | Arg::Inout(e, _) => e,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Name(String),
    Int(String),
    Float(String),
    Str(Vec<StrLit>),
    Bool(bool),
    None,
    Unit,
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Set(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    ListComp {
        element: Box<Expr>,
        loops: Vec<Comprehension>,
    },
    SetComp {
        element: Box<Expr>,
        loops: Vec<Comprehension>,
    },
    DictComp {
        key: Box<Expr>,
        value: Box<Expr>,
        loops: Vec<Comprehension>,
    },
    Generator {
        element: Box<Expr>,
        loops: Vec<Comprehension>,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Compare {
        first: Box<Expr>,
        rest: Vec<(CmpOp, Expr)>,
    },
    Logical {
        op: BoolOp,
        operands: Vec<Expr>,
    },
    Not(Box<Expr>),
    /// `x ?? default`.
    Coalesce {
        value: Box<Expr>,
        default: Box<Expr>,
    },
    IfExp {
        test: Box<Expr>,
        then: Box<Expr>,
        orelse: Box<Expr>,
    },
    Lambda {
        params: Vec<Ident>,
        body: Box<Expr>,
    },
    Call {
        func: Box<Expr>,
        args: Vec<Arg>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        object: Box<Expr>,
        lower: Option<Box<Expr>>,
        upper: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
    Attr {
        object: Box<Expr>,
        name: Ident,
    },
    /// `expr?`.
    Try(Box<Expr>),
    /// `fail error`.
    Fail(Box<Expr>),
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeExpr {
    pub span: Span,
    pub kind: TypeKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Named { name: Ident, args: Vec<TypeExpr> },
    List(Box<TypeExpr>),
    Set(Box<TypeExpr>),
    Dict(Box<TypeExpr>, Box<TypeExpr>),
    Tuple(Vec<TypeExpr>),
    Optional(Box<TypeExpr>),
    Dyn(Ident),
    Unit,
    Error,
}
