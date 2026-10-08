//! Types, inference variables, and unification.

use std::cell::Cell;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntKind {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatKind {
    F32,
    F64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Str,
    Bytes,
    Unit,
    List(Box<Ty>),
    Set(Box<Ty>),
    Dict(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Optional(Box<Ty>),
    /// `T ! E`.
    Result(Box<Ty>, Box<Ty>),
    /// A record or a sum type, by name, with its type arguments.
    Adt(String, Vec<Ty>),
    Heap(Box<Ty>),
    /// A type parameter in scope: `T` inside `fn first[T]`.
    Param(String),
    /// An inference variable.
    Var(u32),
    Func(Vec<Ty>, Box<Ty>),
    Dyn(String),
    Module(String),
    /// A type used as a value: `Counter` in `Counter.new()`.
    TypeName(String),
    /// A Python value no stub types: opaque, left only through `value()`
    /// (adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value).
    PyObject,
    /// What `todo()`, `fail` and `return` produce: no value, so it fits anywhere.
    Never,
    /// A type already reported wrong: fits anywhere, so one mistake is reported once.
    Error,
}

pub const INT: Ty = Ty::Int(IntKind::I64);
pub const F64: Ty = Ty::Float(FloatKind::F64);

impl Ty {
    pub fn list(item: Ty) -> Ty {
        Ty::List(Box::new(item))
    }

    pub fn optional(inner: Ty) -> Ty {
        match inner {
            Ty::Optional(_) | Ty::Error => inner,
            other => Ty::Optional(Box::new(other)),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Float(_))
    }

    /// How many nodes the type has: at most [`TYPE_LIMIT`] for one [`Infer::resolve`] gave.
    pub fn size(&self) -> usize {
        1 + match self {
            Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => t.size(),
            Ty::Dict(a, b) | Ty::Result(a, b) => a.size() + b.size(),
            Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().map(Ty::size).sum(),
            Ty::Func(params, ret) => params.iter().map(Ty::size).sum::<usize>() + ret.size(),
            _ => 0,
        }
    }

    /// Whether this is or holds a `PyObject`: what a native program and a function Python calls
    /// cannot have (specs/python-object R3).
    pub fn holds_py_object(&self) -> bool {
        match self {
            Ty::PyObject => true,
            Ty::List(t) | Ty::Set(t) | Ty::Optional(t) | Ty::Heap(t) => t.holds_py_object(),
            Ty::Dict(a, b) | Ty::Result(a, b) => a.holds_py_object() || b.holds_py_object(),
            Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().any(Ty::holds_py_object),
            Ty::Func(params, ret) => params.iter().any(Ty::holds_py_object) || ret.holds_py_object(),
            _ => false,
        }
    }

    pub fn is_poison(&self) -> bool {
        matches!(self, Ty::Error | Ty::Never)
    }

    pub fn primitive(name: &str) -> Option<Ty> {
        Some(match name {
            "int" | "i64" => INT,
            "i8" => Ty::Int(IntKind::I8),
            "i16" => Ty::Int(IntKind::I16),
            "i32" => Ty::Int(IntKind::I32),
            "u8" => Ty::Int(IntKind::U8),
            "u16" => Ty::Int(IntKind::U16),
            "u32" => Ty::Int(IntKind::U32),
            "u64" => Ty::Int(IntKind::U64),
            "f64" | "float" => F64,
            "f32" => Ty::Float(FloatKind::F32),
            "bool" => Ty::Bool,
            "str" => Ty::Str,
            "bytes" => Ty::Bytes,
            "PyObject" => Ty::PyObject,
            _ => return None,
        })
    }

    /// Replace type parameters by the given types.
    pub fn substitute(&self, names: &[String], with: &[Ty]) -> Ty {
        let go = |t: &Ty| t.substitute(names, with);
        match self {
            Ty::Param(name) => names.iter().position(|n| n == name).map_or_else(|| self.clone(), |i| with[i].clone()),
            Ty::List(t) => Ty::List(Box::new(go(t))),
            Ty::Set(t) => Ty::Set(Box::new(go(t))),
            Ty::Heap(t) => Ty::Heap(Box::new(go(t))),
            Ty::Optional(t) => Ty::Optional(Box::new(go(t))),
            Ty::Dict(k, v) => Ty::Dict(Box::new(go(k)), Box::new(go(v))),
            Ty::Result(t, e) => Ty::Result(Box::new(go(t)), Box::new(go(e))),
            Ty::Tuple(items) => Ty::Tuple(items.iter().map(go).collect()),
            Ty::Adt(name, args) => Ty::Adt(name.clone(), args.iter().map(go).collect()),
            Ty::Func(params, ret) => Ty::Func(params.iter().map(go).collect(), Box::new(go(ret))),
            other => other.clone(),
        }
    }
}

/// The most nodes of a type a message or a hover writes; the rest is `...`.
const SHOWN: usize = 64;

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut budget = SHOWN;
        self.write(f, &mut budget)
    }
}

impl Ty {
    /// The type as lotml writes it, `budget` nodes of it at most.
    fn write(&self, f: &mut fmt::Formatter<'_>, budget: &mut usize) -> fmt::Result {
        fn list(f: &mut fmt::Formatter<'_>, items: &[Ty], budget: &mut usize) -> fmt::Result {
            for (i, t) in items.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                t.write(f, budget)?;
            }
            Ok(())
        }
        if *budget == 0 {
            return f.write_str("...");
        }
        *budget -= 1;
        match self {
            Ty::Int(kind) => f.write_str(match kind {
                IntKind::I64 => "int",
                IntKind::I8 => "i8",
                IntKind::I16 => "i16",
                IntKind::I32 => "i32",
                IntKind::U8 => "u8",
                IntKind::U16 => "u16",
                IntKind::U32 => "u32",
                IntKind::U64 => "u64",
            }),
            Ty::Float(FloatKind::F64) => f.write_str("f64"),
            Ty::Float(FloatKind::F32) => f.write_str("f32"),
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("str"),
            Ty::Bytes => f.write_str("bytes"),
            Ty::PyObject => f.write_str("PyObject"),
            Ty::Unit => f.write_str("None"),
            Ty::List(t) => {
                f.write_str("[")?;
                t.write(f, budget)?;
                f.write_str("]")
            }
            Ty::Set(t) => {
                f.write_str("{")?;
                t.write(f, budget)?;
                f.write_str("}")
            }
            Ty::Heap(t) => {
                f.write_str("Heap[")?;
                t.write(f, budget)?;
                f.write_str("]")
            }
            Ty::Dict(k, v) => {
                f.write_str("{")?;
                k.write(f, budget)?;
                f.write_str(": ")?;
                v.write(f, budget)?;
                f.write_str("}")
            }
            Ty::Tuple(items) => {
                f.write_str("(")?;
                list(f, items, budget)?;
                f.write_str(if items.len() == 1 { ",)" } else { ")" })
            }
            Ty::Optional(t) => {
                t.write(f, budget)?;
                f.write_str("?")
            }
            Ty::Result(t, e) => {
                t.write(f, budget)?;
                f.write_str(" ! ")?;
                e.write(f, budget)
            }
            Ty::Adt(name, args) if args.is_empty() => f.write_str(name),
            Ty::Adt(name, args) => {
                write!(f, "{name}[")?;
                list(f, args, budget)?;
                f.write_str("]")
            }
            Ty::Param(name) => f.write_str(name),
            Ty::Var(_) => f.write_str("_"),
            Ty::Func(params, ret) => {
                f.write_str("fn(")?;
                list(f, params, budget)?;
                f.write_str(") -> ")?;
                ret.write(f, budget)
            }
            Ty::Dyn(name) => write!(f, "dyn {name}"),
            Ty::Module(name) => write!(f, "module {name}"),
            Ty::TypeName(name) => write!(f, "type {name}"),
            Ty::Never => f.write_str("never"),
            Ty::Error => f.write_str("{unknown}"),
        }
    }
}

/// The most nodes a type may have once its variables are replaced. A type is a tree, so a pair of
/// pairs doubles it at each step, and variables bound to one another share what a tree repeats:
/// past this, [`Infer::resolve`] gives an error rather than build it.
pub const TYPE_LIMIT: usize = 1024;

/// Inference variables and what each has been unified with.
#[derive(Default)]
pub struct Infer {
    bound: Vec<Option<Ty>>,
    /// Whether a resolution passed [`TYPE_LIMIT`] since [`Infer::take_overflow`] last asked.
    overflowed: Cell<bool>,
}

impl Infer {
    pub fn fresh(&mut self) -> Ty {
        self.bound.push(None);
        Ty::Var((self.bound.len() - 1) as u32)
    }

    /// `ty` with every bound variable replaced, as deep as it goes: an error, noted for
    /// [`Infer::take_overflow`], once that passes [`TYPE_LIMIT`] nodes and variables followed.
    pub fn resolve(&self, ty: &Ty) -> Ty {
        let mut budget = TYPE_LIMIT;
        self.expand(ty, &mut budget).unwrap_or_else(|| {
            self.overflowed.set(true);
            Ty::Error
        })
    }

    /// Whether a resolution passed [`TYPE_LIMIT`] since this last asked.
    pub fn take_overflow(&self) -> bool {
        self.overflowed.replace(false)
    }

    fn expand(&self, ty: &Ty, budget: &mut usize) -> Option<Ty> {
        *budget = budget.checked_sub(1)?;
        let all = |items: &[Ty], budget: &mut usize| -> Option<Vec<Ty>> {
            items.iter().map(|t| self.expand(t, budget)).collect()
        };
        Some(match ty {
            Ty::Var(v) => match self.bound.get(*v as usize).and_then(Option::as_ref) {
                Some(bound) => return self.expand(bound, budget),
                None => ty.clone(),
            },
            Ty::List(t) => Ty::List(Box::new(self.expand(t, budget)?)),
            Ty::Set(t) => Ty::Set(Box::new(self.expand(t, budget)?)),
            Ty::Heap(t) => Ty::Heap(Box::new(self.expand(t, budget)?)),
            Ty::Optional(t) => Ty::Optional(Box::new(self.expand(t, budget)?)),
            Ty::Dict(k, v) => Ty::Dict(Box::new(self.expand(k, budget)?), Box::new(self.expand(v, budget)?)),
            Ty::Result(t, e) => Ty::Result(Box::new(self.expand(t, budget)?), Box::new(self.expand(e, budget)?)),
            Ty::Tuple(items) => Ty::Tuple(all(items, budget)?),
            Ty::Adt(name, args) => Ty::Adt(name.clone(), all(args, budget)?),
            Ty::Func(params, ret) => Ty::Func(all(params, budget)?, Box::new(self.expand(ret, budget)?)),
            other => other.clone(),
        })
    }

    fn occurs(&self, var: u32, ty: &Ty) -> bool {
        fn mentions(ty: &Ty, var: u32) -> bool {
            match ty {
                Ty::Var(v) => *v == var,
                Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => mentions(t, var),
                Ty::Dict(a, b) | Ty::Result(a, b) => mentions(a, var) || mentions(b, var),
                Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().any(|t| mentions(t, var)),
                Ty::Func(params, ret) => params.iter().any(|t| mentions(t, var)) || mentions(ret, var),
                _ => false,
            }
        }
        mentions(&self.resolve(ty), var)
    }

    /// Make `a` and `b` the same type, binding variables; false when they cannot be.
    pub fn unify(&mut self, a: &Ty, b: &Ty) -> bool {
        let (a, b) = (self.resolve(a), self.resolve(b));
        match (&a, &b) {
            _ if a == b => true,
            (Ty::Error | Ty::Never, _) | (_, Ty::Error | Ty::Never) => true,
            (Ty::Var(v), other) | (other, Ty::Var(v)) => {
                if self.occurs(*v, other) {
                    return false;
                }
                self.bound[*v as usize] = Some(other.clone());
                true
            }
            (Ty::List(x), Ty::List(y))
            | (Ty::Set(x), Ty::Set(y))
            | (Ty::Heap(x), Ty::Heap(y))
            | (Ty::Optional(x), Ty::Optional(y)) => self.unify(x, y),
            (Ty::Dict(k1, v1), Ty::Dict(k2, v2)) | (Ty::Result(k1, v1), Ty::Result(k2, v2)) => {
                self.unify(k1, k2) & self.unify(v1, v2)
            }
            (Ty::Tuple(xs), Ty::Tuple(ys)) => {
                xs.len() == ys.len() && xs.iter().zip(ys).fold(true, |ok, (x, y)| self.unify(x, y) & ok)
            }
            (Ty::Adt(n1, xs), Ty::Adt(n2, ys)) => {
                n1 == n2 && xs.len() == ys.len() && xs.iter().zip(ys).fold(true, |ok, (x, y)| self.unify(x, y) & ok)
            }
            (Ty::Func(p1, r1), Ty::Func(p2, r2)) => {
                p1.len() == p2.len()
                    && p1.iter().zip(p2).fold(true, |ok, (x, y)| self.unify(x, y) & ok)
                    && self.unify(r1, r2)
            }
            _ => false,
        }
    }
}
