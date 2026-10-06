//! lotml's syntax tree to Python's, as JSON the runtime turns back into `ast` nodes.

use std::collections::HashSet;

use lotml_check::Types;
use lotml_check::ty::{IntKind, Ty};
use lotml_syntax::ast::*;
use lotml_syntax::span::Span;
use serde_json::{Map, Value, json};

/// The Python module for a checked lotml module.
pub fn module(text: &str, module: &Module, types: &Types) -> Value {
    let mut emitter = Emitter::new(text, module, types);
    emitter.module(module)
}

/// Prelude functions whose result may hold their arguments' values: a value read out of a
/// `var` through them still belongs to the `var`.
const ALIASING: &[&str] =
    &["sorted", "list", "reversed", "zip", "enumerate", "max", "min", "filter", "map", "set", "dict", "Heap"];

/// Methods of built-in types that store their argument in the receiver.
const STORING: &[&str] = &["append", "extend", "insert", "add", "update", "setdefault", "push"];

/// Methods of built-in types that return a value still held by the receiver.
const ELEMENT: &[&str] = &["get", "find", "last", "peek", "setdefault"];

#[derive(Default, Clone)]
struct Scope {
    /// `var` locals and `var` parameters: owned, changed in place.
    mutable: HashSet<String>,
    /// `inout` parameters, read and written through a box.
    boxed: HashSet<String>,
    /// Loop and pattern variables bound to a value still held by a `var`.
    tainted: HashSet<String>,
    fallible: bool,
}

struct Emitter<'a> {
    text: &'a str,
    lines: Vec<u32>,
    types: &'a Types,
    /// Variants without fields, each one value.
    units: HashSet<String>,
    /// Sum types whose variants all have no fields: their values never change.
    enums: HashSet<String>,
    /// Names of top-level functions.
    functions: HashSet<String>,
    scope: Scope,
    temporaries: usize,
    /// Inside a comprehension's iterable or a default value, where `:=` is not allowed.
    no_walrus: bool,
}

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

fn store() -> Value {
    op("Store")
}

fn name(id: &str) -> Value {
    node("Name", vec![("id", id.into()), ("ctx", load())])
}

fn target(id: &str) -> Value {
    node("Name", vec![("id", id.into()), ("ctx", store())])
}

fn attr(value: Value, attr: &str) -> Value {
    node("Attribute", vec![("value", value), ("attr", attr.into()), ("ctx", load())])
}

fn rt(function: &str) -> Value {
    attr(name("__rt"), function)
}

fn call(function: Value, args: Vec<Value>) -> Value {
    node("Call", vec![("func", function), ("args", Value::Array(args)), ("keywords", json!([]))])
}

fn constant(value: Value) -> Value {
    node("Constant", vec![("value", value), ("kind", Value::Null)])
}

fn int(value: i128) -> Value {
    constant(json!({"_int": value.to_string()}))
}

fn stmt_expr(value: Value) -> Value {
    node("Expr", vec![("value", value)])
}

fn assign(targets: Vec<Value>, value: Value) -> Value {
    node("Assign", vec![("targets", Value::Array(targets)), ("value", value), ("type_comment", Value::Null)])
}

fn arguments(args: Vec<Value>, defaults: Vec<Value>, kwonly: Vec<Value>, kw_defaults: Vec<Value>) -> Value {
    node(
        "arguments",
        vec![
            ("posonlyargs", json!([])),
            ("args", Value::Array(args)),
            ("vararg", Value::Null),
            ("kwonlyargs", Value::Array(kwonly)),
            ("kw_defaults", Value::Array(kw_defaults)),
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
            ("body", Value::Array(body)),
            ("decorator_list", json!([])),
            ("returns", Value::Null),
            ("type_comment", Value::Null),
            ("type_params", json!([])),
        ],
    )
}

fn lambda(body: Value) -> Value {
    node("Lambda", vec![("args", arguments(vec![], vec![], vec![], vec![])), ("body", body)])
}

/// The context of a target node turned to `Store`.
fn stored_ctx(mut target: Value) -> Value {
    if let Some(map) = target.as_object_mut() {
        if map.contains_key("ctx") {
            map.insert("ctx".into(), store());
        }
        if let Some(Value::Array(elements)) = map.get_mut("elts") {
            for element in elements.iter_mut() {
                *element = stored_ctx(element.take());
            }
        }
    }
    target
}

fn range_of(kind: IntKind) -> (i128, i128) {
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

impl<'a> Emitter<'a> {
    fn new(text: &'a str, module: &Module, types: &'a Types) -> Emitter<'a> {
        let mut lines = vec![0];
        lines.extend(text.match_indices('\n').map(|(i, _)| i as u32 + 1));
        let mut units = HashSet::new();
        let mut enums = HashSet::new();
        let mut functions = HashSet::new();
        for item in &module.items {
            match item {
                Item::Sum(s) => {
                    for v in &s.variants {
                        if v.fields.is_none() {
                            units.insert(v.name.name.clone());
                        }
                    }
                    if s.variants.iter().all(|v| v.fields.is_none()) {
                        enums.insert(s.name.name.clone());
                    }
                }
                Item::Fn(f) => {
                    functions.insert(f.name.name.clone());
                }
                _ => {}
            }
        }
        Emitter {
            text,
            lines,
            types,
            units,
            enums,
            functions,
            scope: Scope::default(),
            temporaries: 0,
            no_walrus: false,
        }
    }

    // Positions ---------------------------------------------------------------------------

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

    fn temporary(&mut self, prefix: &str) -> String {
        self.temporaries += 1;
        format!("__{prefix}{}", self.temporaries)
    }

    // Types ---------------------------------------------------------------------------------

    fn ty(&self, e: &Expr) -> Option<&Ty> {
        self.types.get(&e.span)
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

    fn changeable_expr(&self, e: &Expr) -> bool {
        self.ty(e).is_none_or(|t| self.changeable(t))
    }

    /// Whether the value of `e` is still held by a `var` or an `inout`, so binding or storing
    /// it elsewhere would share it.
    fn rooted(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::Name(n) => {
                self.scope.mutable.contains(n) || self.scope.boxed.contains(n) || self.scope.tainted.contains(n)
            }
            ExprKind::Attr { object, .. } | ExprKind::Index { object, .. } | ExprKind::Slice { object, .. } => {
                self.rooted(object)
            }
            ExprKind::IfExp { then, orelse, .. } => self.rooted(then) || self.rooted(orelse),
            ExprKind::Coalesce { value, default } => self.rooted(value) || self.rooted(default),
            ExprKind::Try(inner) => self.rooted(inner),
            ExprKind::Call { func, args } => {
                if args.iter().any(|a| matches!(a, Arg::Inout(..))) {
                    return true;
                }
                match &func.kind {
                    ExprKind::Attr { object, name } => {
                        let builtin = matches!(
                            self.ty(object),
                            Some(Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_) | Ty::Str)
                        );
                        self.rooted(object) && (!builtin || ELEMENT.contains(&name.name.as_str()))
                    }
                    ExprKind::Name(n) if ALIASING.contains(&n.as_str()) && !self.functions.contains(n) => {
                        args.iter().any(|a| self.rooted(a.expr()))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Whether `e` builds a new value no one else holds: a literal or a comprehension of
    /// values that cannot change, or of new values.
    fn fresh(&self, e: &Expr) -> bool {
        let part = |x: &Expr| !self.changeable_expr(x) || self.fresh(x);
        match &e.kind {
            ExprKind::List(items) | ExprKind::Set(items) | ExprKind::Tuple(items) => items.iter().all(part),
            ExprKind::Dict(pairs) => pairs.iter().all(|(_, v)| part(v)),
            ExprKind::ListComp { element, .. } | ExprKind::SetComp { element, .. } => part(element),
            ExprKind::DictComp { value, .. } => part(value),
            ExprKind::Call { func, args } => match &func.kind {
                ExprKind::Name(n) if !self.functions.contains(n) && ALIASING.contains(&n.as_str()) => {
                    self.ty(e).is_some_and(|t| self.shallow(t))
                }
                ExprKind::Attr { object, name } => {
                    matches!(self.ty(object), Some(Ty::Str))
                        || (matches!(name.name.as_str(), "keys" | "values" | "items" | "copy")
                            && self.ty(e).is_some_and(|t| self.shallow(t)))
                }
                ExprKind::Name(_) => args.iter().all(|a| part(a.expr())) && !self.functions.contains_key_fn(func),
                _ => false,
            },
            _ => !self.changeable_expr(e),
        }
    }

    fn copied(&self, value: Value, e: &Expr) -> Value {
        let function = if self.ty(e).is_some_and(|t| self.shallow(t)) { "shallow" } else { "copy" };
        self.at(call(rt(function), vec![value]), e.span)
    }

    /// A value entering a `var`: copied unless it is new.
    fn entering_var(&mut self, e: &Expr) -> Value {
        let value = self.expr(e);
        if self.changeable_expr(e) && !self.fresh(e) { self.copied(value, e) } else { value }
    }

    /// A value bound or stored anywhere: copied if a `var` still holds it.
    fn stored(&mut self, e: &Expr) -> Value {
        let value = self.expr(e);
        if self.changeable_expr(e) && self.rooted(e) { self.copied(value, e) } else { value }
    }

    // Overflow --------------------------------------------------------------------------------

    fn int_kind(&self, e: &Expr) -> Option<IntKind> {
        match self.ty(e) {
            Some(Ty::Int(kind)) => Some(*kind),
            _ => None,
        }
    }

    /// `value`, an integer result, trapping outside its type: inline, or through the runtime
    /// where `:=` is not allowed.
    fn checked(&mut self, value: Value, kind: IntKind, span: Span) -> Value {
        let (low, high) = range_of(kind);
        if self.no_walrus {
            return self.at(call(rt("check"), vec![value, int(low), int(high)]), span);
        }
        let t = self.temporary("o");
        let walrus = node("NamedExpr", vec![("target", target(&t)), ("value", value)]);
        let test = node(
            "Compare",
            vec![
                ("left", int(low)),
                ("ops", json!([op("LtE"), op("LtE")])),
                ("comparators", json!([walrus, int(high)])),
            ],
        );
        let trap = call(rt("overflow"), vec![name(&t)]);
        self.at(node("IfExp", vec![("test", test), ("body", name(&t)), ("orelse", trap)]), span)
    }

    /// The statement that traps when `place`, just assigned an integer, left its type.
    fn trap_after(&self, place: Value, kind: IntKind, span: Span) -> Value {
        let (low, high) = range_of(kind);
        let below = node(
            "Compare",
            vec![("left", place.clone()), ("ops", json!([op("Lt")])), ("comparators", json!([int(low)]))],
        );
        let above = node(
            "Compare",
            vec![("left", place.clone()), ("ops", json!([op("Gt")])), ("comparators", json!([int(high)]))],
        );
        let test = node("BoolOp", vec![("op", op("Or")), ("values", json!([below, above]))]);
        let body = stmt_expr(call(rt("overflow"), vec![place]));
        self.at(node("If", vec![("test", test), ("body", json!([body])), ("orelse", json!([]))]), span)
    }

    // Items -----------------------------------------------------------------------------------

    fn module(&mut self, module: &Module) -> Value {
        let mut imports = Vec::new();
        let mut declarations = Vec::new();
        let mut traits = Vec::new();
        let mut functions = Vec::new();
        let mut impls = Vec::new();
        let mut tests = Vec::new();
        let mut defaults: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for item in &module.items {
            if let Item::Trait(t) = item {
                for m in t.methods.iter().filter(|m| m.body.is_some()) {
                    let id = format!("__{}_{}", t.name.name, m.name.name);
                    traits.push(self.function(m, &id));
                    defaults.entry(t.name.name.clone()).or_default().push(m.name.name.clone());
                }
            }
        }
        let mut count = 0;
        for item in &module.items {
            match item {
                Item::Import(import) => imports.extend(self.import(import)),
                Item::Record(r) => {
                    let fields: Vec<(String, Option<&Expr>)> = r
                        .fields
                        .iter()
                        .map(|f| (f.name.as_ref().map_or(String::new(), |n| n.name.clone()), f.default.as_ref()))
                        .collect();
                    declarations.push(self.record(&r.name.name, &fields, r.span));
                }
                Item::Sum(s) => {
                    for v in &s.variants {
                        match &v.fields {
                            Some(fields) => {
                                let fields: Vec<(String, Option<&Expr>)> = fields
                                    .iter()
                                    .enumerate()
                                    .map(|(i, f)| {
                                        (
                                            f.name.as_ref().map_or(format!("_{i}"), |n| n.name.clone()),
                                            f.default.as_ref(),
                                        )
                                    })
                                    .collect();
                                declarations.push(self.record(&v.name.name, &fields, v.span));
                            }
                            None => {
                                let unit = assign(
                                    vec![target(&v.name.name)],
                                    call(rt("Unit"), vec![constant(v.name.name.clone().into())]),
                                );
                                let register = assign(
                                    vec![node(
                                        "Attribute",
                                        vec![
                                            ("value", name("__variants")),
                                            ("attr", v.name.name.clone().into()),
                                            ("ctx", store()),
                                        ],
                                    )],
                                    name(&v.name.name),
                                );
                                declarations.push(self.at(unit, v.span));
                                declarations.push(self.at(register, v.span));
                            }
                        }
                    }
                }
                Item::Fn(f) => functions.push(self.function(f, &f.name.name)),
                Item::Impl(imp) => {
                    let TypeKind::Named { name: target_name, .. } = &imp.target.kind else { continue };
                    for m in &imp.methods {
                        let id = format!("__{}_{}", target_name.name, m.name.name);
                        impls.push(self.function(m, &id));
                        let is_method = m.params.first().is_some_and(|p| p.name.name == "self");
                        let attach = call(
                            rt("attach"),
                            vec![
                                name(&target_name.name),
                                constant(m.name.name.clone().into()),
                                name(&id),
                                constant(is_method.into()),
                            ],
                        );
                        impls.push(self.at(stmt_expr(attach), m.span));
                    }
                    if let Some(TypeExpr { kind: TypeKind::Named { name: trait_name, .. }, .. }) = &imp.trait_name
                        && let Some(methods) = defaults.get(&trait_name.name)
                    {
                        let keys: Vec<Value> = methods.iter().map(|m| constant(m.clone().into())).collect();
                        let values: Vec<Value> =
                            methods.iter().map(|m| name(&format!("__{}_{m}", trait_name.name))).collect();
                        let table = node("Dict", vec![("keys", Value::Array(keys)), ("values", Value::Array(values))]);
                        let attach = call(rt("attach_defaults"), vec![name(&target_name.name), table]);
                        impls.push(self.at(stmt_expr(attach), imp.span));
                    }
                }
                Item::Test(t) => {
                    count += 1;
                    let id = format!("__test_{count}");
                    self.scope = Scope::default();
                    collect_vars(&t.body, &mut self.scope.mutable);
                    let body = self.block(&t.body);
                    tests.push(self.at(function_def(&id, arguments(vec![], vec![], vec![], vec![]), body), t.span));
                    let entry = node(
                        "Tuple",
                        vec![("elts", json!([constant(t.name.clone().into()), name(&id)])), ("ctx", load())],
                    );
                    let register = call(attr(name("__tests"), "append"), vec![entry]);
                    tests.push(self.at(stmt_expr(register), t.span));
                }
                Item::Trait(_) | Item::Error(_) => {}
            }
        }
        let body: Vec<Value> =
            imports.into_iter().chain(declarations).chain(traits).chain(functions).chain(impls).chain(tests).collect();
        node("Module", vec![("body", Value::Array(body)), ("type_ignores", json!([]))])
    }

    fn import(&mut self, import: &Import) -> Vec<Value> {
        let module = rt("math");
        if import.names.is_empty() {
            let id = import.module.first().map_or("math", |m| m.name.as_str());
            return vec![self.at(assign(vec![target(id)], module), import.span)];
        }
        import
            .names
            .iter()
            .map(|n| self.at(assign(vec![target(&n.name)], attr(module.clone(), &n.name)), n.span))
            .collect()
    }

    fn record(&mut self, id: &str, fields: &[(String, Option<&Expr>)], span: Span) -> Value {
        let names: Vec<Value> = fields.iter().map(|(f, _)| constant(f.clone().into())).collect();
        let mut keys = Vec::new();
        let mut values = Vec::new();
        self.scope = Scope::default();
        for (field, default) in fields {
            if let Some(default) = default {
                keys.push(constant(field.clone().into()));
                let value = self.expr(default);
                values.push(self.at(lambda(value), default.span));
            }
        }
        let builder = call(
            rt("record"),
            vec![
                constant(id.into()),
                node("Tuple", vec![("elts", Value::Array(names)), ("ctx", load())]),
                node("Dict", vec![("keys", Value::Array(keys)), ("values", Value::Array(values))]),
            ],
        );
        self.at(assign(vec![target(id)], builder), span)
    }

    fn function(&mut self, f: &FnDef, id: &str) -> Value {
        let mut scope = Scope { fallible: f.error.is_some(), ..Scope::default() };
        let mut prologue = Vec::new();
        let mut args = Vec::new();
        let mut defaults = Vec::new();
        for p in &f.params {
            args.push(self.at(arg(&p.name.name), p.name.span));
            if let Some(default) = &p.default {
                let outer = std::mem::replace(&mut self.no_walrus, true);
                defaults.push(self.expr(default));
                self.no_walrus = outer;
            }
            match p.convention {
                Convention::Inout if p.name.name != "self" => {
                    scope.boxed.insert(p.name.name.clone());
                }
                Convention::Var => {
                    scope.mutable.insert(p.name.name.clone());
                    let copy = assign(vec![target(&p.name.name)], call(rt("copy"), vec![name(&p.name.name)]));
                    prologue.push(self.at(copy, p.span));
                }
                Convention::Inout => {
                    scope.mutable.insert(p.name.name.clone());
                }
                _ => {}
            }
        }
        if let Some(body) = &f.body {
            collect_vars(body, &mut scope.mutable);
        }
        let outer = std::mem::replace(&mut self.scope, scope);
        let mut body = prologue;
        body.extend(f.body.as_ref().map(|b| self.block(b)).unwrap_or_default());
        if self.scope.fallible {
            body.push(node("Return", vec![("value", call(name("Ok"), vec![constant(Value::Null)]))]));
            let failure = attr(name("__failure"), "error");
            let handler = node(
                "ExceptHandler",
                vec![
                    ("type", rt("Fail")),
                    ("name", "__failure".into()),
                    ("body", json!([node("Return", vec![("value", call(name("Err"), vec![failure]))])])),
                ],
            );
            let attempt = node(
                "Try",
                vec![
                    ("body", Value::Array(body)),
                    ("handlers", json!([handler])),
                    ("orelse", json!([])),
                    ("finalbody", json!([])),
                ],
            );
            body = vec![self.at(attempt, f.span)];
        }
        if body.is_empty() {
            body.push(node("Pass", vec![]));
        }
        self.scope = outer;
        self.at(function_def(id, arguments(args, defaults, vec![], vec![]), body), f.span)
    }

    // Statements ----------------------------------------------------------------------------

    fn block(&mut self, block: &Block) -> Vec<Value> {
        let mut out = Vec::new();
        for stmt in &block.stmts {
            out.extend(self.stmt(stmt));
        }
        if out.is_empty() {
            out.push(self.at(node("Pass", vec![]), block.span));
        }
        out
    }

    fn stmt(&mut self, stmt: &Stmt) -> Vec<Value> {
        let span = stmt.span;
        let out = match &stmt.kind {
            StmtKind::Expr(e) => vec![stmt_expr(self.expr(e))],
            StmtKind::Var { name, value, .. } => self.bind(&name.name, name.span, value, true),
            StmtKind::Annotated { target: t, value, .. } => {
                let into = self.scope.mutable.contains(&t.name);
                self.bind(&t.name, t.span, value, into)
            }
            StmtKind::Assign { target: place, value } => self.assign_stmt(place, value),
            StmtKind::AugAssign { target: place, op: o, value } => self.aug_assign(place, *o, value, span),
            StmtKind::Return(value) => {
                let mut v = value.as_ref().map_or(constant(Value::Null), |e| self.expr(e));
                if self.scope.fallible {
                    v = call(name("Ok"), vec![v]);
                }
                vec![node("Return", vec![("value", v)])]
            }
            StmtKind::Assert { test, message } => self.assert(test, message.as_ref()),
            StmtKind::Pass => vec![node("Pass", vec![])],
            StmtKind::Break => vec![node("Break", vec![])],
            StmtKind::Continue => vec![node("Continue", vec![])],
            StmtKind::If { branches, orelse } => {
                let mut tail = orelse.as_ref().map_or(Vec::new(), |b| self.block(b));
                for (i, (test, body)) in branches.iter().enumerate().rev() {
                    let test_value = self.expr(test);
                    let body_value = self.block(body);
                    let branch = node(
                        "If",
                        vec![("test", test_value), ("body", Value::Array(body_value)), ("orelse", Value::Array(tail))],
                    );
                    let branch_span = if i == 0 { span } else { Span { start: test.span.start, end: body.span.end } };
                    tail = vec![self.at(branch, branch_span)];
                }
                tail
            }
            StmtKind::While { test, body } => {
                let test = self.expr(test);
                let body = self.block(body);
                vec![node("While", vec![("test", test), ("body", Value::Array(body)), ("orelse", json!([]))])]
            }
            StmtKind::For { target: t, iter, body } => {
                let iterable = self.expr(iter);
                let saved = self.scope.tainted.clone();
                self.taint(t, iter);
                let loop_target = self.loop_target(t);
                let body = self.block(body);
                self.scope.tainted = saved;
                vec![node(
                    "For",
                    vec![
                        ("target", loop_target),
                        ("iter", iterable),
                        ("body", Value::Array(body)),
                        ("orelse", json!([])),
                        ("type_comment", Value::Null),
                    ],
                )]
            }
            StmtKind::Match { subject, arms } => self.match_stmt(subject, arms),
            StmtKind::Error => vec![],
        };
        out.into_iter().map(|s| self.at(s, span)).collect()
    }

    /// `assert`: a failed one reports its expression and, for a single comparison, the value
    /// of each side, evaluated once, left first, as Python would.
    fn assert(&mut self, test: &Expr, message: Option<&Expr>) -> Vec<Value> {
        let text = self.text.get(test.span.range()).unwrap_or("").to_string();
        let message = message.map_or(constant(Value::Null), |m| self.expr(m));
        if let ExprKind::Compare { first, rest } = &test.kind
            && let [(o, right)] = rest.as_slice()
        {
            let (l, r) = (self.temporary("a"), self.temporary("b"));
            let left_value = self.expr(first);
            let right_value = self.expr(right);
            let compare = node(
                "Compare",
                vec![("left", name(&l)), ("ops", json!([op(cmpop_name(*o))])), ("comparators", json!([name(&r)]))],
            );
            let failed = call(
                rt("assertion_failed"),
                vec![constant(text.into()), constant(o.text().into()), name(&l), name(&r), message],
            );
            let check = node(
                "If",
                vec![
                    ("test", node("UnaryOp", vec![("op", op("Not")), ("operand", compare)])),
                    ("body", json!([stmt_expr(failed)])),
                    ("orelse", json!([])),
                ],
            );
            return vec![assign(vec![target(&l)], left_value), assign(vec![target(&r)], right_value), check];
        }
        let value = self.expr(test);
        let failed = call(
            rt("assertion_failed"),
            vec![constant(text.into()), constant(Value::Null), constant(Value::Null), constant(Value::Null), message],
        );
        vec![node(
            "If",
            vec![
                ("test", node("UnaryOp", vec![("op", op("Not")), ("operand", value)])),
                ("body", json!([stmt_expr(failed)])),
                ("orelse", json!([])),
            ],
        )]
    }

    /// Bind `id` to `value`: into a `var`, the value is copied unless new; into anything else,
    /// copied only if a `var` still holds it. Integer arithmetic traps after the assignment.
    fn bind(&mut self, id: &str, at: Span, value: &Expr, entering_var: bool) -> Vec<Value> {
        if let Some((kind, raw)) = self.top_arithmetic(value) {
            let place = self.place_name(id);
            let assigned = self.at(assign(vec![stored_ctx(place.clone())], raw), value.span);
            return vec![assigned, self.trap_after(place, kind, value.span)];
        }
        let v = if entering_var { self.entering_var(value) } else { self.stored(value) };
        vec![assign(vec![self.at(stored_ctx(self.place_name(id)), at)], v)]
    }

    /// A name as a place: an `inout` parameter is its box's value.
    fn place_name(&self, id: &str) -> Value {
        if self.scope.boxed.contains(id) { attr(name(id), "value") } else { name(id) }
    }

    /// An integer `+ - * //` at the top of `value`, unchecked, with its operands checked: the
    /// statement then traps on the place it assigned.
    fn top_arithmetic(&mut self, value: &Expr) -> Option<(IntKind, Value)> {
        if self.no_walrus {
            return None;
        }
        let ExprKind::Binary { op: o, left, right } = &value.kind else { return None };
        let kind = self.int_kind(value)?;
        if !matches!(o, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::FloorDiv) {
            return None;
        }
        let l = self.expr(left);
        let r = self.expr(right);
        let raw = node("BinOp", vec![("left", l), ("op", op(binop_name(*o))), ("right", r)]);
        Some((kind, self.at(raw, value.span)))
    }

    fn assign_stmt(&mut self, place: &Expr, value: &Expr) -> Vec<Value> {
        match &place.kind {
            ExprKind::Name(id) => {
                let into = self.scope.mutable.contains(id) || self.scope.boxed.contains(id);
                self.bind(id, place.span, value, into)
            }
            ExprKind::Tuple(items) => {
                let targets: Vec<Value> = items.iter().map(|i| stored_ctx(self.expr(i))).collect();
                let any_var = items.iter().any(|i| self.rooted(i));
                let v = match &value.kind {
                    ExprKind::Tuple(values) if values.len() == items.len() => {
                        let elements: Vec<Value> = items
                            .iter()
                            .zip(values)
                            .map(|(t, v)| if self.rooted(t) { self.entering_var(v) } else { self.stored(v) })
                            .collect();
                        self.at(node("Tuple", vec![("elts", Value::Array(elements)), ("ctx", load())]), value.span)
                    }
                    _ if any_var => self.entering_var(value),
                    _ => self.stored(value),
                };
                let tuple = node("Tuple", vec![("elts", Value::Array(targets)), ("ctx", store())]);
                vec![assign(vec![self.at(tuple, place.span)], v)]
            }
            _ => {
                let v = self.entering_var(value);
                let t = stored_ctx(self.expr(place));
                vec![assign(vec![t], v)]
            }
        }
    }

    fn aug_assign(&mut self, place: &Expr, o: BinOp, value: &Expr, span: Span) -> Vec<Value> {
        let kind = self.int_kind(place);
        let traps = kind.is_some()
            && matches!(o, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::FloorDiv | BinOp::Pow | BinOp::LShift);
        if !traps {
            let v = if self.changeable_expr(value) { self.entering_var(value) } else { self.expr(value) };
            let t = stored_ctx(self.expr(place));
            return vec![node("AugAssign", vec![("target", t), ("op", op(binop_name(o))), ("value", v)])];
        }
        let kind = kind.unwrap_or(IntKind::I64);
        // A place with parts that run code is read and written once each: its parts go first
        // into temporaries.
        let mut prelude = Vec::new();
        let read = self.stable_place(place, &mut prelude);
        let right = self.expr(value);
        let combined = match o {
            BinOp::Pow => call(rt("power"), vec![read.clone(), right]),
            BinOp::LShift => call(rt("lshift"), vec![read.clone(), right]),
            _ => node("BinOp", vec![("left", read.clone()), ("op", op(binop_name(o))), ("right", right)]),
        };
        let combined = self.at(combined, span);
        if matches!(o, BinOp::Pow | BinOp::LShift) {
            prelude.push(assign(vec![stored_ctx(read)], combined));
            return prelude;
        }
        prelude.push(assign(vec![stored_ctx(read.clone())], combined));
        prelude.push(self.trap_after(read, kind, span));
        prelude
    }

    /// `place` as an expression that can be read and then written without running its parts
    /// twice: an object or index that is a call goes into a temporary first.
    fn stable_place(&mut self, place: &Expr, prelude: &mut Vec<Value>) -> Value {
        let simple = |e: &Expr| matches!(e.kind, ExprKind::Name(_) | ExprKind::Int(_) | ExprKind::Str(_));
        match &place.kind {
            ExprKind::Attr { object, name: field } if !simple(object) => {
                let t = self.temporary("k");
                let object_value = self.expr(object);
                prelude.push(self.at(assign(vec![target(&t)], object_value), object.span));
                self.at(attr(name(&t), &field.name), place.span)
            }
            ExprKind::Index { object, index } if !simple(object) || !simple(index) => {
                let object_value = self.expr(object);
                let index_value = self.expr(index);
                let (o, i) = (self.temporary("k"), self.temporary("k"));
                prelude.push(self.at(assign(vec![target(&o)], object_value), object.span));
                prelude.push(self.at(assign(vec![target(&i)], index_value), index.span));
                self.at(node("Subscript", vec![("value", name(&o)), ("slice", name(&i)), ("ctx", load())]), place.span)
            }
            _ => self.expr(place),
        }
    }

    /// Mark the names a loop binds as still held by a `var` when it iterates over one.
    fn taint(&mut self, t: &Target, iter: &Expr) {
        if self.rooted(iter) {
            for n in t.names() {
                self.scope.tainted.insert(n.name.clone());
            }
        }
    }

    fn loop_target(&self, t: &Target) -> Value {
        match t {
            Target::Name(n) => self.at(target(&n.name), n.span),
            Target::Tuple(items, span) => {
                let elements: Vec<Value> = items.iter().map(|i| self.loop_target(i)).collect();
                self.at(node("Tuple", vec![("elts", Value::Array(elements)), ("ctx", store())]), *span)
            }
        }
    }

    fn match_stmt(&mut self, subject: &Expr, arms: &[Arm]) -> Vec<Value> {
        let subject_value = self.expr(subject);
        let rooted = self.rooted(subject);
        let mut cases = Vec::new();
        let mut complete = false;
        for arm in arms {
            let saved = self.scope.tainted.clone();
            if rooted {
                bound_names(&arm.pattern, &mut self.scope.tainted);
            }
            let pattern = self.pattern(&arm.pattern);
            complete |= irrefutable(&arm.pattern, &self.units);
            let body = self.block(&arm.body);
            self.scope.tainted = saved;
            let case =
                node("match_case", vec![("pattern", pattern), ("guard", Value::Null), ("body", Value::Array(body))]);
            cases.push(self.at(case, arm.span));
        }
        if !complete {
            let unmatched = node("MatchAs", vec![("pattern", Value::Null), ("name", "__unmatched".into())]);
            let fallback = stmt_expr(call(rt("no_match"), vec![name("__unmatched")]));
            let case =
                node("match_case", vec![("pattern", unmatched), ("guard", Value::Null), ("body", json!([fallback]))]);
            cases.push(self.at(case, subject.span));
        }
        vec![node("Match", vec![("subject", subject_value), ("cases", Value::Array(cases))])]
    }

    fn pattern(&mut self, p: &Pattern) -> Value {
        let value = match &p.kind {
            PatternKind::Wildcard | PatternKind::Error => {
                node("MatchAs", vec![("pattern", Value::Null), ("name", Value::Null)])
            }
            PatternKind::Name(n) if self.units.contains(&n.name) => {
                node("MatchValue", vec![("value", self.at(attr(name("__variants"), &n.name), n.span))])
            }
            PatternKind::Name(n) => node("MatchAs", vec![("pattern", Value::Null), ("name", n.name.clone().into())]),
            PatternKind::Variant { name: n, args } => {
                let patterns: Vec<Value> = args.iter().map(|a| self.pattern(a)).collect();
                node(
                    "MatchClass",
                    vec![
                        ("cls", self.at(name(&n.name), n.span)),
                        ("patterns", Value::Array(patterns)),
                        ("kwd_attrs", json!([])),
                        ("kwd_patterns", json!([])),
                    ],
                )
            }
            PatternKind::Tuple(items) => {
                let patterns: Vec<Value> = items.iter().map(|i| self.pattern(i)).collect();
                node("MatchSequence", vec![("patterns", Value::Array(patterns))])
            }
            PatternKind::Literal(e) => match &e.kind {
                ExprKind::None => node("MatchSingleton", vec![("value", Value::Null)]),
                ExprKind::Bool(b) => node("MatchSingleton", vec![("value", (*b).into())]),
                _ => node("MatchValue", vec![("value", self.expr(e))]),
            },
        };
        self.at(value, p.span)
    }

    // Expressions ----------------------------------------------------------------------------

    fn expr(&mut self, e: &Expr) -> Value {
        let value = self.expr_inner(e);
        self.at(value, e.span)
    }

    fn list(&mut self, items: &[Expr]) -> Value {
        Value::Array(items.iter().map(|i| self.stored(i)).collect())
    }

    fn expr_inner(&mut self, e: &Expr) -> Value {
        match &e.kind {
            ExprKind::Name(n) => self.place_name(n),
            ExprKind::Int(text) => constant(json!({"_int": text})),
            ExprKind::Float(text) => constant(json!({"_float": text})),
            ExprKind::Bool(b) => constant((*b).into()),
            ExprKind::None | ExprKind::Unit => constant(Value::Null),
            ExprKind::Str(literals) => self.string(literals),
            ExprKind::Tuple(items) => {
                let elements = self.list(items);
                node("Tuple", vec![("elts", elements), ("ctx", load())])
            }
            ExprKind::List(items) => {
                let elements = self.list(items);
                node("List", vec![("elts", elements), ("ctx", load())])
            }
            ExprKind::Set(items) => {
                let elements = self.list(items);
                node("Set", vec![("elts", elements)])
            }
            ExprKind::Dict(pairs) => {
                let keys: Vec<Value> = pairs.iter().map(|(k, _)| self.expr(k)).collect();
                let values: Vec<Value> = pairs.iter().map(|(_, v)| self.stored(v)).collect();
                node("Dict", vec![("keys", Value::Array(keys)), ("values", Value::Array(values))])
            }
            ExprKind::ListComp { element, loops } => self.comprehension("ListComp", None, element, loops),
            ExprKind::SetComp { element, loops } => self.comprehension("SetComp", None, element, loops),
            ExprKind::Generator { element, loops } => self.comprehension("GeneratorExp", None, element, loops),
            ExprKind::DictComp { key, value, loops } => self.comprehension("DictComp", Some(key), value, loops),
            ExprKind::Unary { op: o, operand } => {
                let value = self.expr(operand);
                match o {
                    UnaryOp::Neg => {
                        let negated = self.at(node("UnaryOp", vec![("op", op("USub")), ("operand", value)]), e.span);
                        match self.int_kind(e) {
                            Some(kind) => self.checked(negated, kind, e.span),
                            None => negated,
                        }
                    }
                    UnaryOp::Pos => node("UnaryOp", vec![("op", op("UAdd")), ("operand", value)]),
                    UnaryOp::Invert => node("UnaryOp", vec![("op", op("Invert")), ("operand", value)]),
                }
            }
            ExprKind::Binary { op: o, left, right } => {
                let l = self.expr(left);
                let r = self.expr(right);
                let kind = self.int_kind(e);
                match (o, kind) {
                    (BinOp::Pow, Some(_)) => call(rt("power"), vec![l, r]),
                    (BinOp::LShift, Some(_)) => call(rt("lshift"), vec![l, r]),
                    (BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::FloorDiv, Some(kind)) => {
                        let raw =
                            self.at(node("BinOp", vec![("left", l), ("op", op(binop_name(*o))), ("right", r)]), e.span);
                        self.checked(raw, kind, e.span)
                    }
                    _ => node("BinOp", vec![("left", l), ("op", op(binop_name(*o))), ("right", r)]),
                }
            }
            ExprKind::Compare { first, rest } => {
                let left = self.expr(first);
                let ops: Vec<Value> = rest.iter().map(|(o, _)| op(cmpop_name(*o))).collect();
                let comparators: Vec<Value> = rest.iter().map(|(_, r)| self.expr(r)).collect();
                node(
                    "Compare",
                    vec![("left", left), ("ops", Value::Array(ops)), ("comparators", Value::Array(comparators))],
                )
            }
            ExprKind::Logical { op: o, operands } => {
                let values: Vec<Value> = operands.iter().map(|x| self.expr(x)).collect();
                let kind = if *o == BoolOp::And { "And" } else { "Or" };
                node("BoolOp", vec![("op", op(kind)), ("values", Value::Array(values))])
            }
            ExprKind::Not(inner) => {
                let value = self.expr(inner);
                node("UnaryOp", vec![("op", op("Not")), ("operand", value)])
            }
            ExprKind::Coalesce { value, default } => {
                let v = self.stored(value);
                let d = self.stored(default);
                let thunk = self.at(lambda(d), default.span);
                call(rt("coalesce"), vec![v, thunk])
            }
            ExprKind::IfExp { test, then, orelse } => {
                let test = self.expr(test);
                let body = self.expr(then);
                let orelse = self.expr(orelse);
                node("IfExp", vec![("test", test), ("body", body), ("orelse", orelse)])
            }
            ExprKind::Lambda { params, body } => self.lambda(params, body),
            ExprKind::Call { func, args } => self.call(func, args, e),
            ExprKind::Index { object, index } => {
                let o = self.expr(object);
                let i = self.expr(index);
                node("Subscript", vec![("value", o), ("slice", i), ("ctx", load())])
            }
            ExprKind::Slice { object, lower, upper, step } => {
                let o = self.expr(object);
                let mut part = |p: &Option<Box<Expr>>| p.as_ref().map_or(Value::Null, |x| self.expr(x));
                let (l, u, s) = (part(lower), part(upper), part(step));
                let slice = node("Slice", vec![("lower", l), ("upper", u), ("step", s)]);
                node("Subscript", vec![("value", o), ("slice", slice), ("ctx", load())])
            }
            ExprKind::Attr { object, name: field } => {
                let o = self.expr(object);
                attr(o, &field.name)
            }
            ExprKind::Try(inner) => {
                let value = self.expr(inner);
                call(rt("unwrap"), vec![value])
            }
            ExprKind::Fail(error) => {
                let value = self.stored(error);
                call(rt("fail"), vec![value])
            }
            ExprKind::Error => constant(Value::Null),
        }
    }

    fn string(&mut self, literals: &[StrLit]) -> Value {
        if literals.iter().any(|l| l.bytes) {
            let bytes: Vec<u8> = literals
                .iter()
                .flat_map(|l| l.parts.iter())
                .filter_map(|p| match p {
                    StrPart::Text(t) => Some(t.chars().map(|c| c as u32 as u8).collect::<Vec<_>>()),
                    StrPart::Expr { .. } => None,
                })
                .flatten()
                .collect();
            return constant(json!({ "_bytes": bytes }));
        }
        let parts: Vec<&StrPart> = literals.iter().flat_map(|l| l.parts.iter()).collect();
        if parts.iter().all(|p| matches!(p, StrPart::Text(_))) {
            let text: String = parts.iter().map(|p| if let StrPart::Text(t) = p { t.as_str() } else { "" }).collect();
            return constant(text.into());
        }
        let values = self.formatted(&parts);
        node("JoinedStr", vec![("values", Value::Array(values))])
    }

    fn formatted(&mut self, parts: &[&StrPart]) -> Vec<Value> {
        let mut values = Vec::new();
        for part in parts {
            match part {
                StrPart::Text(t) if t.is_empty() => {}
                StrPart::Text(t) => values.push(constant(t.clone().into())),
                StrPart::Expr { expr, conversion, spec } => {
                    let value = self.expr(expr);
                    let spec_parts: Vec<&StrPart> = spec.iter().collect();
                    let spec = if spec.is_empty() {
                        Value::Null
                    } else {
                        node("JoinedStr", vec![("values", Value::Array(self.formatted(&spec_parts)))])
                    };
                    let conversion = conversion.map_or(-1, |c| c as i64);
                    values.push(self.at(
                        node(
                            "FormattedValue",
                            vec![("value", value), ("conversion", conversion.into()), ("format_spec", spec)],
                        ),
                        expr.span,
                    ));
                }
            }
        }
        values
    }

    fn comprehension(&mut self, kind: &str, key: Option<&Expr>, element: &Expr, loops: &[Comprehension]) -> Value {
        let saved = self.scope.tainted.clone();
        let mut generators = Vec::new();
        for l in loops {
            let outer = std::mem::replace(&mut self.no_walrus, true);
            let iterable = self.expr(&l.iter);
            self.no_walrus = outer;
            self.taint(&l.target, &l.iter);
            let conditions: Vec<Value> = l.conditions.iter().map(|c| self.expr(c)).collect();
            generators.push(node(
                "comprehension",
                vec![
                    ("target", self.loop_target(&l.target)),
                    ("iter", iterable),
                    ("ifs", Value::Array(conditions)),
                    ("is_async", 0.into()),
                ],
            ));
        }
        let value = match key {
            Some(k) => {
                let key_value = self.expr(k);
                let value = self.stored(element);
                node("DictComp", vec![("key", key_value), ("value", value), ("generators", Value::Array(generators))])
            }
            None => {
                let value = self.stored(element);
                node(kind, vec![("elt", value), ("generators", Value::Array(generators))])
            }
        };
        self.scope.tainted = saved;
        value
    }

    /// A lambda; the locals it uses that may change after it is made are captured as copies,
    /// through keyword defaults evaluated when the lambda is.
    fn lambda(&mut self, params: &[Ident], body: &Expr) -> Value {
        let mut free = Vec::new();
        names_in(body, &mut free);
        let own: HashSet<&str> = params.iter().map(|p| p.name.as_str()).collect();
        let mut seen = HashSet::new();
        let captured: Vec<String> = free
            .into_iter()
            .filter(|n| !own.contains(n.as_str()) && (self.scope.mutable.contains(n) || self.scope.boxed.contains(n)))
            .filter(|n| seen.insert(n.clone()))
            .collect();
        let saved = self.scope.clone();
        for p in params {
            self.scope.mutable.remove(&p.name);
            self.scope.boxed.remove(&p.name);
            self.scope.tainted.remove(&p.name);
        }
        let kw_defaults: Vec<Value> = captured.iter().map(|n| call(rt("copy"), vec![self.place_name(n)])).collect();
        for n in &captured {
            self.scope.mutable.remove(n);
            self.scope.boxed.remove(n);
        }
        let value = self.expr(body);
        self.scope = saved;
        let args: Vec<Value> = params.iter().map(|p| self.at(arg(&p.name), p.span)).collect();
        let kwonly: Vec<Value> = captured.iter().map(|n| arg(n)).collect();
        node("Lambda", vec![("args", arguments(args, vec![], kwonly, kw_defaults)), ("body", value)])
    }

    fn call(&mut self, func: &Expr, args: &[Arg], whole: &Expr) -> Value {
        // A built-in method whose lotml meaning differs from Python's goes to the runtime.
        if let ExprKind::Attr { object, name: method } = &func.kind
            && let Some(receiver) = self.ty(object).cloned()
        {
            if let Some(runtime) = runtime_method(&receiver, &method.name, args.len()) {
                let receiver_value = self.expr(object);
                let (mut positional, keywords) = self.plain_arguments(args, false);
                positional.insert(0, receiver_value);
                return node(
                    "Call",
                    vec![
                        ("func", rt(runtime)),
                        ("args", Value::Array(positional)),
                        ("keywords", Value::Array(keywords)),
                    ],
                );
            }
            let builtin = matches!(receiver, Ty::List(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Heap(_));
            if builtin {
                let storing = STORING.contains(&method.name.as_str());
                let callee = self.expr(func);
                let (positional, keywords) = self.plain_arguments(args, storing);
                return node(
                    "Call",
                    vec![("func", callee), ("args", Value::Array(positional)), ("keywords", Value::Array(keywords))],
                );
            }
        }
        // `Stack[int]()` and `first[int](xs)`: the type arguments are erased.
        let callee = match &func.kind {
            ExprKind::Index { object, .. } if matches!(object.kind, ExprKind::Name(_)) => self.expr(object),
            _ => self.expr(func),
        };
        let prelude = match &func.kind {
            ExprKind::Name(n) if !self.functions.contains(n) => Some(n.as_str()),
            _ => None,
        };
        // Prelude functions that keep nothing of their arguments read them as they are.
        let keeps = prelude.is_none_or(|n| ALIASING.contains(&n) || n.chars().next().is_some_and(char::is_uppercase));
        let mut positional = Vec::new();
        let mut keywords = Vec::new();
        let mut writebacks = Vec::new();
        for a in args {
            match a {
                Arg::Positional(e) => positional.push(if keeps { self.stored(e) } else { self.expr(e) }),
                Arg::Keyword(k, e) => {
                    let value = if keeps { self.stored(e) } else { self.expr(e) };
                    keywords.push(node("keyword", vec![("arg", k.name.clone().into()), ("value", value)]));
                }
                Arg::Inout(place, _) => {
                    // An `inout` parameter is passed on as the box it already is.
                    if let ExprKind::Name(n) = &place.kind
                        && self.scope.boxed.contains(n)
                    {
                        positional.push(self.at(name(n), place.span));
                        continue;
                    }
                    let current = self.expr(place);
                    let b = self.temporary("inout");
                    let boxed =
                        node("NamedExpr", vec![("target", target(&b)), ("value", call(rt("Box"), vec![current]))]);
                    positional.push(self.at(boxed, place.span));
                    writebacks.push(self.writeback(place, &b));
                }
            }
        }
        let node_call = node(
            "Call",
            vec![("func", callee), ("args", Value::Array(positional)), ("keywords", Value::Array(keywords))],
        );
        if writebacks.is_empty() {
            return node_call;
        }
        let mut values = vec![self.at(node_call, whole.span)];
        values.extend(writebacks);
        call(rt("returning"), values)
    }

    /// The arguments of a built-in method, positional and keyword; with `storing`, each enters
    /// the receiver, a `var`, so it is copied unless new.
    fn plain_arguments(&mut self, args: &[Arg], storing: bool) -> (Vec<Value>, Vec<Value>) {
        let mut positional = Vec::new();
        let mut keywords = Vec::new();
        for a in args {
            let value = if storing { self.entering_var(a.expr()) } else { self.expr(a.expr()) };
            match a {
                Arg::Keyword(k, _) => {
                    keywords.push(node("keyword", vec![("arg", k.name.clone().into()), ("value", value)]))
                }
                _ => positional.push(value),
            }
        }
        (positional, keywords)
    }

    /// The expression storing box `b`'s final value back into `place`.
    fn writeback(&mut self, place: &Expr, b: &str) -> Value {
        let value = attr(name(b), "value");
        let out = match &place.kind {
            ExprKind::Name(n) => node("NamedExpr", vec![("target", target(n)), ("value", value)]),
            ExprKind::Attr { object, name: field } => {
                let o = self.expr(object);
                call(rt("set_attr"), vec![o, constant(field.name.clone().into()), value])
            }
            ExprKind::Index { object, index } => {
                let o = self.expr(object);
                let i = self.expr(index);
                call(rt("set_item"), vec![o, i, value])
            }
            _ => value,
        };
        self.at(out, place.span)
    }
}

trait FunctionNames {
    fn contains_key_fn(&self, func: &Expr) -> bool;
}

impl FunctionNames for HashSet<String> {
    /// Whether `func` names a user function, whose result may hold its arguments.
    fn contains_key_fn(&self, func: &Expr) -> bool {
        matches!(&func.kind, ExprKind::Name(n) if self.contains(n))
    }
}

/// The runtime function for a built-in method whose lotml meaning differs from Python's.
fn runtime_method(receiver: &Ty, method: &str, args: usize) -> Option<&'static str> {
    Some(match (receiver, method) {
        (Ty::Str, "find") => "str_find",
        (Ty::Str, "rfind") => "str_rfind",
        (Ty::Str, "split_once") => "split_once",
        (Ty::Str, "to_int") => "to_int",
        (Ty::Str, "to_float") => "to_float",
        (Ty::Str, "format") => "str_format",
        (Ty::List(_), "pop") => "list_pop",
        (Ty::List(_), "last") => "list_last",
        (Ty::List(_), "find") => "list_find",
        (Ty::List(_), "index") if args == 1 => "list_index",
        (Ty::List(_) | Ty::Dict(..) | Ty::Set(_), "contains") => "contains",
        (Ty::Dict(..), "pop") => "dict_pop",
        (Ty::Dict(..), "keys") => "keys",
        (Ty::Dict(..), "values") => "values",
        (Ty::Dict(..), "items") => "items",
        (Ty::Set(_), "pop") => "set_pop",
        _ => return None,
    })
}

fn binop_name(o: BinOp) -> &'static str {
    match o {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mult",
        BinOp::Div => "Div",
        BinOp::FloorDiv => "FloorDiv",
        BinOp::Mod => "Mod",
        BinOp::Pow => "Pow",
        BinOp::LShift => "LShift",
        BinOp::RShift => "RShift",
        BinOp::BitOr => "BitOr",
        BinOp::BitXor => "BitXor",
        BinOp::BitAnd => "BitAnd",
    }
}

fn cmpop_name(o: CmpOp) -> &'static str {
    match o {
        CmpOp::Lt => "Lt",
        CmpOp::Gt => "Gt",
        CmpOp::Le => "LtE",
        CmpOp::Ge => "GtE",
        CmpOp::Eq => "Eq",
        CmpOp::NotEq => "NotEq",
        CmpOp::In => "In",
        CmpOp::NotIn => "NotIn",
        CmpOp::Is => "Is",
        CmpOp::IsNot => "IsNot",
    }
}

/// The names a function declares with `var`, anywhere in its body.
fn collect_vars(block: &Block, out: &mut HashSet<String>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Var { name, .. } => {
                out.insert(name.name.clone());
            }
            StmtKind::If { branches, orelse } => {
                branches.iter().for_each(|(_, b)| collect_vars(b, out));
                if let Some(b) = orelse {
                    collect_vars(b, out);
                }
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => collect_vars(body, out),
            StmtKind::Match { arms, .. } => arms.iter().for_each(|a| collect_vars(&a.body, out)),
            _ => {}
        }
    }
}

fn bound_names(p: &Pattern, out: &mut HashSet<String>) {
    match &p.kind {
        PatternKind::Name(n) => {
            out.insert(n.name.clone());
        }
        PatternKind::Variant { args, .. } | PatternKind::Tuple(args) => args.iter().for_each(|a| bound_names(a, out)),
        _ => {}
    }
}

fn irrefutable(p: &Pattern, units: &HashSet<String>) -> bool {
    match &p.kind {
        PatternKind::Wildcard => true,
        PatternKind::Name(n) => !units.contains(&n.name),
        _ => false,
    }
}

/// Every name an expression reads, in order.
fn names_in(e: &Expr, out: &mut Vec<String>) {
    let mut go = |x: &Expr| names_in(x, out);
    match &e.kind {
        ExprKind::Name(n) => out.push(n.clone()),
        ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => items.iter().for_each(go),
        ExprKind::Dict(pairs) => pairs.iter().for_each(|(k, v)| {
            names_in(k, out);
            names_in(v, out);
        }),
        ExprKind::ListComp { element, loops }
        | ExprKind::SetComp { element, loops }
        | ExprKind::Generator { element, loops } => {
            names_in(element, out);
            for l in loops {
                names_in(&l.iter, out);
                l.conditions.iter().for_each(|c| names_in(c, out));
            }
        }
        ExprKind::DictComp { key, value, loops } => {
            names_in(key, out);
            names_in(value, out);
            for l in loops {
                names_in(&l.iter, out);
                l.conditions.iter().for_each(|c| names_in(c, out));
            }
        }
        ExprKind::Unary { operand: x, .. } | ExprKind::Not(x) | ExprKind::Try(x) | ExprKind::Fail(x) => go(x),
        ExprKind::Lambda { body, .. } => go(body),
        ExprKind::Binary { left, right, .. } => {
            names_in(left, out);
            names_in(right, out);
        }
        ExprKind::Coalesce { value, default } => {
            names_in(value, out);
            names_in(default, out);
        }
        ExprKind::Compare { first, rest } => {
            names_in(first, out);
            rest.iter().for_each(|(_, r)| names_in(r, out));
        }
        ExprKind::Logical { operands, .. } => operands.iter().for_each(go),
        ExprKind::IfExp { test, then, orelse } => {
            names_in(test, out);
            names_in(then, out);
            names_in(orelse, out);
        }
        ExprKind::Call { func, args } => {
            names_in(func, out);
            args.iter().for_each(|a| names_in(a.expr(), out));
        }
        ExprKind::Index { object, index } => {
            names_in(object, out);
            names_in(index, out);
        }
        ExprKind::Slice { object, lower, upper, step } => {
            names_in(object, out);
            for part in [lower, upper, step].into_iter().flatten() {
                names_in(part, out);
            }
        }
        ExprKind::Attr { object, .. } => go(object),
        ExprKind::Str(literals) => {
            for literal in literals {
                for part in &literal.parts {
                    if let StrPart::Expr { expr, .. } = part {
                        names_in(expr, out);
                    }
                }
            }
        }
        _ => {}
    }
}
