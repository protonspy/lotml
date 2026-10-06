//! Types, inference variables, and unification.

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

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            Ty::Unit => f.write_str("None"),
            Ty::List(t) => write!(f, "[{t}]"),
            Ty::Set(t) => write!(f, "{{{t}}}"),
            Ty::Heap(t) => write!(f, "Heap[{t}]"),
            Ty::Dict(k, v) => write!(f, "{{{k}: {v}}}"),
            Ty::Tuple(items) => {
                let inner: Vec<String> = items.iter().map(ToString::to_string).collect();
                if items.len() == 1 { write!(f, "({},)", inner[0]) } else { write!(f, "({})", inner.join(", ")) }
            }
            Ty::Optional(t) => write!(f, "{t}?"),
            Ty::Result(t, e) => write!(f, "{t} ! {e}"),
            Ty::Adt(name, args) if args.is_empty() => f.write_str(name),
            Ty::Adt(name, args) => {
                let inner: Vec<String> = args.iter().map(ToString::to_string).collect();
                write!(f, "{name}[{}]", inner.join(", "))
            }
            Ty::Param(name) => f.write_str(name),
            Ty::Var(_) => f.write_str("_"),
            Ty::Func(params, ret) => {
                let inner: Vec<String> = params.iter().map(ToString::to_string).collect();
                write!(f, "fn({}) -> {ret}", inner.join(", "))
            }
            Ty::Dyn(name) => write!(f, "dyn {name}"),
            Ty::Module(name) => write!(f, "module {name}"),
            Ty::TypeName(name) => write!(f, "type {name}"),
            Ty::Never => f.write_str("never"),
            Ty::Error => f.write_str("{unknown}"),
        }
    }
}

/// Inference variables and what each has been unified with.
#[derive(Default)]
pub struct Infer {
    bound: Vec<Option<Ty>>,
}

impl Infer {
    pub fn fresh(&mut self) -> Ty {
        self.bound.push(None);
        Ty::Var((self.bound.len() - 1) as u32)
    }

    /// `ty` with every bound variable replaced, as deep as it goes.
    pub fn resolve(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(v) => match self.bound.get(*v as usize).and_then(Clone::clone) {
                Some(bound) => self.resolve(&bound),
                None => ty.clone(),
            },
            Ty::List(t) => Ty::List(Box::new(self.resolve(t))),
            Ty::Set(t) => Ty::Set(Box::new(self.resolve(t))),
            Ty::Heap(t) => Ty::Heap(Box::new(self.resolve(t))),
            Ty::Optional(t) => Ty::Optional(Box::new(self.resolve(t))),
            Ty::Dict(k, v) => Ty::Dict(Box::new(self.resolve(k)), Box::new(self.resolve(v))),
            Ty::Result(t, e) => Ty::Result(Box::new(self.resolve(t)), Box::new(self.resolve(e))),
            Ty::Tuple(items) => Ty::Tuple(items.iter().map(|t| self.resolve(t)).collect()),
            Ty::Adt(name, args) => Ty::Adt(name.clone(), args.iter().map(|t| self.resolve(t)).collect()),
            Ty::Func(params, ret) => {
                Ty::Func(params.iter().map(|t| self.resolve(t)).collect(), Box::new(self.resolve(ret)))
            }
            other => other.clone(),
        }
    }

    fn occurs(&self, var: u32, ty: &Ty) -> bool {
        match self.resolve(ty) {
            Ty::Var(v) => v == var,
            Ty::List(t) | Ty::Set(t) | Ty::Heap(t) | Ty::Optional(t) => self.occurs(var, &t),
            Ty::Dict(a, b) | Ty::Result(a, b) => self.occurs(var, &a) || self.occurs(var, &b),
            Ty::Tuple(items) | Ty::Adt(_, items) => items.iter().any(|t| self.occurs(var, t)),
            Ty::Func(params, ret) => params.iter().any(|t| self.occurs(var, t)) || self.occurs(var, &ret),
            _ => false,
        }
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
