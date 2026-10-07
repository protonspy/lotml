//! The C of the program's types: a struct per tuple, optional and result type, held by value; a
//! cell per record and sum type, held by pointer; and for each, the functions and descriptor the
//! runtime's collections use (specs/c-backend/design.md, "Data").

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use lotml_check::TypeDef;
use lotml_check::ty::{IntKind, Ty};

use crate::emit::c_string_text;
use crate::mir::counted;

/// A field of a struct or cell: how it is named in the text of a value, and its type.
struct Field {
    name: Option<String>,
    ty: Ty,
}

enum Shape {
    Tuple(Vec<Ty>),
    Optional(Ty),
    Result(Ty, Ty),
    Record(String, Vec<Field>),
    /// Each variant: its name, and its fields, `None` for a variant written without them.
    Sum(Vec<(String, Option<Vec<Field>>)>),
    /// A `dyn` value of the trait: the cell, and the table of its type's methods.
    Dyn(String),
}

pub struct Types<'d> {
    order: Vec<Ty>,
    pub ids: HashMap<Ty, usize>,
    declared: &'d BTreeMap<String, TypeDef>,
    /// How many methods each trait's table holds.
    traits: &'d BTreeMap<String, usize>,
}

impl<'d> Types<'d> {
    pub fn new(declared: &'d BTreeMap<String, TypeDef>, traits: &'d BTreeMap<String, usize>) -> Types<'d> {
        Types { order: Vec::new(), ids: HashMap::new(), declared, traits }
    }

    pub fn register(&mut self, ty: &Ty) {
        match ty {
            Ty::List(t) | Ty::Set(t) | Ty::Heap(t) => self.register(t),
            Ty::Dict(k, v) => {
                self.register(k);
                self.register(v);
            }
            Ty::Func(params, ret) => {
                for t in params {
                    self.register(t);
                }
                self.register(ret);
            }
            Ty::Tuple(items) => {
                for t in items {
                    self.register(t);
                }
                self.add(ty);
            }
            Ty::Optional(t) => {
                self.register(t);
                self.add(ty);
            }
            Ty::Result(t, e) => {
                self.register(t);
                self.register(e);
                self.add(ty);
            }
            Ty::Dyn(_) => self.add(ty),
            Ty::Adt(..) => {
                if self.ids.contains_key(ty) {
                    return;
                }
                self.add(ty);
                let fields: Vec<Ty> = match self.shape(ty) {
                    Shape::Record(_, fields) => fields.into_iter().map(|f| f.ty).collect(),
                    Shape::Sum(variants) => {
                        variants.into_iter().flat_map(|(_, f)| f.unwrap_or_default()).map(|f| f.ty).collect()
                    }
                    _ => Vec::new(),
                };
                for t in &fields {
                    self.register(t);
                }
            }
            _ => {}
        }
    }

    fn add(&mut self, ty: &Ty) {
        if !self.ids.contains_key(ty) {
            self.ids.insert(ty.clone(), self.order.len());
            self.order.push(ty.clone());
        }
    }

    fn shape(&self, ty: &Ty) -> Shape {
        match ty {
            Ty::Tuple(items) => Shape::Tuple(items.clone()),
            Ty::Optional(t) => Shape::Optional((**t).clone()),
            Ty::Result(t, e) => Shape::Result((**t).clone(), (**e).clone()),
            Ty::Dyn(name) => Shape::Dyn(name.clone()),
            Ty::Adt(name, args) => {
                let fields = |fields: &[lotml_check::FieldSig], params: &[String]| {
                    fields.iter().map(|f| Field { name: f.name.clone(), ty: f.ty.substitute(params, args) }).collect()
                };
                match self.declared.get(name) {
                    Some(TypeDef::Record { params, fields: fs }) => Shape::Record(name.clone(), fields(fs, params)),
                    Some(TypeDef::Sum { params, variants }) => Shape::Sum(
                        variants
                            .iter()
                            .map(|v| (v.name.clone(), v.fields.as_deref().map(|fs| fields(fs, params))))
                            .collect(),
                    ),
                    None => Shape::Tuple(Vec::new()),
                }
            }
            _ => Shape::Tuple(Vec::new()),
        }
    }

    /// The C type of a value of `ty`.
    pub fn c_type(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(IntKind::I8) => "int8_t".into(),
            Ty::Int(IntKind::I16) => "int16_t".into(),
            Ty::Int(IntKind::I32) => "int32_t".into(),
            Ty::Int(IntKind::I64) => "int64_t".into(),
            Ty::Int(IntKind::U8) => "uint8_t".into(),
            Ty::Int(IntKind::U16) => "uint16_t".into(),
            Ty::Int(IntKind::U32) => "uint32_t".into(),
            Ty::Int(IntKind::U64) => "uint64_t".into(),
            // An `f32` is a Python float on the Python target, so it is a double here too.
            Ty::Float(_) => "double".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "lt_str *".into(),
            Ty::List(_) | Ty::Heap(_) => "lt_list *".into(),
            Ty::Dict(..) => "lt_dict *".into(),
            Ty::Set(_) => "lt_set *".into(),
            Ty::Func(..) => "lt_closure *".into(),
            Ty::Tuple(_) | Ty::Optional(_) | Ty::Result(..) | Ty::Dyn(_) => format!("lt_t{}", self.ids[ty]),
            Ty::Adt(..) => format!("lt_t{} *", self.ids[ty]),
            _ => "uint8_t".into(),
        }
    }

    /// Whether a value of `ty` is a struct held by value, which C zero-initialises with `{0}`.
    pub fn by_value(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Tuple(_) | Ty::Optional(_) | Ty::Result(..) | Ty::Dyn(_))
    }

    /// The descriptor of `ty`, as a C pointer.
    pub fn desc(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(IntKind::I8) => "&lt_type_i8".into(),
            Ty::Int(IntKind::I16) => "&lt_type_i16".into(),
            Ty::Int(IntKind::I32) => "&lt_type_i32".into(),
            Ty::Int(IntKind::I64) => "&lt_type_i64".into(),
            Ty::Int(IntKind::U8) => "&lt_type_u8".into(),
            Ty::Int(IntKind::U16) => "&lt_type_u16".into(),
            Ty::Int(IntKind::U32) => "&lt_type_u32".into(),
            Ty::Int(IntKind::U64) => "&lt_type_u64".into(),
            Ty::Float(_) => "&lt_type_f64".into(),
            Ty::Bool => "&lt_type_bool".into(),
            Ty::Str => "&lt_type_str".into(),
            Ty::List(_) => "&lt_type_list".into(),
            Ty::Heap(_) => "&lt_type_heap".into(),
            Ty::Dict(..) => "&lt_type_dict".into(),
            Ty::Set(_) => "&lt_type_set".into(),
            Ty::Func(..) => "&lt_type_closure".into(),
            Ty::Tuple(_) | Ty::Optional(_) | Ty::Result(..) | Ty::Adt(..) | Ty::Dyn(_) => {
                format!("&lt_type_t{}", self.ids[ty])
            }
            _ => "&lt_type_none".into(),
        }
    }

    pub fn counted(&self, ty: &Ty) -> bool {
        counted(ty)
    }

    /// The C statement adding (`inc`) or taking a count of the value `name`, of type `ty`; empty
    /// for a type that holds none. A local not yet set is NULL or zero, which neither touches.
    pub fn count(&self, ty: &Ty, inc: bool, name: &str) -> String {
        match ty {
            Ty::Str | Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Adt(..) | Ty::Func(..) if inc => {
                format!("if ({name}) lt_inc({name});")
            }
            Ty::Func(..) => format!("lt_closure_drop({name});"),
            Ty::Str => format!("lt_str_drop({name});"),
            Ty::List(_) | Ty::Heap(_) => format!("lt_list_drop({name});"),
            Ty::Dict(..) => format!("lt_dict_drop({name});"),
            Ty::Set(_) => format!("lt_set_drop({name});"),
            _ if self.counted(ty) => {
                let id = self.ids[ty];
                format!("lt_{}_t{id}(&{name});", if inc { "inc" } else { "dec" })
            }
            _ => String::new(),
        }
    }

    /// The C name of the variant `k` of the sum type `ty`: its struct, or its static cell.
    pub fn variant(&self, ty: &Ty, k: usize) -> String {
        format!("lt_t{}_v{k}", self.ids[ty])
    }

    /// The structs, functions and descriptors of the program's types.
    pub fn definitions(&self) -> String {
        let mut out = String::new();
        for (name, &n) in self.traits {
            let _ = writeln!(
                out,
                "typedef struct lt_vt_{name} {{ const lt_type *type; void (*m[{}])(void); }} lt_vt_{name};",
                n.max(1)
            );
        }
        for (id, ty) in self.order.iter().enumerate() {
            if matches!(ty, Ty::Adt(..)) {
                let _ = writeln!(out, "typedef struct lt_t{id} lt_t{id};");
            }
        }
        for (id, ty) in self.order.iter().enumerate() {
            let fields = match self.shape(ty) {
                Shape::Tuple(items) => {
                    items.iter().enumerate().map(|(i, t)| format!("{} f{i};", self.c_type(t))).collect()
                }
                Shape::Optional(t) => vec!["bool some;".to_string(), format!("{} value;", self.c_type(&t))],
                Shape::Result(t, e) => {
                    vec![
                        "bool ok;".to_string(),
                        format!("{} value;", self.c_type(&t)),
                        format!("{} error;", self.c_type(&e)),
                    ]
                }
                Shape::Dyn(name) => vec!["void *cell;".to_string(), format!("const lt_vt_{name} *v;")],
                Shape::Record(..) | Shape::Sum(_) => continue,
            };
            let _ = writeln!(out, "typedef struct lt_t{id} {{ {} }} lt_t{id};", fields.join(" "));
        }
        for (id, ty) in self.order.iter().enumerate() {
            match self.shape(ty) {
                Shape::Record(_, fields) => {
                    let fields: Vec<String> =
                        fields.iter().enumerate().map(|(i, f)| format!(" {} f{i};", self.c_type(&f.ty))).collect();
                    let _ = writeln!(out, "struct lt_t{id} {{ lt_cell cell;{} }};", fields.concat());
                }
                Shape::Sum(variants) => {
                    let _ = writeln!(out, "struct lt_t{id} {{ lt_cell cell; }};");
                    for (k, (_, fields)) in variants.iter().enumerate() {
                        match fields {
                            Some(fields) => {
                                let fields: Vec<String> = fields
                                    .iter()
                                    .enumerate()
                                    .map(|(i, f)| format!(" {} f{i};", self.c_type(&f.ty)))
                                    .collect();
                                let _ = writeln!(
                                    out,
                                    "typedef struct lt_t{id}_v{k} {{ lt_cell cell;{} }} lt_t{id}_v{k};",
                                    fields.concat()
                                );
                            }
                            None => {
                                let _ = writeln!(out, "static lt_cell lt_t{id}_v{k} LT_UNUSED = {{0, {k}}};");
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for id in 0..self.order.len() {
            let _ = writeln!(
                out,
                "static void lt_inc_t{id}(void *p);\nstatic void lt_dec_t{id}(void *p);\n\
                 static bool lt_eq_t{id}(const void *a, const void *b);\n\
                 static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at);\n\
                 static int64_t lt_hash_t{id}(const void *p);\nstatic void lt_repr_t{id}(lt_buf *b, const void *p);\n\
                 static void lt_share_t{id}(void *p);\nstatic void lt_show_t{id}(lt_buf *b, const void *p);"
            );
        }
        for (id, ty) in self.order.iter().enumerate() {
            let c = self.c_type(ty);
            // An optional writes its value as the value itself does: str inside print, repr
            // inside a container.
            let text = if matches!(ty, Ty::Optional(_)) { format!("lt_text_t{id}") } else { format!("lt_repr_t{id}") };
            if matches!(ty, Ty::Optional(_)) {
                let _ = writeln!(out, "static void lt_text_t{id}(lt_buf *b, const void *p);");
            }
            let _ = writeln!(
                out,
                "static const lt_type lt_type_t{id} LT_UNUSED = {{sizeof({c}), lt_inc_t{id}, lt_dec_t{id}, lt_eq_t{id}, \
                 lt_cmp_t{id}, lt_hash_t{id}, lt_repr_t{id}, {text}, lt_share_t{id}, lt_show_t{id}}};"
            );
        }
        for (id, ty) in self.order.iter().enumerate() {
            match self.shape(ty) {
                Shape::Tuple(items) => self.tuple_functions(&mut out, id, &items),
                Shape::Optional(t) => self.optional_functions(&mut out, id, &t),
                Shape::Result(t, e) => self.result_functions(&mut out, id, &t, &e),
                Shape::Record(name, fields) => self.record_functions(&mut out, id, &name, &fields),
                Shape::Sum(variants) => self.sum_functions(&mut out, id, &variants),
                Shape::Dyn(name) => dyn_functions(&mut out, id, &name),
            }
        }
        out.push('\n');
        out
    }

    /// `call` (`inc`, `dec`, `share`) on each counted field `f{i}` of the struct `v` points at.
    fn each_field(&self, types: &[Ty], v: &str, call: &str) -> String {
        types
            .iter()
            .enumerate()
            .filter(|(_, t)| self.counted(t))
            .map(|(i, t)| format!(" ({})->{call}(&{v}->f{i});", self.desc(t)))
            .collect()
    }

    fn eq_fields(&self, types: &[Ty], x: &str, y: &str) -> String {
        let eq: Vec<String> =
            types.iter().enumerate().map(|(i, t)| format!("({})->eq(&{x}->f{i}, &{y}->f{i})", self.desc(t))).collect();
        if eq.is_empty() { "true".to_string() } else { eq.join(" && ") }
    }

    fn cmp_fields(&self, types: &[Ty], x: &str, y: &str) -> String {
        types
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let d = self.desc(t);
                format!(" if (!({d})->eq(&{x}->f{i}, &{y}->f{i})) return ({d})->cmp(&{x}->f{i}, &{y}->f{i}, at);")
            })
            .collect()
    }

    fn hash_fields(&self, types: &[Ty], v: &str) -> String {
        let lanes: String = types
            .iter()
            .enumerate()
            .map(|(i, t)| format!(" lanes[{i}] = lt_hash_part({}, &{v}->f{i});", self.desc(t)))
            .collect();
        format!(
            "int64_t lanes[{}]; (void)lanes;{lanes} return lt_hash_tuple(lanes, {});",
            types.len().max(1),
            types.len()
        )
    }

    /// The text of a value with these fields: `Name(f=…, g=…)`, `Name(_0=…)`, or a tuple's
    /// `(…, …)`.
    fn repr_fields(&self, name: Option<&str>, fields: &[Field], v: &str) -> String {
        let mut repr = String::new();
        match name {
            Some(name) => {
                let _ = write!(repr, " lt_buf_puts(b, \"{}(\");", c_string_text(name.as_bytes()));
            }
            None => repr.push_str(" lt_buf_put(b, \"(\", 1);"),
        }
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                repr.push_str(" lt_buf_put(b, \", \", 2);");
            }
            if name.is_some() {
                let label = f.name.clone().unwrap_or_else(|| format!("_{i}"));
                let _ = write!(repr, " lt_buf_puts(b, \"{}=\");", c_string_text(label.as_bytes()));
            }
            let _ = write!(repr, " ({})->repr(b, &{v}->f{i});", self.desc(&f.ty));
        }
        if name.is_none() && fields.len() == 1 {
            repr.push_str(" lt_buf_put(b, \",\", 1);");
        }
        repr.push_str(" lt_buf_put(b, \")\", 1);");
        repr
    }

    /// A value with these fields as a test report shows it: `Name(…)` when every field is
    /// positional, `Name(f=…, _1=…)` when any is named, a tuple's `(…,)`.
    fn show_fields(&self, name: Option<&str>, fields: &[Field], v: &str) -> String {
        let labelled = name.is_some() && fields.iter().any(|f| f.name.is_some());
        let mut show = match name {
            Some(name) => format!(" lt_buf_puts(b, \"{}(\");", c_string_text(name.as_bytes())),
            None => " lt_buf_put(b, \"(\", 1);".to_string(),
        };
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                show.push_str(" lt_buf_put(b, \", \", 2);");
            }
            if labelled {
                let label = f.name.clone().unwrap_or_else(|| format!("_{i}"));
                let _ = write!(show, " lt_buf_puts(b, \"{}=\");", c_string_text(label.as_bytes()));
            }
            let _ = write!(show, " ({})->show(b, &{v}->f{i});", self.desc(&f.ty));
        }
        if name.is_none() && fields.len() == 1 {
            show.push_str(" lt_buf_put(b, \",\", 1);");
        }
        show.push_str(" lt_buf_put(b, \")\", 1);");
        show
    }

    fn tuple_functions(&self, out: &mut String, id: usize, items: &[Ty]) {
        let _ = writeln!(
            out,
            "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}",
            self.each_field(items, "v", "inc")
        );
        let _ = writeln!(
            out,
            "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}",
            self.each_field(items, "v", "dec")
        );
        let _ = writeln!(
            out,
            "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}",
            self.each_field(items, "v", "share")
        );
        let _ = writeln!(
            out,
            "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = a, *y = b; (void)x; (void)y; return {}; }}",
            self.eq_fields(items, "x", "y")
        );
        let _ = writeln!(
            out,
            "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ const lt_t{id} *x = a, *y = b; (void)x; (void)y; (void)at;{} return 0; }}",
            self.cmp_fields(items, "x", "y")
        );
        let _ = writeln!(
            out,
            "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = p; (void)v; {} }}",
            self.hash_fields(items, "v")
        );
        let fields: Vec<Field> = items.iter().map(|t| Field { name: None, ty: t.clone() }).collect();
        let _ = writeln!(
            out,
            "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p;{} }}",
            self.repr_fields(None, &fields, "v")
        );
        let _ = writeln!(
            out,
            "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p;{} }}",
            self.show_fields(None, &fields, "v")
        );
    }

    fn optional_functions(&self, out: &mut String, id: usize, t: &Ty) {
        let d = self.desc(t);
        let held = |call: &str| {
            if self.counted(t) { format!(" if (v->some) ({d})->{call}(&v->value);") } else { String::new() }
        };
        let _ = writeln!(out, "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("inc"));
        let _ = writeln!(out, "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("dec"));
        let _ = writeln!(out, "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("share"));
        let _ = writeln!(
            out,
            "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = a, *y = b; \
             if (!x->some || !y->some) return x->some == y->some; return ({d})->eq(&x->value, &y->value); }}"
        );
        let _ = writeln!(
            out,
            "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ const lt_t{id} *x = a, *y = b; \
             if (!x->some || !y->some) lt_unorderable(at, \"NoneType\"); return ({d})->cmp(&x->value, &y->value, at); }}"
        );
        let _ = writeln!(
            out,
            "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = p; return v->some ? lt_hash_part({d}, &v->value) : (int64_t)0xFCA86420; }}"
        );
        let _ = writeln!(
            out,
            "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p; if (v->some) ({d})->repr(b, &v->value); else lt_buf_puts(b, \"None\"); }}"
        );
        let _ = writeln!(
            out,
            "static void lt_text_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p; if (v->some) ({d})->str(b, &v->value); else lt_buf_puts(b, \"None\"); }}"
        );
        let _ = writeln!(
            out,
            "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p; if (v->some) ({d})->show(b, &v->value); else lt_buf_puts(b, \"None\"); }}"
        );
    }

    fn result_functions(&self, out: &mut String, id: usize, t: &Ty, e: &Ty) {
        let (dt, de) = (self.desc(t), self.desc(e));
        let held = |call: &str| {
            let ok = if self.counted(t) { format!(" if (v->ok) ({dt})->{call}(&v->value);") } else { String::new() };
            let err = if self.counted(e) { format!(" if (!v->ok) ({de})->{call}(&v->error);") } else { String::new() };
            format!("{ok}{err}")
        };
        let _ = writeln!(out, "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("inc"));
        let _ = writeln!(out, "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("dec"));
        let _ = writeln!(out, "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = p; (void)v;{} }}", held("share"));
        let _ = writeln!(
            out,
            "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = a, *y = b; if (x->ok != y->ok) return false; \
             return x->ok ? ({dt})->eq(&x->value, &y->value) : ({de})->eq(&x->error, &y->error); }}"
        );
        let _ = writeln!(
            out,
            "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ (void)a; (void)b; lt_unorderable(at, \"Ok\"); }}"
        );
        let _ = writeln!(
            out,
            "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = p; int64_t lane = v->ok ? lt_hash_part({dt}, &v->value) : lt_hash_part({de}, &v->error); return lt_hash_tuple(&lane, 1); }}"
        );
        let _ = writeln!(
            out,
            "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p; \
             if (v->ok) {{ lt_buf_puts(b, \"Ok(value=\"); ({dt})->repr(b, &v->value); }} \
             else {{ lt_buf_puts(b, \"Err(error=\"); ({de})->repr(b, &v->error); }} lt_buf_put(b, \")\", 1); }}"
        );
        let _ = writeln!(
            out,
            "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = p; \
             if (v->ok) {{ lt_buf_puts(b, \"Ok(\"); ({dt})->show(b, &v->value); }} \
             else {{ lt_buf_puts(b, \"Err(\"); ({de})->show(b, &v->error); }} lt_buf_put(b, \")\", 1); }}"
        );
    }

    fn record_functions(&self, out: &mut String, id: usize, name: &str, fields: &[Field]) {
        let types: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
        let _ =
            writeln!(out, "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v) lt_inc(v); }}");
        let _ = writeln!(
            out,
            "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v == NULL || !lt_dec(v)) return;{} lt_free(v); }}",
            self.each_field(&types, "v", "dec")
        );
        let _ = writeln!(
            out,
            "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v == NULL || lt_count_of(&v->cell) <= 0) return; \
             v->cell.count = -v->cell.count;{} }}",
            self.each_field(&types, "v", "share")
        );
        let _ = writeln!(
            out,
            "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = *(lt_t{id} *const *)a, *y = *(lt_t{id} *const *)b; \
             if (x == y) return true; return {}; }}",
            self.eq_fields(&types, "x", "y")
        );
        let _ = writeln!(
            out,
            "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ const lt_t{id} *x = *(lt_t{id} *const *)a, *y = *(lt_t{id} *const *)b; \
             (void)at;{} return 0; }}",
            self.cmp_fields(&types, "x", "y")
        );
        let _ = writeln!(
            out,
            "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p; {} }}",
            self.hash_fields(&types, "v")
        );
        let _ = writeln!(
            out,
            "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p;{} }}",
            self.repr_fields(Some(name), fields, "v")
        );
        let _ = writeln!(
            out,
            "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p;{} }}",
            self.show_fields(Some(name), fields, "v")
        );
        let _ = writeln!(
            out,
            "static LT_UNUSED void *lt_reuse_t{id}(lt_t{id} **slot, size_t *size) {{ lt_t{id} *v = *slot; \
             if (v != NULL && lt_unique(v)) {{{} *size = sizeof(lt_t{id}); return v; }} lt_dec_t{id}(slot); *size = 0; return NULL; }}",
            self.each_field(&types, "v", "dec")
        );
        let _ = writeln!(
            out,
            "static LT_UNUSED lt_t{id} *lt_own_t{id}(lt_t{id} **slot) {{ if (!lt_unique(*slot)) {{ \
             lt_t{id} *c = lt_alloc(sizeof(lt_t{id})); memcpy((char *)c + sizeof(lt_cell), (char *)*slot + sizeof(lt_cell), \
             sizeof(lt_t{id}) - sizeof(lt_cell));{} lt_dec_t{id}(slot); *slot = c; }} return *slot; }}",
            self.each_field(&types, "c", "inc")
        );
    }

    fn sum_functions(&self, out: &mut String, id: usize, variants: &[(String, Option<Vec<Field>>)]) {
        let cases = |body: &dyn Fn(usize, &[Ty]) -> String| -> String {
            variants
                .iter()
                .enumerate()
                .filter_map(|(k, (_, fields))| {
                    let fields = fields.as_ref()?;
                    let types: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
                    Some(format!(" case {k}: {{ {} break; }}", body(k, &types)))
                })
                .collect()
        };
        let _ =
            writeln!(out, "static void lt_inc_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v) lt_inc(v); }}");
        let dec = cases(&|k, types| {
            format!("lt_t{id}_v{k} *f = (lt_t{id}_v{k} *)v; (void)f;{}", self.each_field(types, "f", "dec"))
        });
        let _ = writeln!(
            out,
            "static void lt_dec_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v == NULL || !lt_dec(v)) return; \
             switch (v->cell.aux) {{{dec} default: break; }} lt_free(v); }}"
        );
        let share = cases(&|k, types| {
            format!("lt_t{id}_v{k} *f = (lt_t{id}_v{k} *)v; (void)f;{}", self.each_field(types, "f", "share"))
        });
        let _ = writeln!(
            out,
            "static void lt_share_t{id}(void *p) {{ lt_t{id} *v = *(lt_t{id} **)p; if (v == NULL || lt_count_of(&v->cell) <= 0) return; \
             v->cell.count = -v->cell.count; switch (v->cell.aux) {{{share} default: break; }} }}"
        );
        let eq = cases(&|k, types| {
            format!(
                "const lt_t{id}_v{k} *fx = (const void *)x, *fy = (const void *)y; (void)fx; (void)fy; return {};",
                self.eq_fields(types, "fx", "fy")
            )
        });
        let _ = writeln!(
            out,
            "static bool lt_eq_t{id}(const void *a, const void *b) {{ const lt_t{id} *x = *(lt_t{id} *const *)a, *y = *(lt_t{id} *const *)b; \
             if (x == y) return true; if (x->cell.aux != y->cell.aux) return false; switch (x->cell.aux) {{{eq} default: break; }} return true; }}"
        );
        let names: Vec<String> = variants.iter().map(|(n, _)| format!("\"{}\"", c_string_text(n.as_bytes()))).collect();
        let units: Vec<String> =
            variants.iter().map(|(_, f)| (if f.is_none() { "1" } else { "0" }).to_string()).collect();
        let cmp = cases(&|k, types| {
            format!(
                "const lt_t{id}_v{k} *fx = (const void *)x, *fy = (const void *)y; (void)fx; (void)fy;{} return 0;",
                self.cmp_fields(types, "fx", "fy")
            )
        });
        let _ = writeln!(
            out,
            "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ static const char *names[] = {{{}}}; static const bool unit[] = {{{}}}; \
             const lt_t{id} *x = *(lt_t{id} *const *)a, *y = *(lt_t{id} *const *)b; \
             if (x->cell.aux != y->cell.aux) {{ if (unit[x->cell.aux] && unit[y->cell.aux]) {{ int c = strcmp(names[x->cell.aux], names[y->cell.aux]); return c < 0 ? -1 : c > 0; }} \
             lt_unorderable(at, names[x->cell.aux]); }} switch (x->cell.aux) {{{cmp} default: break; }} return 0; }}",
            names.join(", "),
            units.join(", ")
        );
        let hash = cases(&|k, types| {
            format!("const lt_t{id}_v{k} *f = (const void *)v; (void)f; {}", self.hash_fields(types, "f"))
        });
        let _ = writeln!(
            out,
            "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p; switch (v->cell.aux) {{{hash} default: break; }} return (int64_t)v->cell.aux; }}"
        );
        let repr: String = variants
            .iter()
            .enumerate()
            .map(|(k, (name, fields))| match fields {
                Some(fields) => format!(
                    " case {k}: {{ const lt_t{id}_v{k} *f = (const void *)v;{} break; }}",
                    self.repr_fields(Some(name), fields, "f")
                ),
                None => format!(" case {k}: lt_buf_puts(b, \"{}\"); break;", c_string_text(name.as_bytes())),
            })
            .collect();
        let _ = writeln!(
            out,
            "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p; switch (v->cell.aux) {{{repr} default: break; }} }}"
        );
        let show: String = variants
            .iter()
            .enumerate()
            .map(|(k, (name, fields))| match fields {
                Some(fields) => format!(
                    " case {k}: {{ const lt_t{id}_v{k} *f = (const void *)v;{} break; }}",
                    self.show_fields(Some(name), fields, "f")
                ),
                None => format!(" case {k}: lt_buf_puts(b, \"{}\"); break;", c_string_text(name.as_bytes())),
            })
            .collect();
        let _ = writeln!(
            out,
            "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = *(lt_t{id} *const *)p; switch (v->cell.aux) {{{show} default: break; }} }}"
        );
        let reuse = cases(&|k, types| {
            format!(
                "lt_t{id}_v{k} *f = (lt_t{id}_v{k} *)v; (void)f;{} *size = sizeof(lt_t{id}_v{k});",
                self.each_field(types, "f", "dec")
            )
        });
        let _ = writeln!(
            out,
            "static LT_UNUSED void *lt_reuse_t{id}(lt_t{id} **slot, size_t *size) {{ lt_t{id} *v = *slot; *size = 0; \
             if (v != NULL && lt_unique(v)) {{ switch (v->cell.aux) {{{reuse} default: break; }} if (*size > 0) return v; }} \
             lt_dec_t{id}(slot); return NULL; }}"
        );
    }
}

/// The functions of a `dyn` type: each one passes the cell to its type's own.
fn dyn_functions(out: &mut String, id: usize, name: &str) {
    let get = format!("lt_t{id} *v = (lt_t{id} *)p; if (v->cell == NULL) return;");
    let _ = writeln!(out, "static void lt_inc_t{id}(void *p) {{ {get} v->v->type->inc(&v->cell); }}");
    let _ = writeln!(out, "static void lt_dec_t{id}(void *p) {{ {get} v->v->type->dec(&v->cell); }}");
    let _ = writeln!(out, "static void lt_share_t{id}(void *p) {{ {get} v->v->type->share(&v->cell); }}");
    let both = format!("const lt_t{id} *x = (const lt_t{id} *)a, *y = (const lt_t{id} *)b;");
    let _ = writeln!(
        out,
        "static bool lt_eq_t{id}(const void *a, const void *b) {{ {both} \
         return x->v->type == y->v->type && x->v->type->eq(&x->cell, &y->cell); }}"
    );
    let _ = writeln!(
        out,
        "static int lt_cmp_t{id}(const void *a, const void *b, const lt_at *at) {{ {both} \
         if (x->v->type != y->v->type) lt_unorderable(at, \"dyn {}\"); return x->v->type->cmp(&x->cell, &y->cell, at); }}",
        c_string_text(name.as_bytes())
    );
    let _ = writeln!(
        out,
        "static int64_t lt_hash_t{id}(const void *p) {{ const lt_t{id} *v = (const lt_t{id} *)p; return v->v->type->hash(&v->cell); }}"
    );
    let _ = writeln!(
        out,
        "static void lt_repr_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = (const lt_t{id} *)p; v->v->type->repr(b, &v->cell); }}"
    );
    let _ = writeln!(
        out,
        "static void lt_show_t{id}(lt_buf *b, const void *p) {{ const lt_t{id} *v = (const lt_t{id} *)p; v->v->type->show(b, &v->cell); }}"
    );
}
