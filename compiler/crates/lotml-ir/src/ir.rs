//! The IR: monomorphic functions whose statements work on typed, named locals, so that counting
//! can follow each value's last use (specs/shared-ir/design.md, specs/c-backend/design.md).

use lotml_check::ty::{IntKind, Ty};
use lotml_syntax::span::Span;

/// A local of a function: a parameter, a variable or an intermediate value.
pub type Local = usize;

/// An operation the language defines and a runtime performs, named for what it does, never for
/// the symbol of one backend's runtime (R1.3); `lotml-runtime` maps each to its C function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Builtin {
    AssertCompared,
    Comb,
    DictClear,
    DictContains,
    DictCopy,
    DictFromPairs,
    DictGet,
    DictGetOptional,
    DictGetOr,
    DictItems,
    DictKeys,
    DictPop,
    DictSet,
    DictValues,
    Factorial,
    Gcd,
    HashValue,
    HeapPeek,
    HeapPop,
    HeapPush,
    Heapify,
    Isqrt,
    ListAll,
    ListAny,
    ListClear,
    ListConcat,
    ListContains,
    ListCopy,
    ListCount,
    ListExtend,
    ListExtreme,
    ListIndex,
    ListInsert,
    ListLast,
    ListPop,
    ListPush,
    ListRemove,
    ListRepeat,
    ListReverse,
    ListReversed,
    ListSlice,
    ListSort,
    ListSortByKeys,
    ListSorted,
    ListUnique,
    ListUnpack,
    MathAtan,
    MathAtan2,
    MathCeil,
    MathCos,
    MathExp,
    MathFabs,
    MathFloor,
    MathHypot,
    MathLn,
    MathLog10,
    MathLog2,
    MathPow,
    MathSin,
    MathSqrt,
    MathTan,
    MathTrunc,
    Perm,
    PowMod,
    RangeList,
    RoundF64,
    RoundI64,
    SetAdd,
    SetContains,
    SetCopy,
    SetDifference,
    SetDiscard,
    SetFromList,
    SetIntersection,
    SetIssubset,
    SetList,
    SetPop,
    SetRemove,
    SetUnion,
    StrCapitalize,
    StrChars,
    StrChr,
    StrConcat,
    StrCount,
    StrEndswith,
    StrFind,
    StrFloat,
    StrIndex,
    StrInt,
    StrIsalnum,
    StrIsalpha,
    StrIsdigit,
    StrIslower,
    StrIsspace,
    StrIsupper,
    StrJoin,
    StrLower,
    StrOrd,
    StrPad,
    StrPartitionPart,
    StrRepeat,
    StrReplace,
    StrSlice,
    StrSplit,
    StrSplitOnce,
    StrSplitlines,
    StrStartswith,
    StrStrip,
    StrSwapcase,
    StrTitle,
    StrToFloat,
    StrToInt,
    StrUpper,
    StrZfill,
    SumF64,
    SumI64,
    SumU64,
    TestError,
    WrappingAdd,
    WrappingMul,
    WrappingSub,
}

impl Builtin {
    /// Every built-in operation.
    pub const ALL: &[Builtin] = &[
        Builtin::AssertCompared,
        Builtin::Comb,
        Builtin::DictClear,
        Builtin::DictContains,
        Builtin::DictCopy,
        Builtin::DictFromPairs,
        Builtin::DictGet,
        Builtin::DictGetOptional,
        Builtin::DictGetOr,
        Builtin::DictItems,
        Builtin::DictKeys,
        Builtin::DictPop,
        Builtin::DictSet,
        Builtin::DictValues,
        Builtin::Factorial,
        Builtin::Gcd,
        Builtin::HashValue,
        Builtin::HeapPeek,
        Builtin::HeapPop,
        Builtin::HeapPush,
        Builtin::Heapify,
        Builtin::Isqrt,
        Builtin::ListAll,
        Builtin::ListAny,
        Builtin::ListClear,
        Builtin::ListConcat,
        Builtin::ListContains,
        Builtin::ListCopy,
        Builtin::ListCount,
        Builtin::ListExtend,
        Builtin::ListExtreme,
        Builtin::ListIndex,
        Builtin::ListInsert,
        Builtin::ListLast,
        Builtin::ListPop,
        Builtin::ListPush,
        Builtin::ListRemove,
        Builtin::ListRepeat,
        Builtin::ListReverse,
        Builtin::ListReversed,
        Builtin::ListSlice,
        Builtin::ListSort,
        Builtin::ListSortByKeys,
        Builtin::ListSorted,
        Builtin::ListUnique,
        Builtin::ListUnpack,
        Builtin::MathAtan,
        Builtin::MathAtan2,
        Builtin::MathCeil,
        Builtin::MathCos,
        Builtin::MathExp,
        Builtin::MathFabs,
        Builtin::MathFloor,
        Builtin::MathHypot,
        Builtin::MathLn,
        Builtin::MathLog10,
        Builtin::MathLog2,
        Builtin::MathPow,
        Builtin::MathSin,
        Builtin::MathSqrt,
        Builtin::MathTan,
        Builtin::MathTrunc,
        Builtin::Perm,
        Builtin::PowMod,
        Builtin::RangeList,
        Builtin::RoundF64,
        Builtin::RoundI64,
        Builtin::SetAdd,
        Builtin::SetContains,
        Builtin::SetCopy,
        Builtin::SetDifference,
        Builtin::SetDiscard,
        Builtin::SetFromList,
        Builtin::SetIntersection,
        Builtin::SetIssubset,
        Builtin::SetList,
        Builtin::SetPop,
        Builtin::SetRemove,
        Builtin::SetUnion,
        Builtin::StrCapitalize,
        Builtin::StrChars,
        Builtin::StrChr,
        Builtin::StrConcat,
        Builtin::StrCount,
        Builtin::StrEndswith,
        Builtin::StrFind,
        Builtin::StrFloat,
        Builtin::StrIndex,
        Builtin::StrInt,
        Builtin::StrIsalnum,
        Builtin::StrIsalpha,
        Builtin::StrIsdigit,
        Builtin::StrIslower,
        Builtin::StrIsspace,
        Builtin::StrIsupper,
        Builtin::StrJoin,
        Builtin::StrLower,
        Builtin::StrOrd,
        Builtin::StrPad,
        Builtin::StrPartitionPart,
        Builtin::StrRepeat,
        Builtin::StrReplace,
        Builtin::StrSlice,
        Builtin::StrSplit,
        Builtin::StrSplitOnce,
        Builtin::StrSplitlines,
        Builtin::StrStartswith,
        Builtin::StrStrip,
        Builtin::StrSwapcase,
        Builtin::StrTitle,
        Builtin::StrToFloat,
        Builtin::StrToInt,
        Builtin::StrUpper,
        Builtin::StrZfill,
        Builtin::SumF64,
        Builtin::SumI64,
        Builtin::SumU64,
        Builtin::TestError,
        Builtin::WrappingAdd,
        Builtin::WrappingMul,
        Builtin::WrappingSub,
    ];
}

#[derive(Clone, Debug)]
pub struct LocalInfo {
    pub ty: Ty,
    /// The lotml name, when the local is one; the C name is made from it.
    pub name: Option<String>,
    /// An `inout` parameter: a pointer to the caller's slot, every use of it through the pointer.
    /// The callee holds no count of its own: what it reads it copies, what it assigns replaces
    /// the caller's value.
    pub by_ref: bool,
}

#[derive(Clone, Debug)]
pub struct Function {
    /// The C name.
    pub name: String,
    /// The name a panic reports: `in check`.
    pub source_name: String,
    /// The type parameters the function is generic over, its types naming them as `Ty::Param`:
    /// empty once `mono` has made the program monomorphic (specs/python-on-ir R4).
    pub type_params: Vec<String>,
    pub params: Vec<Local>,
    pub ret: Ty,
    pub locals: Vec<LocalInfo>,
    pub body: Block,
    /// The span of the declaration.
    pub span: Span,
}

pub type Block = Vec<Stmt>;

#[derive(Clone, Debug)]
pub struct Stmt {
    /// The span of the source statement this one was lowered from (R1.2).
    pub span: Span,
    /// The span of the expression the statement computes, the statement's own where it computes
    /// none: what a Python traceback underlines (specs/python-on-ir R2.1).
    pub at: Span,
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
    /// An element of a list known to be its slot's own: no check for a copy first.
    OwnedIndex(Operand),
    /// A field of a record.
    Field(usize),
    /// The value of a key of a dict, which must be there.
    Key(Operand),
    /// The value of a key of a dict, set to the default first when missing: `setdefault`. The
    /// key and the default are taken over.
    SetDefault(Operand, Operand),
}

#[derive(Clone, Debug)]
pub enum StmtKind {
    /// `local = value`.
    Let(Local, Expr),
    /// `place = value`, the value it held dropped.
    Store(Place, Operand),
    /// A built-in operation that changes the value at `place`, given a pointer to its slot; what
    /// it returns is set in `result`, when there is one.
    Mutate {
        op: Builtin,
        place: Place,
        args: Vec<Arg>,
        at: bool,
        result: Option<Local>,
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
    /// `Dec`, but when the value was unique its fields are dropped and its memory kept as the
    /// reuse token `token`, for a constructor further on.
    DropReuse {
        local: Local,
        token: usize,
    },
    /// Count a call of this function toward the recursion limit, after stopping the program with
    /// a `RecursionError` when the count is already at it (specs/recursion-depth R1.2).
    Enter,
    /// Take back the count `Enter` added, on the way out of the function (R1.3).
    Leave,
}

#[derive(Clone, Debug)]
pub enum Panic {
    Todo,
    Assert(String),
    /// A ValueError with this message.
    Value(String),
}

#[derive(Clone)]
pub enum Operand {
    Local(Local),
    Const(Const),
}

/// `%3` for a local, a constant as `Const` shows it: the form the IR's text writes (R1.5).
impl std::fmt::Debug for Operand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operand::Local(l) => write!(f, "%{l}"),
            Operand::Const(c) => write!(f, "{c:?}"),
        }
    }
}

/// A constant as the IR's text writes it: `7_i64`, `1.5`, `true`, `()`, `"text"`, `null`, `'^'`.
impl std::fmt::Debug for Const {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Const::Int(v, kind) => write!(f, "{v}_{}", format!("{kind:?}").to_lowercase()),
            Const::Float(v) => write!(f, "{v:?}"),
            Const::Bool(b) => write!(f, "{b}"),
            Const::Unit => write!(f, "()"),
            Const::Str(s) => write!(f, "{s:?}"),
            Const::Bytes(b) => write!(f, "b{:?}", String::from_utf8_lossy(b)),
            Const::Null => write!(f, "null"),
            Const::Char(c) => write!(f, "{c:?}"),
        }
    }
}

#[derive(Clone)]
pub enum Const {
    Int(i128, IntKind),
    Float(f64),
    Bool(bool),
    Unit,
    /// A string literal's text.
    Str(String),
    /// A bytes literal's bytes.
    Bytes(Vec<u8>),
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
    /// A pointer to a local the function sets, whatever it returns: an output.
    Out(Local, Ty),
    /// The descriptor of a type.
    Desc(Ty),
    /// The offset of the second field of a pair type: where a dict's value goes in an item.
    Offset(Ty),
    /// A pointer to the slot of a place: an `inout` argument.
    Slot(Place),
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
    /// A call of the module's function `name` passing `inout` arguments as slots.
    CallSlots(String, Vec<Arg>),
    Print {
        args: Vec<(Operand, Ty)>,
        sep: Option<Operand>,
        end: Option<Operand>,
    },
    /// A built-in operation, given the place in the source when `at` is set.
    Rt {
        op: Builtin,
        args: Vec<Arg>,
        at: bool,
    },
    /// A built-in operation returning a pointer to a value of `ty`, read.
    RtValue {
        op: Builtin,
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
    /// An element of a list, read; its index checked against the length unless the reader
    /// already keeps it below.
    ListGet {
        list: Operand,
        index: Operand,
        elem: Ty,
        checked: bool,
    },
    TupleNew {
        ty: Ty,
        items: Vec<Operand>,
    },
    TupleGet {
        tuple: Operand,
        index: usize,
    },
    /// A new record (`variant` None) or variant of a sum type `ty`, holding `fields`; built in
    /// the memory of the reuse token `reuse` when it holds some large enough.
    Construct {
        ty: Ty,
        variant: Option<usize>,
        fields: Vec<Operand>,
        reuse: Option<usize>,
    },
    /// A variant of `ty` without fields: a static cell.
    UnitVariant {
        ty: Ty,
        variant: usize,
    },
    /// A field of a record (`variant` None) or of a variant, read.
    Field {
        value: Operand,
        ty: Ty,
        variant: Option<usize>,
        index: usize,
    },
    /// The variant a value of a sum type is.
    Tag(Operand),
    /// An optional `ty` holding `value`, or `None`.
    OptNew {
        ty: Ty,
        value: Option<Operand>,
    },
    OptIsSome(Operand),
    /// The value inside an optional known to hold one, read.
    OptValue(Operand),
    /// A result `ty`: `Ok(value)` when `ok`, else `Err(value)`.
    ResultNew {
        ty: Ty,
        ok: bool,
        value: Operand,
    },
    ResultIsOk(Operand),
    /// The value at a place, read; reaching it may change the containers on the way, as
    /// `setdefault` does.
    ReadPlace(Place),
    /// A new dict holding `items`, keys and values taken over.
    DictNew {
        key: Ty,
        value: Ty,
        items: Vec<(Operand, Operand)>,
    },
    /// A new set holding `items`, taken over.
    SetNew {
        elem: Ty,
        items: Vec<Operand>,
    },
    /// A closure of the lambda `lambda`, of type `ty`, holding copies of `captures`.
    Closure {
        lambda: usize,
        ty: Ty,
        captures: Vec<Operand>,
    },
    /// The module's function `name`, its C name, as a value.
    FnRef(String),
    /// A call of the C library function `symbol` declared with `params` and `ret`: the arguments
    /// read, a `str` passed as its bytes.
    CallC {
        symbol: String,
        args: Vec<Operand>,
        params: Vec<Ty>,
        ret: Ty,
    },
    /// A call of the function `function` of the Python module `module`, declared by its interface
    /// with `params` and returning `ret`, a `T ! PyError`: the arguments read and converted.
    CallPython {
        module: String,
        function: String,
        args: Vec<Operand>,
        params: Vec<Ty>,
        ret: Ty,
    },
    /// A method of `object`, a value of a Python class (adr:0034), declared with `params` and
    /// returning `ret`, a `T ! PyError`: `object.method(args)`, the arguments read and converted.
    CallPyMethod {
        object: Operand,
        method: String,
        args: Vec<Operand>,
        params: Vec<Ty>,
        ret: Ty,
    },
    /// The attribute `name` of `object`, a value of a Python class, read: a `ret`, `T ! PyError`.
    PyAttribute {
        object: Operand,
        name: String,
        ret: Ty,
    },
    /// `parallel(tasks)`: each closure of `tasks` run on a thread, its result of type `result`.
    Parallel {
        tasks: Operand,
        result: Ty,
    },
    /// The record or sum value `value` as the `dyn` type `ty`, calling through the table `vtable`.
    ToDyn {
        value: Operand,
        ty: Ty,
        vtable: usize,
    },
    /// A call of the method in slot `slot` of the `dyn` value `receiver`, of type `ty`, whose
    /// other parameters are `params` and result `ret`; the receiver and the arguments taken over.
    CallDyn {
        receiver: Operand,
        ty: Ty,
        slot: usize,
        args: Vec<Operand>,
        params: Vec<Ty>,
        ret: Ty,
    },
    /// A call of a function known once the type arguments are: an instance of a generic
    /// function, or a method of a generic type or of a type parameter; `mono` makes it a `Call`
    /// or a `CallSlots`.
    CallGeneric {
        callee: Callee,
        args: Vec<Arg>,
    },
    /// The instance of the generic function `name` for `type_args`, as a value: a `FnRef` once
    /// `mono` has made it.
    FnRefGeneric {
        name: String,
        type_args: Vec<Ty>,
    },
    /// The value `value`, of type `from`, as the `dyn` type `ty`: a `ToDyn` through `from`'s
    /// table once `mono` has made it.
    ToDynOf {
        value: Operand,
        ty: Ty,
        from: Ty,
    },
    /// The built-in method `method` of the value at `place`, of type `ty`, which the IR has no
    /// operation of its own for: the Python target calls Python's own method, which may change the
    /// value in place; a native target refuses it.
    Method {
        place: Place,
        ty: Ty,
        method: String,
        args: Vec<Operand>,
        keywords: Vec<(String, Operand)>,
    },
    /// A call of the closure `callee`, of type `ty`; the closure and the arguments taken over.
    CallClosure {
        callee: Operand,
        args: Vec<Operand>,
        ty: Ty,
    },
    /// What the lambda `lambda` captured at `index`, read from its closure.
    Capture {
        closure: Operand,
        lambda: usize,
        index: usize,
    },
    /// An optional `ty` holding `value` when `cond` holds, else `None`.
    OptIf {
        ty: Ty,
        cond: Operand,
        value: Operand,
    },
    /// `o.value()` on a `PyObject`: the Python value `value` checked against `ty` by the boundary
    /// and copied, a result `ty ! PyError` (specs/python-object R2.3). The Python target's only.
    PyValue {
        value: Operand,
        ty: Ty,
    },
    /// The value of an `Ok`, read.
    ResultValue(Operand),
    /// The error of an `Err`, read.
    ResultError(Operand),
}

/// What a `CallGeneric` calls.
#[derive(Clone, Debug, PartialEq)]
pub enum Callee {
    /// The generic function `name`, its LotML name, for `type_args`.
    Function { name: String, type_args: Vec<Ty> },
    /// The method `method` of the type `owner` — a record or sum type, or a type parameter whose
    /// bound declares it — with the method's own type arguments `own`.
    Method { owner: Ty, method: String, own: Vec<Ty> },
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
            Arg::Out(..) | Arg::Desc(_) | Arg::Offset(_) | Arg::Slot(_) => None,
        }
    }
}

impl Expr {
    /// Every operand the expression reads, in order.
    pub fn operands(&self, f: &mut impl FnMut(&Operand)) {
        match self {
            Expr::Use(a) | Expr::Unary(_, a, _) | Expr::Convert(a, ..) | Expr::ToStr(a, _) | Expr::Len(a, _) => f(a),
            Expr::TupleGet { tuple, .. }
            | Expr::Field { value: tuple, .. }
            | Expr::Tag(tuple)
            | Expr::OptIsSome(tuple)
            | Expr::OptValue(tuple)
            | Expr::ResultNew { value: tuple, .. }
            | Expr::ResultIsOk(tuple)
            | Expr::ResultValue(tuple)
            | Expr::ResultError(tuple)
            | Expr::PyValue { value: tuple, .. } => f(tuple),
            Expr::OptNew { value, .. } => value.iter().for_each(f),
            Expr::Closure { captures, .. } => captures.iter().for_each(f),
            Expr::FnRef(_) => {}
            Expr::Parallel { tasks, .. } => f(tasks),
            Expr::CallC { args, .. } | Expr::CallPython { args, .. } => args.iter().for_each(f),
            Expr::CallPyMethod { object, args, .. } => {
                f(object);
                args.iter().for_each(f);
            }
            Expr::PyAttribute { object, .. } => f(object),
            Expr::ToDyn { value, .. } | Expr::ToDynOf { value, .. } => f(value),
            Expr::Method { place, args, keywords, .. } => {
                f(&Operand::Local(place.local));
                place_operands(place, f);
                args.iter().for_each(&mut *f);
                keywords.iter().for_each(|(_, o)| f(o));
            }
            Expr::FnRefGeneric { .. } => {}
            Expr::CallDyn { receiver, args, .. } => {
                f(receiver);
                args.iter().for_each(f);
            }
            Expr::CallClosure { callee, args, .. } => {
                f(callee);
                args.iter().for_each(f);
            }
            Expr::Capture { closure, .. } => f(closure),
            Expr::ReadPlace(place) => {
                f(&Operand::Local(place.local));
                place_operands(place, f);
            }
            Expr::DictNew { items, .. } => {
                for (k, v) in items {
                    f(k);
                    f(v);
                }
            }
            Expr::SetNew { items, .. } => items.iter().for_each(f),
            Expr::OptIf { cond, value, .. } => {
                f(cond);
                f(value);
            }
            Expr::UnitVariant { .. } => {}
            Expr::Binary(_, a, b, _) | Expr::Compare(_, a, b, _) | Expr::MinMax { a, b, .. } => {
                f(a);
                f(b);
            }
            Expr::ListGet { list, index, .. } => {
                f(list);
                f(index);
            }
            Expr::Call(_, args)
            | Expr::ListNew { items: args, .. }
            | Expr::TupleNew { items: args, .. }
            | Expr::Construct { fields: args, .. } => {
                args.iter().for_each(f);
            }
            Expr::Rt { args, .. } | Expr::RtValue { args, .. } => args.iter().filter_map(Arg::operand).for_each(f),
            Expr::CallSlots(_, args) | Expr::CallGeneric { args, .. } => {
                for a in args {
                    match a {
                        Arg::Slot(place) => {
                            f(&Operand::Local(place.local));
                            place_operands(place, f);
                        }
                        other => other.operand().into_iter().for_each(&mut *f),
                    }
                }
            }
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

pub fn place_operands(place: &Place, f: &mut impl FnMut(&Operand)) {
    for proj in &place.proj {
        match proj {
            Proj::Index(i) | Proj::OwnedIndex(i) | Proj::Key(i) => f(i),
            Proj::SetDefault(k, d) => {
                f(k);
                f(d);
            }
            Proj::Field(_) => {}
        }
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
            | StmtKind::Dec(_)
            | StmtKind::DropReuse { .. }
            | StmtKind::Enter
            | StmtKind::Leave => {}
        }
    }
}

/// Every type named by an expression of a block, nested blocks included: what the C needs a
/// definition and a descriptor for.
/// Every expression a block evaluates, nested blocks included.
pub fn block_exprs(block: &Block, f: &mut impl FnMut(&Expr)) {
    for stmt in block {
        match &stmt.kind {
            StmtKind::Let(_, e) | StmtKind::Do(e) => f(e),
            StmtKind::If(_, then, otherwise) => {
                block_exprs(then, f);
                block_exprs(otherwise, f);
            }
            StmtKind::Loop(body) => block_exprs(body, f),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                block_exprs(body, f);
                block_exprs(exit, f);
            }
            _ => {}
        }
    }
}

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
            Arg::Address(_, ty) | Arg::Out(_, ty) | Arg::Desc(ty) | Arg::Offset(ty) => f(ty),
            Arg::Value(_) | Arg::Slot(_) => {}
        }
    }
}

fn expr_types(e: &Expr, f: &mut impl FnMut(&Ty)) {
    match e {
        Expr::ListNew { elem, .. } | Expr::ListGet { elem, .. } => f(&Ty::List(Box::new(elem.clone()))),
        Expr::SetNew { elem, .. } => f(&Ty::Set(Box::new(elem.clone()))),
        Expr::DictNew { key, value, .. } => f(&Ty::Dict(Box::new(key.clone()), Box::new(value.clone()))),
        Expr::TupleNew { ty, .. }
        | Expr::ToStr(_, ty)
        | Expr::Contains { ty, .. }
        | Expr::Construct { ty, .. }
        | Expr::UnitVariant { ty, .. }
        | Expr::Field { ty, .. }
        | Expr::OptNew { ty, .. }
        | Expr::OptIf { ty, .. }
        | Expr::ResultNew { ty, .. }
        | Expr::Len(_, ty)
        | Expr::Closure { ty, .. }
        | Expr::CallClosure { ty, .. }
        | Expr::ToDyn { ty, .. }
        | Expr::ToDynOf { ty, .. }
        | Expr::Method { ty, .. }
        | Expr::Compare(_, _, _, ty) => f(ty),
        Expr::CallDyn { ty, params, ret, .. } => {
            f(ty);
            params.iter().for_each(&mut *f);
            f(ret);
        }
        Expr::Parallel { result, .. } => f(&Ty::List(Box::new(result.clone()))),
        Expr::CallC { params, ret, .. }
        | Expr::CallPython { params, ret, .. }
        | Expr::CallPyMethod { params, ret, .. } => {
            params.iter().for_each(&mut *f);
            f(ret);
        }
        Expr::PyAttribute { ret, .. } => f(ret),
        Expr::Rt { args, .. } | Expr::CallSlots(_, args) | Expr::CallGeneric { args, .. } => arg_types(args, f),
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
        Ty::Str | Ty::List(_) | Ty::Set(_) | Ty::Dict(..) | Ty::Heap(_) | Ty::Adt(..) | Ty::Func(..) | Ty::Dyn(_) => {
            true
        }
        Ty::Tuple(items) => items.iter().any(counted),
        Ty::Optional(t) => counted(t),
        Ty::Result(t, e) => counted(t) || counted(e),
        _ => false,
    }
}

/// The locals a runtime call of `args` sets through its outputs.
pub fn outs(args: &[Arg]) -> impl Iterator<Item = Local> + '_ {
    args.iter().filter_map(|a| if let Arg::Out(l, _) = a { Some(*l) } else { None })
}

impl Expr {
    /// The locals the expression sets through outputs, besides the local it is stored in.
    pub fn outs(&self) -> Vec<Local> {
        match self {
            Expr::Rt { args, .. } | Expr::RtValue { args, .. } => outs(args).collect(),
            _ => Vec::new(),
        }
    }
}

impl Function {
    /// Every type the function names, its locals' and its result's among them, given to `f` to
    /// change: how `mono` writes an instance of a generic function.
    pub fn types_mut(&mut self, f: &mut impl FnMut(&mut Ty)) {
        self.locals.iter_mut().for_each(|l| f(&mut l.ty));
        f(&mut self.ret);
        block_types_mut(&mut self.body, f);
    }
}

fn block_types_mut(block: &mut Block, f: &mut impl FnMut(&mut Ty)) {
    for stmt in block {
        match &mut stmt.kind {
            StmtKind::Let(_, e) | StmtKind::Do(e) => expr_types_mut(e, f),
            StmtKind::Mutate { args, .. } => args_types_mut(args, f),
            StmtKind::If(_, then, otherwise) => {
                block_types_mut(then, f);
                block_types_mut(otherwise, f);
            }
            StmtKind::Loop(body) => block_types_mut(body, f),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                block_types_mut(body, f);
                block_types_mut(exit, f);
            }
            StmtKind::Store(..)
            | StmtKind::Break
            | StmtKind::Continue
            | StmtKind::Return(_)
            | StmtKind::Panic(_)
            | StmtKind::Inc(_)
            | StmtKind::Dec(_)
            | StmtKind::DropReuse { .. }
            | StmtKind::Enter
            | StmtKind::Leave => {}
        }
    }
}

fn args_types_mut(args: &mut [Arg], f: &mut impl FnMut(&mut Ty)) {
    for a in args {
        match a {
            Arg::Address(_, ty) | Arg::Out(_, ty) | Arg::Desc(ty) | Arg::Offset(ty) => f(ty),
            Arg::Value(_) | Arg::Slot(_) => {}
        }
    }
}

fn format_types_mut(parts: &mut [FormatPart], f: &mut impl FnMut(&mut Ty)) {
    for part in parts {
        if let FormatPart::Value { ty, spec, .. } = part {
            f(ty);
            format_types_mut(spec, f);
        }
    }
}

fn expr_types_mut(e: &mut Expr, f: &mut impl FnMut(&mut Ty)) {
    match e {
        Expr::Binary(_, _, _, ty)
        | Expr::Unary(_, _, ty)
        | Expr::Compare(_, _, _, ty)
        | Expr::MinMax { ty, .. }
        | Expr::Contains { ty, .. }
        | Expr::ToStr(_, ty)
        | Expr::Len(_, ty)
        | Expr::ListNew { elem: ty, .. }
        | Expr::ListGet { elem: ty, .. }
        | Expr::TupleNew { ty, .. }
        | Expr::Construct { ty, .. }
        | Expr::UnitVariant { ty, .. }
        | Expr::Field { ty, .. }
        | Expr::OptNew { ty, .. }
        | Expr::ResultNew { ty, .. }
        | Expr::SetNew { elem: ty, .. }
        | Expr::Closure { ty, .. }
        | Expr::Parallel { result: ty, .. }
        | Expr::ToDyn { ty, .. }
        | Expr::Method { ty, .. }
        | Expr::CallClosure { ty, .. }
        | Expr::PyValue { ty, .. }
        | Expr::OptIf { ty, .. } => f(ty),
        Expr::Convert(_, from, to) => {
            f(from);
            f(to);
        }
        Expr::DictNew { key, value, .. } => {
            f(key);
            f(value);
        }
        Expr::Print { args, .. } => args.iter_mut().for_each(|(_, ty)| f(ty)),
        Expr::Rt { args, .. } | Expr::CallSlots(_, args) => args_types_mut(args, f),
        Expr::RtValue { args, ty, .. } => {
            args_types_mut(args, f);
            f(ty);
        }
        Expr::Format(parts) => format_types_mut(parts, f),
        Expr::CallC { params, ret, .. }
        | Expr::CallPython { params, ret, .. }
        | Expr::CallPyMethod { params, ret, .. } => {
            params.iter_mut().for_each(&mut *f);
            f(ret);
        }
        Expr::PyAttribute { ret, .. } => f(ret),
        Expr::CallDyn { ty, params, ret, .. } => {
            f(ty);
            params.iter_mut().for_each(&mut *f);
            f(ret);
        }
        Expr::CallGeneric { callee, args } => {
            match callee {
                Callee::Function { type_args, .. } => type_args.iter_mut().for_each(&mut *f),
                Callee::Method { owner, own, .. } => {
                    f(owner);
                    own.iter_mut().for_each(&mut *f);
                }
            }
            args_types_mut(args, f);
        }
        Expr::FnRefGeneric { type_args, .. } => type_args.iter_mut().for_each(&mut *f),
        Expr::ToDynOf { ty, from, .. } => {
            f(ty);
            f(from);
        }
        Expr::Use(_)
        | Expr::Call(..)
        | Expr::TupleGet { .. }
        | Expr::Tag(_)
        | Expr::OptIsSome(_)
        | Expr::OptValue(_)
        | Expr::ResultIsOk(_)
        | Expr::ResultValue(_)
        | Expr::ResultError(_)
        | Expr::ReadPlace(_)
        | Expr::FnRef(_)
        | Expr::Capture { .. } => {}
    }
}
