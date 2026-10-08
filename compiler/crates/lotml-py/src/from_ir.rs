//! The generic IR `lotml_ir::lower` writes, as Python's syntax tree in JSON the runtime turns back
//! into `ast` nodes (specs/python-on-ir): one Python function per function of the IR, a generic one
//! once; every node at the LotML position of the statement it was lowered from, a computed value at
//! its expression's, so a traceback names the `.lot` line and underlines the expression. The
//! checker's types decide the rest: which arithmetic traps outside its integer type, and which
//! values are copied — those a `var` or an `inout` holds, as they enter or leave it.

use std::collections::{HashMap, HashSet};

use lotml_check::TypeDef;
use lotml_check::ty::{FloatKind, IntKind, Ty};
use lotml_ir::ir::{
    Arg, BinOp, Block, Builtin, Callee, CmpOp, Const, Expr, FormatPart, Function, Local, Operand, Panic, Place, Proj,
    Stmt, StmtKind, UnOp,
};
use lotml_ir::lower::{DefaultOf, Lowered, prelude_type};
use lotml_syntax::span::Span;
use serde_json::{Map, Value, json};

use crate::boundary::descriptor;

// JSON nodes ----------------------------------------------------------------------------------

fn node(kind: &str, fields: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    map.insert("_".into(), Value::String(kind.into()));
    for (k, v) in fields {
        map.insert(k.into(), v);
    }
    Value::Object(map)
}

fn op(kind: &str) -> Value {
    node(kind, vec![])
}

fn load() -> Value {
    op("Load")
}

fn name(id: &str) -> Value {
    node("Name", vec![("id", id.into()), ("ctx", load())])
}

fn target(id: &str) -> Value {
    node("Name", vec![("id", id.into()), ("ctx", op("Store"))])
}

fn attr(value: Value, attr: &str) -> Value {
    node("Attribute", vec![("value", value), ("attr", attr.into()), ("ctx", load())])
}

fn rt(function: &str) -> Value {
    attr(name("__rt"), function)
}

/// A Python built-in, which no name of the program can shadow.
fn builtin(function: &str) -> Value {
    attr(rt("builtins"), function)
}

fn call(function: Value, args: Vec<Value>) -> Value {
    node("Call", vec![("func", function), ("args", Value::Array(args)), ("keywords", json!([]))])
}

fn call_kw(function: Value, args: Vec<Value>, keywords: Vec<(&str, Value)>) -> Value {
    let keywords: Vec<Value> =
        keywords.into_iter().map(|(k, v)| node("keyword", vec![("arg", k.into()), ("value", v)])).collect();
    node("Call", vec![("func", function), ("args", Value::Array(args)), ("keywords", Value::Array(keywords))])
}

fn method(receiver: Value, name: &str, args: Vec<Value>) -> Value {
    call(attr(receiver, name), args)
}

fn constant(value: Value) -> Value {
    node("Constant", vec![("value", value), ("kind", Value::Null)])
}

fn none() -> Value {
    constant(Value::Null)
}

fn int(value: i128) -> Value {
    constant(json!({"_int": value.to_string()}))
}

fn text(value: &str) -> Value {
    constant(value.into())
}

fn stmt_expr(value: Value) -> Value {
    node("Expr", vec![("value", value)])
}

fn assign(targets: Vec<Value>, value: Value) -> Value {
    node("Assign", vec![("targets", Value::Array(targets)), ("value", value), ("type_comment", Value::Null)])
}

fn tuple(items: Vec<Value>) -> Value {
    node("Tuple", vec![("elts", Value::Array(items)), ("ctx", load())])
}

fn subscript(value: Value, index: Value) -> Value {
    node("Subscript", vec![("value", value), ("slice", index), ("ctx", load())])
}

fn binop(left: Value, kind: &str, right: Value) -> Value {
    node("BinOp", vec![("left", left), ("op", op(kind)), ("right", right)])
}

fn compare(left: Value, kind: &str, right: Value) -> Value {
    node("Compare", vec![("left", left), ("ops", json!([op(kind)])), ("comparators", json!([right]))])
}

fn not(value: Value) -> Value {
    node("UnaryOp", vec![("op", op("Not")), ("operand", value)])
}

fn if_stmt(test: Value, body: Vec<Value>, orelse: Vec<Value>) -> Value {
    node("If", vec![("test", test), ("body", Value::Array(nonempty(body))), ("orelse", Value::Array(orelse))])
}

fn nonempty(mut body: Vec<Value>) -> Vec<Value> {
    if body.is_empty() {
        body.push(op("Pass"));
    }
    body
}

fn arguments(args: Vec<Value>, defaults: Vec<Value>) -> Value {
    node(
        "arguments",
        vec![
            ("posonlyargs", json!([])),
            ("args", Value::Array(args)),
            ("vararg", Value::Null),
            ("kwonlyargs", json!([])),
            ("kw_defaults", json!([])),
            ("kwarg", Value::Null),
            ("defaults", Value::Array(defaults)),
        ],
    )
}

fn arg(id: &str) -> Value {
    node("arg", vec![("arg", id.into()), ("annotation", Value::Null), ("type_comment", Value::Null)])
}

fn function_def(id: &str, args: Value, body: Vec<Value>) -> Value {
    node(
        "FunctionDef",
        vec![
            ("name", id.into()),
            ("args", args),
            ("body", Value::Array(nonempty(body))),
            ("decorator_list", json!([])),
            ("returns", Value::Null),
            ("type_comment", Value::Null),
            ("type_params", json!([])),
        ],
    )
}

/// The context of a target node turned to `Store`.
fn stored_ctx(mut target: Value) -> Value {
    if let Some(map) = target.as_object_mut()
        && map.contains_key("ctx")
    {
        map.insert("ctx".into(), op("Store"));
    }
    target
}

/// The C type a value of `ty` crosses into C as, by the name the runtime's `c_function` reads.
pub(crate) fn c_type(ty: &Ty) -> &'static str {
    match ty {
        Ty::Int(IntKind::I8) => "i8",
        Ty::Int(IntKind::I16) => "i16",
        Ty::Int(IntKind::I32) => "i32",
        Ty::Int(IntKind::I64) => "i64",
        Ty::Int(IntKind::U8) => "u8",
        Ty::Int(IntKind::U16) => "u16",
        Ty::Int(IntKind::U32) => "u32",
        Ty::Int(IntKind::U64) => "u64",
        Ty::Float(FloatKind::F32) => "f32",
        Ty::Float(_) => "f64",
        Ty::Bool => "bool",
        Ty::Str => "str",
        _ => "none",
    }
}

pub(crate) fn range_of(kind: IntKind) -> (i128, i128) {
    match kind {
        IntKind::I8 => (i8::MIN.into(), i8::MAX.into()),
        IntKind::I16 => (i16::MIN.into(), i16::MAX.into()),
        IntKind::I32 => (i32::MIN.into(), i32::MAX.into()),
        IntKind::I64 => (i64::MIN.into(), i64::MAX.into()),
        IntKind::U8 => (0, u8::MAX.into()),
        IntKind::U16 => (0, u16::MAX.into()),
        IntKind::U32 => (0, u32::MAX.into()),
        IntKind::U64 => (0, u64::MAX.into()),
    }
}

fn kind_name(kind: IntKind) -> &'static str {
    match kind {
        IntKind::I8 => "i8",
        IntKind::I16 => "i16",
        IntKind::I32 => "i32",
        IntKind::I64 => "i64",
        IntKind::U8 => "u8",
        IntKind::U16 => "u16",
        IntKind::U32 => "u32",
        IntKind::U64 => "u64",
    }
}

/// What a function of the IR is called in Python: a function by its LotML name; a method, a
/// lambda, a test, a default under the `__` the checker keeps from the program's own names, a
/// method's with its type's length, as its symbol has it, so `A_b.c` and `A.b_c` stay apart.
fn py_name(symbol: &str) -> String {
    if let Some(rest) = symbol.strip_prefix("lf_") {
        return rest.to_string();
    }
    if let Some((owner, method)) = method_of(symbol) {
        return method_name(owner, method);
    }
    for (prefix, python) in [("ll", "__lambda_"), ("lt_test", "__test_"), ("ld", "__default_")] {
        if let Some(k) = symbol.strip_prefix(prefix)
            && !k.is_empty()
            && k.chars().all(|c| c.is_ascii_digit())
        {
            return format!("{python}{k}");
        }
    }
    symbol.to_string()
}

/// The Python function of the method `method` of the type `owner`.
fn method_name(owner: &str, method: &str) -> String {
    format!("__m{}_{owner}_{method}", owner.len())
}

/// The type and the method of a method's symbol, `lm<length of the type>_<type>_<method>`.
fn method_of(symbol: &str) -> Option<(&str, &str)> {
    let rest = symbol.strip_prefix("lm")?;
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let n: usize = rest[..digits].parse().ok()?;
    let tail = rest[digits..].strip_prefix('_')?;
    let owner = tail.get(..n)?;
    let method = tail.get(n..)?.strip_prefix('_')?;
    Some((owner, method))
}

// The module ----------------------------------------------------------------------------------

/// The Python module of the program `lowered`, read from `text`.
pub fn module(text: &str, lowered: &Lowered) -> Value {
    let mut m = ModuleWriter::new(text, lowered);
    m.module()
}

struct ModuleWriter<'l> {
    lowered: &'l Lowered,
    lines: Vec<u32>,
    /// Sum types whose variants all have no fields: their values never change.
    enums: HashSet<String>,
    /// Each Python function the program calls, by module and function, with the result its
    /// interface declares, in the order first called: the `k`th is bound to `__py<k>`.
    python_calls: Vec<(String, String, Ty)>,
    /// The default of each parameter, by the function's symbol and the parameter: its index.
    param_defaults: HashMap<(String, String), usize>,
}

impl<'l> ModuleWriter<'l> {
    fn new(text: &str, lowered: &'l Lowered) -> ModuleWriter<'l> {
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(i, _)| i as u32 + 1));
        let enums = lowered
            .declared
            .iter()
            .filter(|(_, d)| matches!(d, TypeDef::Sum { variants, .. } if variants.iter().all(|v| v.fields.is_none())))
            .map(|(n, _)| n.clone())
            .collect();
        let param_defaults = lowered
            .defaults
            .iter()
            .enumerate()
            .filter_map(|(k, d)| match &d.of {
                DefaultOf::Param { function, param } => Some(((function.clone(), param.clone()), k)),
                DefaultOf::Field { .. } => None,
            })
            .collect();
        ModuleWriter { lowered, lines, enums, python_calls: Vec::new(), param_defaults }
    }

    /// Line (from 1) and UTF-8 column (from 0) of a byte offset.
    fn position(&self, offset: u32) -> (usize, u32) {
        let line = self.lines.partition_point(|&start| start <= offset).max(1);
        (line, offset - self.lines[line - 1])
    }

    /// `value` placed at `span`, unless it already has a place.
    fn at(&self, mut value: Value, span: Span) -> Value {
        if let Some(map) = value.as_object_mut()
            && !map.contains_key("lineno")
            && map.contains_key("_")
        {
            let (line, column) = self.position(span.start);
            let (end_line, end_column) = self.position(span.end.max(span.start));
            map.insert("lineno".into(), line.into());
            map.insert("col_offset".into(), column.into());
            map.insert("end_lineno".into(), end_line.into());
            map.insert("end_col_offset".into(), end_column.into());
        }
        value
    }

    fn module(&mut self) -> Value {
        let lowered = self.lowered;
        let mut functions = Vec::new();
        let mut attached = Vec::new();
        let mut defaults = Vec::new();
        for d in &lowered.defaults {
            defaults.push(self.function(&d.function));
        }
        let declarations = self.declarations();
        for f in &lowered.functions {
            functions.push(self.function(f));
            if let Some((owner, m)) = method_of(&f.name) {
                let is_method = f.params.first().is_some_and(|&p| f.locals[p].name.as_deref() == Some("self"));
                let attach =
                    call(rt("attach"), vec![name(owner), text(m), name(&py_name(&f.name)), constant(is_method.into())]);
                attached.push(self.at(stmt_expr(attach), f.span));
                let traced = call(rt("rename"), vec![name(&py_name(&f.name)), text(&format!("__{owner}_{m}"))]);
                attached.push(self.at(stmt_expr(traced), f.span));
            }
            if f.name.starts_with("ll") && f.source_name == "<lambda>" {
                let rename = call(rt("rename"), vec![name(&py_name(&f.name)), text("<lambda>")]);
                attached.push(self.at(stmt_expr(rename), f.span));
            }
        }
        let mut tests = Vec::new();
        for (test_name, symbol) in &self.lowered.tests {
            let entry = tuple(vec![text(test_name), name(&py_name(symbol))]);
            let span = self.lowered.functions.iter().find(|f| &f.name == symbol).map_or(Span::new(0, 0), |f| f.span);
            tests.push(self.at(stmt_expr(method(name("__tests"), "append", vec![entry])), span));
        }
        let mut imports = Vec::new();
        for (symbol, (params, ret)) in &self.lowered.c_functions {
            let library = self.lowered.c_libraries.get(symbol).cloned().unwrap_or_default();
            let params: Vec<Value> = params.iter().map(|p| text(c_type(p))).collect();
            let loaded = call(
                rt("c_function"),
                vec![
                    text(&library),
                    text(symbol),
                    node("List", vec![("elts", Value::Array(params)), ("ctx", load())]),
                    text(c_type(ret)),
                ],
            );
            imports.push(self.at(assign(vec![target(&format!("__c_{symbol}"))], loaded), Span::new(0, 0)));
        }
        for (k, (module, function, ret)) in self.python_calls.iter().enumerate() {
            let returns = match ret {
                Ty::Result(value, _) => descriptor(value),
                other => descriptor(other),
            };
            let bound = call(rt("foreign"), vec![text(module), text(function), text(&returns.to_string())]);
            imports.push(self.at(assign(vec![target(&format!("__py{k}"))], bound), Span::new(0, 0)));
        }
        let body: Vec<Value> = imports
            .into_iter()
            .chain(defaults)
            .chain(declarations)
            .chain(functions)
            .chain(attached)
            .chain(tests)
            .collect();
        node("Module", vec![("body", Value::Array(body)), ("type_ignores", json!([]))])
    }

    /// The program's records and variants: a class each, built by the runtime with its fields
    /// and their defaults; a variant without fields one value.
    fn declarations(&self) -> Vec<Value> {
        let mut out = Vec::new();
        let field_default = |owner: &str, field: &str| {
            self.lowered
                .defaults
                .iter()
                .position(|d| matches!(&d.of, DefaultOf::Field { owner: o, field: f } if o == owner && f == field))
        };
        let record = |id: &str, fields: Vec<String>| {
            let names: Vec<Value> = fields.iter().map(|f| text(f)).collect();
            let mut keys = Vec::new();
            let mut values = Vec::new();
            for f in &fields {
                if let Some(k) = field_default(id, f) {
                    keys.push(text(f));
                    values.push(name(&format!("__default_{k}")));
                }
            }
            let builder = call(
                rt("record"),
                vec![
                    text(id),
                    tuple(names),
                    node("Dict", vec![("keys", Value::Array(keys)), ("values", Value::Array(values))]),
                ],
            );
            assign(vec![target(id)], builder)
        };
        let span = Span::new(0, 0);
        for (type_name, def) in &self.lowered.declared {
            match def {
                TypeDef::Record { fields, .. } => {
                    let names =
                        fields.iter().enumerate().map(|(i, f)| f.name.clone().unwrap_or(format!("_{i}"))).collect();
                    out.push(self.at(record(type_name, names), span));
                }
                TypeDef::Sum { variants, .. } => {
                    for (k, v) in variants.iter().enumerate() {
                        match &v.fields {
                            Some(fields) => {
                                let names = fields
                                    .iter()
                                    .enumerate()
                                    .map(|(i, f)| f.name.clone().unwrap_or(format!("_{i}")))
                                    .collect();
                                out.push(self.at(record(&v.name, names), span));
                            }
                            None => {
                                let unit = assign(vec![target(&v.name)], call(rt("Unit"), vec![text(&v.name)]));
                                out.push(self.at(unit, span));
                                let register = assign(
                                    vec![node(
                                        "Attribute",
                                        vec![
                                            ("value", name("__variants")),
                                            ("attr", v.name.clone().into()),
                                            ("ctx", op("Store")),
                                        ],
                                    )],
                                    name(&v.name),
                                );
                                out.push(self.at(register, span));
                            }
                        }
                        let tag = assign(
                            vec![node(
                                "Attribute",
                                vec![("value", name(&v.name)), ("attr", "_lotml_tag".into()), ("ctx", op("Store"))],
                            )],
                            int(k as i128),
                        );
                        out.push(self.at(tag, span));
                    }
                }
            }
        }
        out
    }

    fn function(&mut self, f: &'l Function) -> Value {
        let mut w = Writer::new(self, f);
        w.function()
    }

    /// Whether a value of `ty` can change in place, so sharing it with a `var` is visible.
    fn changeable(&self, ty: &Ty) -> bool {
        match ty {
            Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::Str | Ty::Bytes | Ty::Unit | Ty::Func(..) => false,
            Ty::Never | Ty::Error | Ty::Module(_) | Ty::TypeName(_) => false,
            Ty::Optional(inner) => self.changeable(inner),
            Ty::Tuple(items) => items.iter().any(|t| self.changeable(t)),
            Ty::Adt(name, _) => !self.enums.contains(name),
            Ty::Result(value, error) => self.changeable(value) || self.changeable(error),
            Ty::List(_) | Ty::Set(_) | Ty::Dict(..) | Ty::Heap(_) => true,
            Ty::Param(_) | Ty::Var(_) | Ty::Dyn(_) => true,
        }
    }

    /// A collection whose elements cannot change: a shallow copy is a full one.
    fn shallow(&self, ty: &Ty) -> bool {
        match ty {
            Ty::List(item) | Ty::Set(item) | Ty::Heap(item) => !self.changeable(item),
            Ty::Dict(_, value) => !self.changeable(value),
            _ => false,
        }
    }

    /// The field names of the record `ty`, or of its variant `variant`.
    fn field_names(&self, ty: &Ty, variant: Option<usize>) -> Vec<String> {
        let Ty::Adt(type_name, _) = ty else { return Vec::new() };
        let def = self.lowered.declared.get(type_name).cloned().or_else(|| prelude_type(type_name));
        let fields = match (def.as_ref(), variant) {
            (Some(TypeDef::Record { fields, .. }), _) => fields.clone(),
            (Some(TypeDef::Sum { variants, .. }), Some(k)) => {
                variants.get(k).and_then(|v| v.fields.clone()).unwrap_or_default()
            }
            _ => Vec::new(),
        };
        fields.iter().enumerate().map(|(i, f)| f.name.clone().unwrap_or(format!("_{i}"))).collect()
    }

    /// The class a value of `ty` (its variant `variant`) is built by.
    fn class_name(&self, ty: &Ty, variant: Option<usize>) -> String {
        let Ty::Adt(type_name, _) = ty else { return String::new() };
        match (self.lowered.declared.get(type_name), variant) {
            (Some(TypeDef::Sum { variants, .. }), Some(k)) => variants.get(k).map_or(String::new(), |v| v.name.clone()),
            _ => type_name.clone(),
        }
    }

    /// The type of a field of the record or variant `ty`.
    fn field_type(&self, ty: &Ty, variant: Option<usize>, index: usize) -> Ty {
        let Ty::Adt(type_name, args) = ty else { return Ty::Error };
        let def = self.lowered.declared.get(type_name).cloned().or_else(|| prelude_type(type_name));
        let (params, fields) = match (def, variant) {
            (Some(TypeDef::Record { params, fields }), _) => (params, fields),
            (Some(TypeDef::Sum { params, variants }), Some(k)) => {
                (params, variants.get(k).and_then(|v| v.fields.clone()).unwrap_or_default())
            }
            _ => return Ty::Error,
        };
        fields.get(index).map_or(Ty::Error, |f| f.ty.substitute(&params, args))
    }
}

// A function ----------------------------------------------------------------------------------

struct Writer<'m, 'l> {
    m: &'m mut ModuleWriter<'l>,
    f: &'l Function,
    /// The Python name of each local.
    names: Vec<String>,
    /// The locals whose value changes in place: a `var`, an `inout`, a temporary a change is
    /// made through. A value entering one is copied.
    mutated: Vec<bool>,
    /// The locals whose value a mutated one may still hold: bound or stored elsewhere, it is
    /// copied.
    rooted: Vec<bool>,
    /// The temporaries holding a value no one else holds, built where they are.
    fresh: Vec<bool>,
    temporaries: usize,
    /// The span of the statement being written, and of the expression it computes.
    span: Span,
    expr_span: Span,
}

impl<'m, 'l> Writer<'m, 'l> {
    fn new(m: &'m mut ModuleWriter<'l>, f: &'l Function) -> Writer<'m, 'l> {
        let n = f.locals.len();
        let mut names = Vec::with_capacity(n);
        for (l, info) in f.locals.iter().enumerate() {
            names.push(match &info.name {
                Some(n) if f.params.contains(&l) => n.clone(),
                Some(n) => format!("__l{l}_{n}"),
                None => format!("__t{l}"),
            });
        }
        let mut w = Writer {
            m,
            f,
            names,
            mutated: vec![false; n],
            rooted: vec![false; n],
            fresh: vec![false; n],
            temporaries: 0,
            span: f.span,
            expr_span: f.span,
        };
        w.analyse();
        w
    }

    fn temporary(&mut self) -> String {
        self.temporaries += 1;
        format!("__b{}", self.temporaries)
    }

    fn ty(&self, l: Local) -> &Ty {
        &self.f.locals[l].ty
    }

    fn at(&self, value: Value) -> Value {
        self.m.at(value, self.span)
    }

    fn at_expr(&self, value: Value) -> Value {
        self.m.at(value, self.expr_span)
    }

    // What changes and what is shared -------------------------------------------------------

    /// Which locals change in place, which may hold what one of those holds, and which hold a
    /// value built where it is.
    fn analyse(&mut self) {
        let f = self.f;
        for (l, info) in f.locals.iter().enumerate() {
            if info.by_ref {
                self.mutated[l] = true;
            }
        }
        let mut defs: Vec<Vec<&Expr>> = vec![Vec::new(); f.locals.len()];
        let mut pushes: Vec<Vec<&Arg>> = vec![Vec::new(); f.locals.len()];
        let mut roots = Vec::new();
        walk(&f.body, &mut |stmt| match &stmt.kind {
            StmtKind::Let(l, e) => {
                defs[*l].push(e);
                slots(e, &mut roots);
                changes(e, &mut roots);
            }
            StmtKind::Do(e) => {
                slots(e, &mut roots);
                changes(e, &mut roots);
            }
            StmtKind::Store(place, _) if !place.proj.is_empty() => roots.push(place.local),
            StmtKind::Mutate { place, args, .. } => {
                roots.push(place.local);
                if place.proj.is_empty() {
                    pushes[place.local].extend(args.iter());
                }
                for a in args {
                    if let Arg::Slot(p) = a {
                        roots.push(p.local);
                    }
                }
            }
            _ => {}
        });
        for (l, d) in defs.iter().enumerate() {
            self.fresh[l] = f.locals[l].name.is_none() && !f.params.contains(&l) && !d.is_empty();
        }
        loop {
            let mut changed = false;
            for l in 0..f.locals.len() {
                if !self.fresh[l] {
                    continue;
                }
                let fresh = defs[l].iter().all(|e| self.fresh_expr(e, &self.f.locals[l].ty))
                    && pushes[l].iter().all(|a| a.operand().is_none_or(|o| self.fresh_operand(o)));
                if !fresh {
                    self.fresh[l] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        for l in roots {
            if !self.fresh[l] {
                self.mutated[l] = true;
            }
        }
        self.rooted = self.mutated.clone();
        let mut edges: Vec<(Vec<Local>, Vec<Local>)> = Vec::new();
        walk(&f.body, &mut |stmt| {
            if let StmtKind::Let(l, e) = &stmt.kind
                && aliasing(e)
            {
                let mut from = Vec::new();
                e.operands(&mut |o| {
                    if let Operand::Local(x) = o {
                        from.push(*x);
                    }
                });
                edges.push((from, std::iter::once(*l).chain(e.outs()).collect()));
            }
        });
        loop {
            let mut changed = false;
            for (from, to) in &edges {
                if from.iter().any(|&x| self.rooted[x]) {
                    for &x in to {
                        if !self.rooted[x] {
                            self.rooted[x] = true;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn fresh_operand(&self, o: &Operand) -> bool {
        match o {
            Operand::Const(_) => true,
            Operand::Local(l) => !self.m.changeable(self.ty(*l)) || self.fresh[*l],
        }
    }

    /// Whether `e`, of type `ty`, builds a value no one else holds.
    fn fresh_expr(&self, e: &Expr, ty: &Ty) -> bool {
        if !self.m.changeable(ty) {
            return true;
        }
        match e {
            Expr::ListNew { items, .. } | Expr::TupleNew { items, .. } | Expr::SetNew { items, .. } => {
                items.iter().all(|o| self.fresh_operand(o))
            }
            Expr::Construct { fields, .. } => fields.iter().all(|o| self.fresh_operand(o)),
            Expr::DictNew { items, .. } => items.iter().all(|(k, v)| self.fresh_operand(k) && self.fresh_operand(v)),
            Expr::OptNew { value, .. } => value.as_ref().is_none_or(|o| self.fresh_operand(o)),
            Expr::ResultNew { value, .. } | Expr::OptIf { value, .. } => self.fresh_operand(value),
            Expr::Rt { op, .. } => self.m.shallow(ty) && !matches!(op, Builtin::ListCopy | Builtin::DictCopy),
            _ => false,
        }
    }

    fn is_rooted(&self, o: &Operand) -> bool {
        matches!(o, Operand::Local(l) if self.rooted[*l])
    }

    /// `value`, of type `ty`, copied: deeply, unless its elements cannot change.
    fn copy(&self, value: Value, ty: &Ty) -> Value {
        let function = if self.m.shallow(ty) { "shallow" } else { "copy" };
        self.at_expr(call(rt(function), vec![value]))
    }

    /// The operand `o` stored somewhere it outlives this read — in a container, an argument, a
    /// returned value, a binding: copied when a `var` may still change it, or when `into_var`, a
    /// changed container it is stored in, would share a value someone else holds.
    fn stored(&mut self, o: &Operand, into_var: bool) -> Value {
        let value = self.operand(o);
        let ty = self.operand_ty(o);
        if self.m.changeable(&ty) && (self.is_rooted(o) || (into_var && !self.fresh_operand(o))) {
            return self.copy(value, &ty);
        }
        value
    }

    fn operand_ty(&self, o: &Operand) -> Ty {
        match o {
            Operand::Local(l) => self.ty(*l).clone(),
            Operand::Const(Const::Int(_, k)) => Ty::Int(*k),
            Operand::Const(Const::Float(_)) => Ty::Float(FloatKind::F64),
            Operand::Const(Const::Bool(_)) => Ty::Bool,
            Operand::Const(Const::Str(_) | Const::Char(_)) => Ty::Str,
            Operand::Const(Const::Bytes(_)) => Ty::Bytes,
            Operand::Const(Const::Unit | Const::Null) => Ty::Unit,
        }
    }

    // The function --------------------------------------------------------------------------

    fn function(&mut self) -> Value {
        let f = self.f;
        let py = py_name(&f.name);
        let mut args = Vec::new();
        let mut defaults = Vec::new();
        let mut prologue = Vec::new();
        let mut has_default = false;
        for &p in &f.params {
            let pname = self.names[p].clone();
            args.push(self.m.at(arg(&pname), f.span));
            let default = self.f.locals[p]
                .name
                .as_ref()
                .and_then(|n| self.m.param_defaults.get(&(f.name.clone(), n.clone())).copied());
            match default {
                Some(k) => {
                    has_default = true;
                    defaults.push(rt("MISSING"));
                    let missing = compare(name(&pname), "Is", rt("MISSING"));
                    let given = assign(vec![target(&pname)], call(name(&format!("__default_{k}")), vec![]));
                    prologue.push(self.at(if_stmt(missing, vec![self.at(given)], vec![])));
                }
                None if has_default => defaults.push(rt("MISSING")),
                None => {}
            }
            if self.mutated[p] && !self.f.locals[p].by_ref && self.m.changeable(self.ty(p)) {
                let ty = self.ty(p).clone();
                let copied = self.copy(name(&pname), &ty);
                prologue.push(self.at(assign(vec![target(&pname)], copied)));
            }
        }
        let mut body = prologue;
        body.extend(self.block(&f.body));
        self.m.at(function_def(&py, arguments(args, defaults), body), f.span)
    }

    fn block(&mut self, block: &Block) -> Vec<Value> {
        let mut out = Vec::new();
        for stmt in block {
            out.extend(self.stmt(stmt));
        }
        out
    }

    /// A local read: an `inout` one through its box.
    fn local(&self, l: Local) -> Value {
        if self.f.locals[l].by_ref { attr(name(&self.names[l]), "value") } else { name(&self.names[l]) }
    }

    fn local_target(&self, l: Local) -> Value {
        if self.f.locals[l].by_ref {
            node("Attribute", vec![("value", name(&self.names[l])), ("attr", "value".into()), ("ctx", op("Store"))])
        } else {
            target(&self.names[l])
        }
    }

    fn operand(&mut self, o: &Operand) -> Value {
        match o {
            Operand::Local(l) => self.local(*l),
            Operand::Const(c) => constant_of(c),
        }
    }

    fn stmt(&mut self, stmt: &Stmt) -> Vec<Value> {
        self.span = stmt.span;
        self.expr_span = stmt.at;
        match &stmt.kind {
            StmtKind::Let(l, e) => self.let_stmt(*l, e),
            StmtKind::Do(e) => {
                let mut out = Vec::new();
                let value = self.expr(e, None, &mut out);
                out.push(self.at(stmt_expr(self.at_expr(value))));
                out
            }
            StmtKind::Store(place, v) => {
                let into_var = self.mutated[place.local];
                let value = self.stored(v, into_var && !place.proj.is_empty());
                let mut out = Vec::new();
                let place_target = self.place_target(place, &mut out);
                out.push(self.at(assign(vec![place_target], value)));
                out
            }
            StmtKind::Mutate { op, place, args, result, .. } => self.mutate(*op, place, args, *result),
            StmtKind::If(test, then, otherwise) => {
                let test = self.operand(test);
                let then = self.block(then);
                let otherwise = self.block(otherwise);
                vec![self.at(if_stmt(test, then, otherwise))]
            }
            StmtKind::Loop(body) => {
                let body = self.block(body);
                vec![self.at(node(
                    "While",
                    vec![
                        ("test", constant(true.into())),
                        ("body", Value::Array(nonempty(body))),
                        ("orelse", json!([])),
                    ],
                ))]
            }
            StmtKind::Break => vec![self.at(op("Break"))],
            StmtKind::Continue => vec![self.at(op("Continue"))],
            StmtKind::Return(v) => {
                let value = match v {
                    Some(v) => self.stored(v, false),
                    None => none(),
                };
                vec![self.at(node("Return", vec![("value", value)]))]
            }
            StmtKind::ForRange { var, start, stop, step, body, exit } => {
                let range = call(builtin("range"), vec![self.operand(start), self.operand(stop), self.operand(step)]);
                let range = self.at_expr(range);
                let body = self.block(body);
                let exit = self.block(exit);
                vec![self.at(node(
                    "For",
                    vec![
                        ("target", self.local_target(*var)),
                        ("iter", range),
                        ("body", Value::Array(nonempty(body))),
                        ("orelse", Value::Array(exit)),
                        ("type_comment", Value::Null),
                    ],
                ))]
            }
            StmtKind::ForStr { var, over, body, exit } => {
                let over = self.operand(over);
                let body = self.block(body);
                let exit = self.block(exit);
                vec![self.at(node(
                    "For",
                    vec![
                        ("target", self.local_target(*var)),
                        ("iter", over),
                        ("body", Value::Array(nonempty(body))),
                        ("orelse", Value::Array(exit)),
                        ("type_comment", Value::Null),
                    ],
                ))]
            }
            StmtKind::Panic(panic) => {
                let raised = match panic {
                    Panic::Todo => stmt_expr(call(rt("todo"), vec![])),
                    Panic::Assert(expression) => {
                        stmt_expr(call(rt("assertion_failed"), vec![text(expression), none(), none(), none()]))
                    }
                    Panic::Value(message) => node(
                        "Raise",
                        vec![("exc", call(builtin("ValueError"), vec![text(message)])), ("cause", Value::Null)],
                    ),
                };
                vec![self.at(self.at_expr(raised))]
            }
            StmtKind::Inc(_) | StmtKind::Dec(_) | StmtKind::DropReuse { .. } => Vec::new(),
            StmtKind::Enter => vec![self.at(self.at_expr(stmt_expr(call(rt("enter"), vec![]))))],
            StmtKind::Leave => vec![self.at(stmt_expr(call(rt("leave"), vec![])))],
        }
    }

    fn let_stmt(&mut self, l: Local, e: &Expr) -> Vec<Value> {
        let mut out = Vec::new();
        let ty = self.ty(l).clone();
        let outs = e.outs();
        let value = self.expr(e, Some(l), &mut out);
        let value = self.at_expr(value);
        let snapshot = self.f.locals[l].name.is_none() && matches!(e, Expr::Use(Operand::Local(o)) if self.mutated[*o]);
        let mutated = (self.mutated[l] && !self.fresh_expr(e, &ty)) || snapshot;
        let named = self.f.locals[l].name.is_some();
        let shared = named && aliasing(e) && {
            let mut any = false;
            e.operands(&mut |o| any |= self.is_rooted(o));
            any
        };
        let value = if self.m.changeable(&ty) && outs.is_empty() && (mutated || shared) {
            self.copy(value, &ty)
        } else {
            value
        };
        let targets = if outs.is_empty() {
            vec![self.local_target(l)]
        } else {
            let mut all = vec![self.local_target(l)];
            all.extend(outs.iter().map(|&o| self.local_target(o)));
            vec![node("Tuple", vec![("elts", Value::Array(all)), ("ctx", op("Store"))])]
        };
        out.push(self.at(assign(targets, value)));
        if let Some(check) = self.overflow_check(l, e) {
            out.push(check);
        }
        out
    }

    /// The statement that stops the program when `l`, just given the integer result of `e`, left
    /// its type.
    fn overflow_check(&self, l: Local, e: &Expr) -> Option<Value> {
        let kind = match e {
            Expr::Binary(op, _, _, Ty::Int(kind))
                if !matches!(
                    op,
                    BinOp::Mod | BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shr | BinOp::TrueDiv
                ) =>
            {
                *kind
            }
            Expr::Unary(UnOp::Neg | UnOp::Abs | UnOp::Invert, _, Ty::Int(kind)) => *kind,
            _ => return None,
        };
        let (low, high) = range_of(kind);
        let value = self.local(l);
        let inside = node(
            "Compare",
            vec![
                ("left", int(low)),
                ("ops", json!([op("LtE"), op("LtE")])),
                ("comparators", json!([value.clone(), int(high)])),
            ],
        );
        let trap = stmt_expr(call(rt("overflow"), vec![value]));
        Some(self.at_expr(if_stmt(not(inside), vec![self.at_expr(trap)], vec![])))
    }

    // Places ------------------------------------------------------------------------------------

    /// The place `place` read, and its type.
    fn place_value(&mut self, place: &Place) -> (Value, Ty) {
        let mut value = self.local(place.local);
        let mut ty = self.ty(place.local).clone();
        for proj in &place.proj {
            (value, ty) = self.project(value, &ty, proj);
        }
        (value, ty)
    }

    fn project(&mut self, value: Value, ty: &Ty, proj: &Proj) -> (Value, Ty) {
        match proj {
            Proj::Index(i) | Proj::OwnedIndex(i) => {
                let elem = match ty {
                    Ty::List(t) | Ty::Heap(t) => (**t).clone(),
                    _ => Ty::Error,
                };
                (subscript(value, self.operand(i)), elem)
            }
            Proj::Field(index) => {
                let names = self.m.field_names(ty, None);
                let field = names.get(*index).cloned().unwrap_or_default();
                let field_ty = self.m.field_type(ty, None, *index);
                (attr(value, &field), field_ty)
            }
            Proj::Key(k) => {
                let v = match ty {
                    Ty::Dict(_, v) => (**v).clone(),
                    _ => Ty::Error,
                };
                (subscript(value, self.operand(k)), v)
            }
            Proj::SetDefault(k, d) => {
                let v = match ty {
                    Ty::Dict(_, v) => (**v).clone(),
                    _ => Ty::Error,
                };
                let default = self.stored(d, true);
                (method(value, "setdefault", vec![self.operand(k), default]), v)
            }
        }
    }

    /// The place `place` as an assignment's target, statements reaching it written to `out`.
    fn place_target(&mut self, place: &Place, out: &mut Vec<Value>) -> Value {
        let Some((last, init)) = place.proj.split_last() else { return self.local_target(place.local) };
        let mut value = self.local(place.local);
        let mut ty = self.ty(place.local).clone();
        for proj in init {
            (value, ty) = self.project(value, &ty, proj);
        }
        match last {
            Proj::SetDefault(k, d) => {
                let default = self.stored(d, true);
                let key = self.operand(k);
                out.push(self.at(stmt_expr(method(value.clone(), "setdefault", vec![key.clone(), default]))));
                stored_ctx(subscript(value, key))
            }
            proj => {
                let (target, _) = self.project(value, &ty, proj);
                stored_ctx(target)
            }
        }
    }

    // Expressions ---------------------------------------------------------------------------

    /// The value of `e`, which `l` is given when it is a `Let`'s; statements it needs first written
    /// to `out`.
    fn expr(&mut self, e: &Expr, l: Option<Local>, out: &mut Vec<Value>) -> Value {
        match e {
            Expr::Use(o) => self.operand(o),
            Expr::Binary(o, a, b, ty) => {
                let (a, b) = (self.operand(a), self.operand(b));
                self.binary(*o, a, b, ty)
            }
            Expr::Unary(o, a, ty) => {
                let a = self.operand(a);
                match o {
                    UnOp::Neg => node("UnaryOp", vec![("op", op("USub")), ("operand", a)]),
                    UnOp::Invert => node("UnaryOp", vec![("op", op("Invert")), ("operand", a)]),
                    UnOp::Not => not(a),
                    UnOp::Abs if matches!(ty, Ty::Int(_)) => call(builtin("abs"), vec![a]),
                    UnOp::Abs => call(builtin("abs"), vec![a]),
                }
            }
            Expr::Compare(o, a, b, _) => {
                let (a, b) = (self.operand(a), self.operand(b));
                let kind = match o {
                    CmpOp::Lt => "Lt",
                    CmpOp::Le => "LtE",
                    CmpOp::Gt => "Gt",
                    CmpOp::Ge => "GtE",
                    CmpOp::Eq => "Eq",
                    CmpOp::Ne => "NotEq",
                };
                compare(a, kind, b)
            }
            Expr::Convert(v, _, to) => {
                let v = self.operand(v);
                match to {
                    Ty::Float(_) => call(builtin("float"), vec![v]),
                    Ty::Int(IntKind::I64) => call(rt("_int"), vec![v]),
                    Ty::Int(kind) => call(subscript(rt("SIZED"), text(kind_name(*kind))), vec![v]),
                    Ty::Bool => call(builtin("bool"), vec![v]),
                    _ => v,
                }
            }
            Expr::MinMax { max, a, b, .. } => {
                let (a, b) = (self.operand(a), self.operand(b));
                call(builtin(if *max { "max" } else { "min" }), vec![a, b])
            }
            Expr::Call(callee, args) => {
                let args: Vec<Value> = args.iter().map(|a| self.stored(a, false)).collect();
                call(name(&py_name(callee)), args)
            }
            Expr::CallSlots(callee, args) => self.call_slots(name(&py_name(callee)), args, out),
            Expr::CallGeneric { callee, args } => match callee {
                Callee::Function { name: f, .. } => self.call_slots(name(f), args, out),
                Callee::Method { owner: Ty::Adt(owner, _), method: m, .. } => {
                    self.call_slots(name(&method_name(owner, m)), args, out)
                }
                Callee::Method { method: m, .. } => {
                    // A type parameter's method: the method of the class the receiver is.
                    let receiver = args.first().and_then(|a| match a {
                        Arg::Value(o) => Some(self.operand(o)),
                        Arg::Slot(p) => Some(self.place_value(p).0),
                        _ => None,
                    });
                    let function = attr(call(builtin("type"), vec![receiver.unwrap_or_else(none)]), m);
                    self.call_slots(function, args, out)
                }
            },
            Expr::Print { args, sep, end } => {
                let values: Vec<Value> = args.iter().map(|(o, _)| self.operand(o)).collect();
                let mut keywords = Vec::new();
                if let Some(sep) = sep {
                    keywords.push(("sep", self.operand(sep)));
                }
                if let Some(end) = end {
                    keywords.push(("end", self.operand(end)));
                }
                call_kw(subscript(name("__builtins__"), text("print")), values, keywords)
            }
            Expr::Rt { op, args, .. } | Expr::RtValue { op, args, .. } => self.builtin(*op, args),
            Expr::Contains { container, item, .. } => {
                let (c, i) = (self.operand(container), self.operand(item));
                compare(i, "In", c)
            }
            Expr::Format(parts) => self.format(parts),
            Expr::ToStr(v, _) => {
                let v = self.operand(v);
                call(builtin("str"), vec![v])
            }
            Expr::Len(v, _) => {
                let v = self.operand(v);
                call(builtin("len"), vec![v])
            }
            Expr::ListNew { items, .. } => {
                let items: Vec<Value> = items.iter().map(|o| self.stored(o, false)).collect();
                let list = node("List", vec![("elts", Value::Array(items)), ("ctx", load())]);
                if l.is_some_and(|l| matches!(self.ty(l), Ty::Heap(_))) { call(rt("Heap"), vec![list]) } else { list }
            }
            Expr::ListGet { list, index, .. } => {
                let (list, index) = (self.operand(list), self.operand(index));
                subscript(list, index)
            }
            Expr::TupleNew { items, .. } => {
                let items: Vec<Value> = items.iter().map(|o| self.stored(o, false)).collect();
                tuple(items)
            }
            Expr::TupleGet { tuple: t, index } => {
                let t = self.operand(t);
                subscript(t, int(*index as i128))
            }
            Expr::Construct { ty, variant, fields, .. } => {
                let fields: Vec<Value> = fields.iter().map(|o| self.stored(o, false)).collect();
                call(name(&self.m.class_name(ty, *variant)), fields)
            }
            Expr::UnitVariant { ty, variant } => name(&self.m.class_name(ty, Some(*variant))),
            Expr::Field { value, ty, variant, index } => {
                let v = self.operand(value);
                let names = self.m.field_names(ty, *variant);
                attr(v, names.get(*index).map_or("", String::as_str))
            }
            Expr::Tag(v) => {
                let v = self.operand(v);
                attr(v, "_lotml_tag")
            }
            Expr::OptNew { value, .. } => match value {
                Some(v) => self.stored(v, false),
                None => none(),
            },
            Expr::OptIsSome(v) => {
                let v = self.operand(v);
                compare(v, "IsNot", none())
            }
            Expr::OptValue(v) => self.operand(v),
            Expr::OptIf { cond, value, .. } => {
                let (c, v) = (self.operand(cond), self.stored(value, false));
                node("IfExp", vec![("test", c), ("body", v), ("orelse", none())])
            }
            Expr::ResultNew { ok, value, .. } => {
                let v = self.stored(value, false);
                call(rt(if *ok { "Ok" } else { "Err" }), vec![v])
            }
            Expr::ResultIsOk(v) => {
                let v = self.operand(v);
                call(builtin("isinstance"), vec![v, rt("Ok")])
            }
            Expr::ResultValue(v) => {
                let v = self.operand(v);
                attr(v, "value")
            }
            Expr::ResultError(v) => {
                let v = self.operand(v);
                attr(v, "error")
            }
            Expr::ReadPlace(place) => self.place_value(place).0,
            Expr::DictNew { items, .. } => {
                let mut keys = Vec::new();
                let mut values = Vec::new();
                for (k, v) in items {
                    keys.push(self.operand(k));
                    values.push(self.stored(v, false));
                }
                node("Dict", vec![("keys", Value::Array(keys)), ("values", Value::Array(values))])
            }
            Expr::SetNew { items, folded, .. } => {
                let items: Vec<Value> = items.iter().map(|o| self.operand(o)).collect();
                if *folded {
                    return node("Set", vec![("elts", Value::Array(items))]);
                }
                let list = node("List", vec![("elts", Value::Array(items)), ("ctx", load())]);
                call(builtin("set"), vec![list])
            }
            Expr::Closure { lambda, captures, .. } => {
                let captured: Vec<Value> = captures.iter().map(|o| self.stored(o, false)).collect();
                call(rt("closure"), vec![name(&format!("__lambda_{lambda}")), tuple(captured)])
            }
            Expr::FnRef(f) => name(&py_name(f)),
            Expr::FnRefGeneric { name: f, .. } => name(f),
            Expr::CallC { symbol, args, .. } => {
                let args: Vec<Value> = args.iter().map(|o| self.operand(o)).collect();
                call(name(&format!("__c_{symbol}")), args)
            }
            Expr::CallPython { module, function, args, ret, .. } => {
                let k = match self.m.python_calls.iter().position(|(m, f, _)| m == module && f == function) {
                    Some(k) => k,
                    None => {
                        self.m.python_calls.push((module.clone(), function.clone(), ret.clone()));
                        self.m.python_calls.len() - 1
                    }
                };
                let args: Vec<Value> = args.iter().map(|o| self.stored(o, false)).collect();
                call(name(&format!("__py{k}")), args)
            }
            Expr::Parallel { tasks, .. } => {
                let tasks = self.operand(tasks);
                call(rt("parallel"), vec![tasks])
            }
            Expr::ToDyn { value, .. } | Expr::ToDynOf { value, .. } => self.operand(value),
            Expr::CallDyn { receiver, ty, slot, args, .. } => {
                let Ty::Dyn(trait_name) = ty else { return none() };
                let m = self.m.lowered.dyn_methods.get(trait_name).and_then(|ms| ms.get(*slot).cloned().flatten());
                let receiver = self.operand(receiver);
                let args: Vec<Value> = args.iter().map(|a| self.stored(a, false)).collect();
                method(receiver, &m.unwrap_or_default(), args)
            }
            Expr::Method { place, ty, method: m, args, keywords } => {
                let (receiver, _) = self.place_value(place);
                let into_var = self.mutated[place.local];
                let args: Vec<Value> = args.iter().map(|a| self.stored(a, into_var)).collect();
                let keywords: Vec<(&str, Value)> =
                    keywords.iter().map(|(k, a)| (k.as_str(), self.operand(a))).collect();
                match (ty, m.as_str()) {
                    (Ty::Str, "format") => {
                        call_kw(rt("str_format"), std::iter::once(receiver).chain(args).collect(), keywords)
                    }
                    _ => call_kw(attr(receiver, m), args, keywords),
                }
            }
            Expr::CallClosure { callee, args, .. } => {
                let callee = self.operand(callee);
                let args: Vec<Value> = args.iter().map(|a| self.stored(a, false)).collect();
                call(callee, args)
            }
            Expr::Capture { closure, index, .. } => {
                let c = self.operand(closure);
                subscript(c, int(*index as i128))
            }
        }
    }

    fn binary(&mut self, o: BinOp, a: Value, b: Value, ty: &Ty) -> Value {
        let kind = match o {
            BinOp::Add => "Add",
            BinOp::Sub => "Sub",
            BinOp::Mul => "Mult",
            BinOp::TrueDiv => "Div",
            BinOp::FloorDiv => "FloorDiv",
            BinOp::Mod => "Mod",
            BinOp::Pow => return call(rt("power"), vec![a, b]),
            BinOp::Shl if matches!(ty, Ty::Int(_)) => return call(rt("lshift"), vec![a, b]),
            BinOp::Shl => "LShift",
            BinOp::Shr => "RShift",
            BinOp::BitAnd => "BitAnd",
            BinOp::BitOr => "BitOr",
            BinOp::BitXor => "BitXor",
        };
        binop(a, kind, b)
    }

    /// A call of `function` with `args`: an `inout` one boxed, and taken back out of its box once
    /// the call returns.
    fn call_slots(&mut self, function: Value, args: &[Arg], out: &mut Vec<Value>) -> Value {
        let mut values = Vec::new();
        let mut writebacks = Vec::new();
        for a in args {
            match a {
                Arg::Slot(place) => {
                    let (current, _) = self.place_value(place);
                    let b = self.temporary();
                    out.push(self.at(assign(vec![target(&b)], call(rt("Box"), vec![current]))));
                    values.push(name(&b));
                    let mut reach = Vec::new();
                    let place_target = self.place_target(place, &mut reach);
                    writebacks.push((reach, place_target, b));
                }
                Arg::Value(o) => values.push(self.stored(o, false)),
                _ => {}
            }
        }
        if writebacks.is_empty() {
            return call(function, values);
        }
        let result = self.temporary();
        out.push(self.at(assign(vec![target(&result)], self.at_expr(call(function, values)))));
        for (reach, place_target, b) in writebacks {
            out.extend(reach);
            out.push(self.at(assign(vec![place_target], attr(name(&b), "value"))));
        }
        name(&result)
    }

    fn format(&mut self, parts: &[FormatPart]) -> Value {
        let values: Vec<Value> = parts.iter().map(|p| self.format_part(p)).collect();
        node("JoinedStr", vec![("values", Value::Array(values))])
    }

    fn format_part(&mut self, part: &FormatPart) -> Value {
        match part {
            FormatPart::Text(t) => text(t),
            FormatPart::Value { value, conversion, spec, .. } => {
                let value = self.operand(value);
                let conversion = match conversion {
                    Some('s') => 115,
                    Some('r') => 114,
                    Some('a') => 97,
                    _ => -1,
                };
                let spec = if spec.is_empty() { Value::Null } else { self.format(spec) };
                node("FormattedValue", vec![("value", value), ("conversion", conversion.into()), ("format_spec", spec)])
            }
        }
    }

    // Built-in operations -------------------------------------------------------------------

    /// The operand of each of `args`, read, by its position: a descriptor, an offset or an output,
    /// which only a native runtime takes, as `None`.
    fn values(&mut self, args: &[Arg]) -> Vec<Value> {
        let mut out = Vec::new();
        for a in args {
            out.push(match a {
                Arg::Value(o) | Arg::Address(o, _) => self.operand(o),
                Arg::Slot(p) => self.place_value(p).0,
                Arg::Out(..) | Arg::Desc(_) | Arg::Offset(_) => none(),
            });
        }
        out
    }

    /// The value a built-in operation computes: with outputs, a tuple of its flag and them.
    fn builtin(&mut self, op: Builtin, args: &[Arg]) -> Value {
        let v = self.values(args);
        let flag = |i: usize| -> bool { matches!(args.get(i), Some(Arg::Value(Operand::Const(Const::Bool(true))))) };
        let a = |i: usize| v.get(i).cloned().unwrap_or_else(none);
        let math = |f: &str| call(attr(rt("math"), f), v.clone());
        match op {
            // The text, the operator, each side and the message, after their descriptors.
            Builtin::AssertCompared => call(rt("assertion_failed"), vec![a(0), a(1), a(3), a(5), a(7)]),
            Builtin::TestError => call(rt("fail"), vec![a(1)]),
            Builtin::Comb => math("comb"),
            Builtin::Perm => math("perm"),
            Builtin::Factorial => math("factorial"),
            Builtin::Gcd => math("gcd"),
            Builtin::Isqrt => math("isqrt"),
            Builtin::PowMod => call(rt("_pow"), vec![a(0), a(1), a(2)]),
            Builtin::MathAtan => math("atan"),
            Builtin::MathAtan2 => math("atan2"),
            Builtin::MathCeil => math("ceil"),
            Builtin::MathCos => math("cos"),
            Builtin::MathExp => math("exp"),
            Builtin::MathFabs => math("fabs"),
            Builtin::MathFloor => math("floor"),
            Builtin::MathHypot => math("hypot"),
            Builtin::MathLn => math("log"),
            Builtin::MathLog10 => math("log10"),
            Builtin::MathLog2 => math("log2"),
            Builtin::MathPow => math("pow"),
            Builtin::MathSin => math("sin"),
            Builtin::MathSqrt => math("sqrt"),
            Builtin::MathTan => math("tan"),
            Builtin::MathTrunc => math("trunc"),
            Builtin::RoundF64 => call(rt("_round"), vec![a(0), a(1)]),
            Builtin::RoundI64 => call(rt("_round"), vec![a(0)]),
            Builtin::HashValue => call(builtin("hash"), vec![a(1)]),
            Builtin::WrappingAdd => call(rt("wrapping_add"), vec![a(0), a(1)]),
            Builtin::WrappingSub => call(rt("wrapping_sub"), vec![a(0), a(1)]),
            Builtin::WrappingMul => call(rt("wrapping_mul"), vec![a(0), a(1)]),
            Builtin::SumF64 | Builtin::SumI64 => call(rt("_sum"), vec![a(0)]),
            Builtin::SumU64 => call(subscript(rt("SIZED"), text("u64")), vec![call(builtin("sum"), vec![a(0)])]),
            Builtin::RangeList => call(builtin("list"), vec![call(builtin("range"), vec![a(0), a(1), a(2)])]),
            Builtin::DictClear | Builtin::ListClear => method(a(0), "clear", vec![]),
            Builtin::DictContains | Builtin::ListContains | Builtin::SetContains => compare(a(1), "In", a(0)),
            Builtin::DictCopy => call(builtin("dict"), vec![a(0)]),
            Builtin::DictFromPairs => call(builtin("dict"), vec![a(2)]),
            Builtin::DictGet => subscript(a(0), a(1)),
            Builtin::DictGetOptional => call(rt("lookup"), vec![a(0), a(1)]),
            Builtin::DictGetOr => method(a(0), "get", vec![a(1), a(2)]),
            Builtin::DictItems => call(builtin("list"), vec![method(a(0), "items", vec![])]),
            Builtin::DictKeys => call(builtin("list"), vec![a(0)]),
            Builtin::DictValues => call(builtin("list"), vec![method(a(0), "values", vec![])]),
            Builtin::DictPop => call(rt("popped"), vec![a(0), a(1)]),
            Builtin::DictSet => none(),
            Builtin::HeapPeek => call(rt("out"), vec![method(a(0), "peek", vec![])]),
            Builtin::HeapPop => call(rt("out"), vec![method(a(0), "pop_min", vec![])]),
            Builtin::HeapPush => method(a(0), "push", vec![a(1)]),
            Builtin::Heapify => call(rt("Heap"), vec![a(0)]),
            Builtin::ListAll => call(builtin("all"), vec![a(0)]),
            Builtin::ListAny => call(builtin("any"), vec![a(0)]),
            Builtin::ListConcat | Builtin::StrConcat => binop(a(0), "Add", a(1)),
            Builtin::ListCopy => call(builtin("list"), vec![a(0)]),
            Builtin::ListCount | Builtin::StrCount => method(a(0), "count", vec![a(1)]),
            Builtin::ListExtend => method(a(0), "extend", vec![a(1)]),
            Builtin::ListExtreme => call(builtin(if flag(1) { "max" } else { "min" }), vec![a(0)]),
            Builtin::ListIndex => call(rt("out"), vec![call(rt("list_index"), vec![a(0), a(1)])]),
            Builtin::ListInsert => method(a(0), "insert", vec![a(1), a(2)]),
            Builtin::ListLast => call(rt("out"), vec![call(rt("list_last"), vec![a(0)])]),
            Builtin::ListPop => {
                let popped =
                    if flag(1) { call(rt("list_pop"), vec![a(0), a(2)]) } else { call(rt("list_pop"), vec![a(0)]) };
                call(rt("out"), vec![popped])
            }
            Builtin::ListPush => method(a(0), "append", vec![a(1)]),
            Builtin::ListRemove => method(a(0), "remove", vec![a(1)]),
            Builtin::ListRepeat | Builtin::StrRepeat => binop(a(0), "Mult", a(1)),
            Builtin::ListReverse => method(a(0), "reverse", vec![]),
            Builtin::ListReversed => subscript(a(0), slice(none(), none(), int(-1))),
            Builtin::ListSlice | Builtin::StrSlice => {
                let bound = |i: usize, value: Value| if flag(i) { value } else { none() };
                subscript(a(0), slice(bound(1, a(2)), bound(3, a(4)), bound(5, a(6))))
            }
            Builtin::ListSort => call_kw(attr(a(0), "sort"), vec![], vec![("reverse", a(1))]),
            Builtin::ListSortByKeys => call(rt("sort_by_keys"), vec![a(0), a(1), a(2)]),
            Builtin::ListSorted => call_kw(builtin("sorted"), vec![a(0)], vec![("reverse", a(1))]),
            Builtin::ListUnique => none(),
            Builtin::ListUnpack => call(rt("unpack"), vec![a(0), a(1)]),
            Builtin::SetAdd => method(a(0), "add", vec![a(1)]),
            Builtin::SetCopy => call(builtin("set"), vec![a(0)]),
            Builtin::SetFromList => call(builtin("set"), vec![a(1)]),
            Builtin::SetDifference => binop(a(0), "Sub", a(1)),
            Builtin::SetDiscard => method(a(0), "discard", vec![a(1)]),
            Builtin::SetIntersection => binop(a(0), "BitAnd", a(1)),
            Builtin::SetIssubset => compare(a(0), "LtE", a(1)),
            Builtin::SetList => call(builtin("list"), vec![a(0)]),
            Builtin::SetPop => call(rt("out"), vec![call(rt("set_pop"), vec![a(0)])]),
            Builtin::SetRemove => method(a(0), "remove", vec![a(1)]),
            Builtin::SetUnion => binop(a(0), "BitOr", a(1)),
            Builtin::StrCapitalize => method(a(0), "capitalize", vec![]),
            Builtin::StrChars => call(builtin("list"), vec![a(0)]),
            Builtin::StrChr => call(builtin("chr"), vec![a(0)]),
            Builtin::StrEndswith => method(a(0), "endswith", vec![a(1)]),
            Builtin::StrFind => {
                let find = if flag(2) { "str_rfind" } else { "str_find" };
                call(rt("out"), vec![call(rt(find), vec![a(0), a(1)])])
            }
            Builtin::StrFloat => call(builtin("float"), vec![a(0)]),
            Builtin::StrIndex => subscript(a(0), a(1)),
            Builtin::StrInt => call(rt("_int"), vec![a(0)]),
            Builtin::StrIsalnum => method(a(0), "isalnum", vec![]),
            Builtin::StrIsalpha => method(a(0), "isalpha", vec![]),
            Builtin::StrIsdigit => method(a(0), "isdigit", vec![]),
            Builtin::StrIslower => method(a(0), "islower", vec![]),
            Builtin::StrIsspace => method(a(0), "isspace", vec![]),
            Builtin::StrIsupper => method(a(0), "isupper", vec![]),
            Builtin::StrJoin => method(a(0), "join", vec![a(1)]),
            Builtin::StrLower => method(a(0), "lower", vec![]),
            Builtin::StrOrd => call(builtin("ord"), vec![a(0)]),
            Builtin::StrPad => {
                let align = match args.get(3) {
                    Some(Arg::Value(Operand::Const(Const::Char('^')))) => "center",
                    Some(Arg::Value(Operand::Const(Const::Char('<')))) => "ljust",
                    _ => "rjust",
                };
                let fill = match args.get(2) {
                    Some(Arg::Value(Operand::Const(Const::Null))) | None => text(" "),
                    _ => a(2),
                };
                method(a(0), align, vec![a(1), fill])
            }
            Builtin::StrPartitionPart => subscript(method(a(0), "partition", vec![a(1)]), a(2)),
            Builtin::StrReplace => method(a(0), "replace", vec![a(1), a(2), a(3)]),
            Builtin::StrSplit => method(a(0), "split", vec![a(1), a(2)]),
            Builtin::StrSplitOnce => call(rt("split_pair"), vec![a(0), a(1)]),
            Builtin::StrSplitlines => method(a(0), "splitlines", vec![]),
            Builtin::StrStartswith => method(a(0), "startswith", vec![a(1)]),
            Builtin::StrStrip => {
                let which = match (flag(2), flag(3)) {
                    (true, true) => "strip",
                    (true, false) => "lstrip",
                    _ => "rstrip",
                };
                method(a(0), which, vec![a(1)])
            }
            Builtin::StrSwapcase => method(a(0), "swapcase", vec![]),
            Builtin::StrTitle => method(a(0), "title", vec![]),
            Builtin::StrToFloat => call(rt("out"), vec![call(rt("to_float"), vec![a(0)])]),
            Builtin::StrToInt => call(rt("out"), vec![call(rt("to_int"), vec![a(0)])]),
            Builtin::StrUpper => method(a(0), "upper", vec![]),
            Builtin::StrZfill => method(a(0), "zfill", vec![a(1)]),
        }
    }

    /// A built-in operation changing the value at `place`: its result, when it has one, and the
    /// outputs it sets, given to their locals.
    fn mutate(&mut self, which: Builtin, place: &Place, args: &[Arg], result: Option<Local>) -> Vec<Value> {
        let mut out = Vec::new();
        let into_var = self.mutated[place.local];
        let (receiver, _) = self.place_value(place);
        let mut v = vec![receiver.clone()];
        for a in args {
            match a {
                Arg::Value(o) | Arg::Address(o, _) => {
                    let storing = matches!(
                        which,
                        Builtin::ListPush
                            | Builtin::ListInsert
                            | Builtin::ListExtend
                            | Builtin::DictSet
                            | Builtin::SetAdd
                            | Builtin::HeapPush
                    );
                    v.push(if storing { self.stored(o, into_var) } else { self.operand(o) });
                }
                Arg::Slot(p) => v.push(self.place_value(p).0),
                Arg::Out(..) | Arg::Desc(_) | Arg::Offset(_) => v.push(none()),
            }
        }
        let a = |i: usize| v.get(i).cloned().unwrap_or_else(none);
        match which {
            Builtin::DictSet => {
                let target = stored_ctx(subscript(receiver, a(1)));
                out.push(self.at(assign(vec![target], a(2))));
                return out;
            }
            Builtin::Heapify => {
                let mut reach = Vec::new();
                let target = self.place_target(place, &mut reach);
                out.extend(reach);
                out.push(self.at(assign(vec![target], call(rt("Heap"), vec![receiver]))));
                return out;
            }
            Builtin::ListUnique => return out,
            _ => {}
        }
        let shifted: Vec<Arg> =
            std::iter::once(Arg::Value(Operand::Const(Const::Unit))).chain(args.iter().cloned()).collect();
        let value = self.mutation(which, &v, &shifted);
        let outs: Vec<Local> = lotml_ir::ir::outs(args).collect();
        let value = self.at_expr(value);
        match result {
            Some(r) => {
                let mut targets = vec![self.local_target(r)];
                targets.extend(outs.iter().map(|&o| self.local_target(o)));
                let targets = if targets.len() == 1 {
                    targets
                } else {
                    vec![node("Tuple", vec![("elts", Value::Array(targets)), ("ctx", op("Store"))])]
                };
                out.push(self.at(assign(targets, value)));
            }
            None => out.push(self.at(stmt_expr(value))),
        }
        out
    }

    /// The expression of a changing built-in operation whose receiver and arguments are `v`.
    fn mutation(&self, op: Builtin, v: &[Value], args: &[Arg]) -> Value {
        let a = |i: usize| v.get(i).cloned().unwrap_or_else(none);
        let flag = |i: usize| matches!(args.get(i), Some(Arg::Value(Operand::Const(Const::Bool(true)))));
        match op {
            Builtin::ListPush => method(a(0), "append", vec![a(1)]),
            Builtin::ListExtend => method(a(0), "extend", vec![a(1)]),
            Builtin::ListInsert => method(a(0), "insert", vec![a(1), a(2)]),
            Builtin::ListRemove => method(a(0), "remove", vec![a(1)]),
            Builtin::ListClear | Builtin::DictClear => method(a(0), "clear", vec![]),
            Builtin::ListReverse => method(a(0), "reverse", vec![]),
            Builtin::ListSort => call_kw(attr(a(0), "sort"), vec![], vec![("reverse", a(1))]),
            Builtin::ListSortByKeys => call(rt("sort_by_keys"), vec![a(0), a(1), a(2)]),
            Builtin::ListPop => {
                let popped =
                    if flag(1) { call(rt("list_pop"), vec![a(0), a(2)]) } else { call(rt("list_pop"), vec![a(0)]) };
                call(rt("out"), vec![popped])
            }
            Builtin::DictPop => call(rt("popped"), vec![a(0), a(1)]),
            Builtin::SetAdd => method(a(0), "add", vec![a(1)]),
            Builtin::SetRemove => method(a(0), "remove", vec![a(1)]),
            Builtin::SetDiscard => method(a(0), "discard", vec![a(1)]),
            Builtin::SetPop => call(rt("out"), vec![call(rt("set_pop"), vec![a(0)])]),
            Builtin::HeapPush => method(a(0), "push", vec![a(1)]),
            Builtin::HeapPop => call(rt("out"), vec![method(a(0), "pop_min", vec![])]),
            _ => none(),
        }
    }
}

fn slice(lower: Value, upper: Value, step: Value) -> Value {
    node("Slice", vec![("lower", lower), ("upper", upper), ("step", step)])
}

fn constant_of(c: &Const) -> Value {
    match c {
        Const::Int(v, _) => int(*v),
        Const::Float(v) => constant(json!({"_float": format!("{v:?}")})),
        Const::Bool(b) => constant((*b).into()),
        Const::Unit | Const::Null => none(),
        Const::Str(s) => text(s),
        Const::Bytes(b) => constant(json!({ "_bytes": b })),
        Const::Char(c) => text(&c.to_string()),
    }
}

/// Whether the value `e` gives may be one an operand of it holds, rather than one it builds: a
/// value read, an element, a field, the result of a built-in operation.
fn aliasing(e: &Expr) -> bool {
    matches!(
        e,
        Expr::Use(_)
            | Expr::ListGet { .. }
            | Expr::Field { .. }
            | Expr::TupleGet { .. }
            | Expr::OptValue(_)
            | Expr::OptIf { .. }
            | Expr::ResultValue(_)
            | Expr::ResultError(_)
            | Expr::ReadPlace(_)
            | Expr::RtValue { .. }
            | Expr::Rt { .. }
    )
}

/// The roots of the places a call lends as `inout`.
fn slots(e: &Expr, roots: &mut Vec<Local>) {
    if let Expr::CallSlots(_, args) | Expr::CallGeneric { args, .. } = e {
        for a in args {
            if let Arg::Slot(p) = a {
                roots.push(p.local);
            }
        }
    }
}

/// The root of the place a built-in method changing its receiver is called on.
fn changes(e: &Expr, roots: &mut Vec<Local>) {
    const CHANGING: &[&str] = &[
        "update",
        "setdefault",
        "append",
        "extend",
        "insert",
        "remove",
        "clear",
        "sort",
        "reverse",
        "pop",
        "add",
        "discard",
        "push",
        "pop_min",
    ];
    if let Expr::Method { place, method, .. } = e
        && CHANGING.contains(&method.as_str())
    {
        roots.push(place.local);
    }
}

/// Every statement of `block`, nested ones included, in order.
fn walk<'b>(block: &'b Block, f: &mut impl FnMut(&'b Stmt)) {
    for stmt in block {
        f(stmt);
        match &stmt.kind {
            StmtKind::If(_, then, otherwise) => {
                walk(then, f);
                walk(otherwise, f);
            }
            StmtKind::Loop(body) => walk(body, f),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                walk(body, f);
                walk(exit, f);
            }
            _ => {}
        }
    }
}
