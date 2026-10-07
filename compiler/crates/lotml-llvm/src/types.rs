//! The program's types in LLVM (specs/llvm-parity/design.md, "Data"): a struct per tuple, optional,
//! result and `dyn` type, held by value; a cell per record and sum type, held by pointer; each laid
//! out as C lays it out, since the runtime reads them; and for each, the descriptor and the
//! functions — count, share, compare, hash, write — the runtime's collections call.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use lotml_check::TypeDef;
use lotml_check::ty::{IntKind, Ty};
use lotml_ir::ir::counted;

use crate::module::Module;

/// A field of a struct or cell: how it is named in the text of a value, and its type.
pub(crate) struct Field {
    pub name: Option<String>,
    pub ty: Ty,
}

pub(crate) enum Shape {
    Tuple(Vec<Ty>),
    Optional(Ty),
    Result(Ty, Ty),
    Record(String, Vec<Field>),
    /// Each variant: its name, and its fields, `None` for a variant written without them.
    Sum(Vec<(String, Option<Vec<Field>>)>),
    /// A `dyn` value of the trait: the cell, and the table of its type's methods.
    Dyn(String),
}

/// The slots of the runtime's `lt_type`, in order, after its size.
pub(crate) const INC: usize = 1;
pub(crate) const DEC: usize = 2;
pub(crate) const EQ: usize = 3;
pub(crate) const CMP: usize = 4;
pub(crate) const HASH: usize = 5;
pub(crate) const REPR: usize = 6;
pub(crate) const STR: usize = 7;
pub(crate) const SHARE: usize = 8;
pub(crate) const SHOW: usize = 9;

pub(crate) struct Types<'d> {
    order: Vec<Ty>,
    pub ids: HashMap<Ty, usize>,
    declared: &'d BTreeMap<String, TypeDef>,
    /// How many methods each trait's table holds.
    traits: &'d BTreeMap<String, usize>,
}

fn align_up(n: u64, align: u64) -> u64 {
    n.div_ceil(align) * align
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

    pub fn shape(&self, ty: &Ty) -> Shape {
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

    /// Whether a value of `ty` is a struct held by value.
    pub fn by_value(ty: &Ty) -> bool {
        matches!(ty, Ty::Tuple(_) | Ty::Optional(_) | Ty::Result(..) | Ty::Dyn(_))
    }

    /// The LLVM type a value of `ty` is kept in memory as — a local, a field, an element — laid out
    /// as the runtime's C lays it out: a `bool` is a byte, a unit or a value never made a byte.
    pub fn memory(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(kind) => format!("i{}", int_bits(*kind)),
            Ty::Float(_) => "double".into(),
            Ty::Bool => "i8".into(),
            Ty::Str | Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Func(..) | Ty::Adt(..) => {
                "ptr".into()
            }
            _ if Self::by_value(ty) => format!("%lt_t{}", self.ids[ty]),
            _ => "i8".into(),
        }
    }

    /// The LLVM type of a value of `ty` in a register: a `bool` is an `i1`.
    pub fn value(&self, ty: &Ty) -> String {
        if matches!(ty, Ty::Bool) { "i1".into() } else { self.memory(ty) }
    }

    /// The size and alignment of a value of `ty` in memory, as C gives them.
    pub fn layout(&self, ty: &Ty) -> (u64, u64) {
        match ty {
            Ty::Int(kind) => {
                let bytes = u64::from(int_bits(*kind) / 8);
                (bytes, bytes)
            }
            Ty::Float(_) => (8, 8),
            Ty::Str | Ty::List(_) | Ty::Heap(_) | Ty::Dict(..) | Ty::Set(_) | Ty::Func(..) | Ty::Adt(..) => (8, 8),
            _ if Self::by_value(ty) => self.struct_layout(&self.fields_of(ty)),
            _ => (1, 1),
        }
    }

    /// The layout of a C struct of these fields: its size and alignment.
    pub fn struct_layout(&self, fields: &[Ty]) -> (u64, u64) {
        let (mut size, mut align) = (0, 1);
        for f in fields {
            let (s, a) = self.layout(f);
            size = align_up(size, a) + s;
            align = align.max(a);
        }
        (align_up(size, align), align)
    }

    /// The offset of the field `index` in a C struct of these fields.
    pub fn offset(&self, fields: &[Ty], index: usize) -> u64 {
        let mut offset = 0;
        for (i, f) in fields.iter().enumerate() {
            let (s, a) = self.layout(f);
            offset = align_up(offset, a);
            if i == index {
                return offset;
            }
            offset += s;
        }
        offset
    }

    /// The fields of the struct a value of `ty` is, held by value, in order.
    pub fn fields_of(&self, ty: &Ty) -> Vec<Ty> {
        match self.shape(ty) {
            Shape::Tuple(items) => items,
            Shape::Optional(t) => vec![Ty::Bool, t],
            Shape::Result(t, e) => vec![Ty::Bool, t, e],
            Shape::Dyn(_) => vec![Ty::Str, Ty::Str],
            Shape::Record(..) | Shape::Sum(_) => Vec::new(),
        }
    }

    /// The size of a cell of a record, or of the variant `k` of a sum type: its header and fields.
    pub fn cell_size(&self, fields: &[Ty]) -> u64 {
        let mut all = vec![Ty::Int(IntKind::U32), Ty::Int(IntKind::U32)];
        all.extend(fields.iter().cloned());
        self.struct_layout(&all).0
    }

    /// The descriptor of `ty`, as a global.
    pub fn desc(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int(kind) => format!("@lt_type_{}", int_name(*kind)),
            Ty::Float(_) => "@lt_type_f64".into(),
            Ty::Bool => "@lt_type_bool".into(),
            Ty::Str => "@lt_type_str".into(),
            Ty::List(_) => "@lt_type_list".into(),
            Ty::Heap(_) => "@lt_type_heap".into(),
            Ty::Dict(..) => "@lt_type_dict".into(),
            Ty::Set(_) => "@lt_type_set".into(),
            Ty::Func(..) => "@lt_type_closure".into(),
            _ if self.ids.contains_key(ty) => format!("@lt_type_t{}", self.ids[ty]),
            _ => "@lt_type_none".into(),
        }
    }

    pub fn counted(ty: &Ty) -> bool {
        counted(ty)
    }

    /// The LLVM struct of a record's cell, or of the variant `k` of a sum type's.
    pub fn cell(&self, ty: &Ty, variant: Option<usize>) -> String {
        let id = self.ids[ty];
        match variant {
            Some(k) => format!("%lt_t{id}.v{k}"),
            None => format!("%lt_t{id}.cell"),
        }
    }

    /// The global static cell of the variant `k`, written without fields, of the sum type `ty`.
    pub fn unit_variant(&self, ty: &Ty, k: usize) -> String {
        format!("@lt_t{}.unit{k}", self.ids[ty])
    }

    /// The fields of a record (`variant` None) or of a variant, as their types.
    pub fn cell_fields(&self, ty: &Ty, variant: Option<usize>) -> Vec<Ty> {
        match (self.shape(ty), variant) {
            (Shape::Record(_, fields), _) => fields.into_iter().map(|f| f.ty).collect(),
            (Shape::Sum(variants), Some(k)) => {
                variants.into_iter().nth(k).and_then(|(_, f)| f).unwrap_or_default().into_iter().map(|f| f.ty).collect()
            }
            _ => Vec::new(),
        }
    }

    /// The types, descriptors and functions of the program's types, into `module`.
    pub fn define(&self, module: &mut Module) {
        let mut out = String::new();
        for (name, &n) in self.traits {
            let _ = writeln!(out, "%lt_vt_{name} = type {{ ptr, [{} x ptr] }}", n.max(1));
        }
        for (id, ty) in self.order.iter().enumerate() {
            match self.shape(ty) {
                Shape::Record(_, fields) => {
                    let fields: Vec<String> = fields.iter().map(|f| self.memory(&f.ty)).collect();
                    let tail = if fields.is_empty() { String::new() } else { format!(", {}", fields.join(", ")) };
                    let _ = writeln!(out, "%lt_t{id}.cell = type {{ %lt_cell{tail} }}");
                }
                Shape::Sum(variants) => {
                    for (k, (_, fields)) in variants.iter().enumerate() {
                        match fields {
                            Some(fields) => {
                                let fields: Vec<String> = fields.iter().map(|f| self.memory(&f.ty)).collect();
                                let tail =
                                    if fields.is_empty() { String::new() } else { format!(", {}", fields.join(", ")) };
                                let _ = writeln!(out, "%lt_t{id}.v{k} = type {{ %lt_cell{tail} }}");
                            }
                            None => {
                                let _ =
                                    writeln!(out, "@lt_t{id}.unit{k} = internal global %lt_cell {{ i32 0, i32 {k} }}");
                            }
                        }
                    }
                }
                Shape::Dyn(_) => {
                    let _ = writeln!(out, "%lt_t{id} = type {{ ptr, ptr }}");
                }
                _ => {
                    let fields: Vec<String> = self.fields_of(ty).iter().map(|f| self.memory(f)).collect();
                    let _ = writeln!(out, "%lt_t{id} = type {{ {} }}", fields.join(", "));
                }
            }
        }
        for d in BUILTIN_DESCRIPTORS {
            let _ = writeln!(out, "@{d} = external constant %lt_type");
        }
        for (id, ty) in self.order.iter().enumerate() {
            let size = self.layout(ty).0;
            let text =
                if matches!(ty, Ty::Optional(_)) { format!("@lt_text_t{id}") } else { format!("@lt_repr_t{id}") };
            let _ = writeln!(
                out,
                "@lt_type_t{id} = internal constant %lt_type {{ i64 {size}, ptr @lt_inc_t{id}, ptr @lt_dec_t{id}, ptr @lt_eq_t{id}, \
                 ptr @lt_cmp_t{id}, ptr @lt_hash_t{id}, ptr @lt_repr_t{id}, ptr {text}, ptr @lt_share_t{id}, ptr @lt_show_t{id} }}"
            );
        }
        out.push('\n');
        for (id, ty) in self.order.iter().enumerate() {
            let mut glue = Glue { types: self, module, out: &mut out, id };
            match self.shape(ty) {
                Shape::Tuple(items) => glue.tuple(&items),
                Shape::Optional(t) => glue.optional(&t),
                Shape::Result(t, e) => glue.result(&t, &e),
                Shape::Record(name, fields) => glue.record(ty, &name, &fields),
                Shape::Sum(variants) => glue.sum(ty, &variants),
                Shape::Dyn(name) => glue.dyn_type(&name),
            }
        }
        module.definitions.push_str(&out);
    }
}

/// The runtime's own descriptors, of the types it defines.
const BUILTIN_DESCRIPTORS: [&str; 17] = [
    "lt_type_i8",
    "lt_type_i16",
    "lt_type_i32",
    "lt_type_i64",
    "lt_type_u8",
    "lt_type_u16",
    "lt_type_u32",
    "lt_type_u64",
    "lt_type_f64",
    "lt_type_bool",
    "lt_type_none",
    "lt_type_str",
    "lt_type_list",
    "lt_type_heap",
    "lt_type_dict",
    "lt_type_closure",
    "lt_type_set",
];

pub(crate) fn int_bits(kind: IntKind) -> u32 {
    match kind {
        IntKind::I8 | IntKind::U8 => 8,
        IntKind::I16 | IntKind::U16 => 16,
        IntKind::I32 | IntKind::U32 => 32,
        IntKind::I64 | IntKind::U64 => 64,
    }
}

fn int_name(kind: IntKind) -> &'static str {
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

/// A function of a descriptor's, written as IR.
struct Body {
    lines: Vec<String>,
    fresh: usize,
}

impl Body {
    fn new() -> Body {
        Body { lines: Vec::new(), fresh: 0 }
    }

    fn name(&mut self, prefix: &str) -> String {
        self.fresh += 1;
        format!("{prefix}{}", self.fresh)
    }

    fn line(&mut self, text: impl Into<String>) {
        self.lines.push(format!("  {}", text.into()));
    }

    fn value(&mut self, text: impl AsRef<str>) -> String {
        let r = self.name("%v");
        self.line(format!("{r} = {}", text.as_ref()));
        r
    }

    fn label(&mut self, label: &str) {
        self.lines.push(format!("{label}:"));
    }

    fn finish(self, head: &str) -> String {
        let mut out = format!("{head} {{\nentry:\n");
        for line in self.lines {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("}\n\n");
        out
    }
}

/// The functions of one type's descriptor.
/// What writes the code of one variant of a sum type: given its number and its fields.
type VariantBody<'a, G> = dyn FnMut(&mut G, &mut Body, usize, &[Field]) + 'a;

struct Glue<'g, 'd> {
    types: &'g Types<'d>,
    module: &'g mut Module,
    out: &'g mut String,
    id: usize,
}

impl Glue<'_, '_> {
    /// A call of the descriptor `desc`'s function in `slot`, with `args`: its result, if any.
    fn call(&mut self, b: &mut Body, desc: &str, slot: usize, args: &[String]) -> Option<String> {
        let at = b.value(format!("getelementptr %lt_type, ptr {desc}, i32 0, i32 {slot}"));
        let f = b.value(format!("load ptr, ptr {at}"));
        let args: Vec<String> = args.iter().map(|a| format!("ptr {a}")).collect();
        let args = args.join(", ");
        match slot {
            EQ => Some(b.value(format!("call zeroext i1 {f}({args})"))),
            CMP => Some(b.value(format!("call i32 {f}({args})"))),
            HASH => Some(b.value(format!("call i64 {f}({args})"))),
            _ => {
                b.line(format!("call void {f}({args})"));
                None
            }
        }
    }

    fn runtime(&mut self, b: &mut Body, name: &str, args: &[String]) -> Option<String> {
        let sig = self.module.runtime(name);
        let passed: Vec<String> = sig.params.iter().zip(args).map(|(p, a)| format!("{} {a}", p.llvm())).collect();
        let call = format!("call {} @{name}({})", sig.ret.returned(), passed.join(", "));
        if sig.ret == lotml_runtime::abi::CType::Void {
            b.line(call);
            if sig.noreturn {
                b.line("unreachable");
            }
            None
        } else {
            Some(b.value(call))
        }
    }

    /// The pointer to field `i` of the struct type `ty` that `v` points at.
    fn field(&self, b: &mut Body, ty: &str, v: &str, i: usize) -> String {
        b.value(format!("getelementptr {ty}, ptr {v}, i32 0, i32 {i}"))
    }

    /// `call` (`INC`, `DEC`, `SHARE`) on each counted field of the struct `ty` at `v`; `first` is the
    /// index the fields start at (one past a cell's header).
    fn each_field(&mut self, b: &mut Body, fields: &[Ty], ty: &str, v: &str, first: usize, call: usize) {
        for (i, f) in fields.iter().enumerate() {
            if Types::counted(f) {
                let at = self.field(b, ty, v, first + i);
                let desc = self.types.desc(f);
                self.call(b, &desc, call, &[at]);
            }
        }
    }

    fn put(&mut self, b: &mut Body, buf: &str, text: &str) {
        if text.is_empty() {
            return;
        }
        let global = self.module.text(text.as_bytes());
        self.runtime(b, "lt_buf_put", &[buf.to_string(), global, text.len().to_string()]);
    }

    /// Fields `first..` of the struct `ty` at `x` and at `y` compared equal, with a branch to `no`
    /// when one differs.
    fn eq_fields(&mut self, b: &mut Body, fields: &[Ty], ty: &str, (x, y): (&str, &str), first: usize, no: &str) {
        for (i, f) in fields.iter().enumerate() {
            let (fx, fy) = (self.field(b, ty, x, first + i), self.field(b, ty, y, first + i));
            let desc = self.types.desc(f);
            let same = self.call(b, &desc, EQ, &[fx, fy]).expect("eq returns");
            let next = b.name("next");
            b.line(format!("br i1 {same}, label %{next}, label %{no}"));
            b.label(&next);
        }
    }

    /// The first field of `first..` that differs between `x` and `y`, compared: its order returned.
    fn cmp_fields(&mut self, b: &mut Body, fields: &[Ty], ty: &str, (x, y): (&str, &str), first: usize, at: &str) {
        for (i, f) in fields.iter().enumerate() {
            let (fx, fy) = (self.field(b, ty, x, first + i), self.field(b, ty, y, first + i));
            let desc = self.types.desc(f);
            let same = self.call(b, &desc, EQ, &[fx.clone(), fy.clone()]).expect("eq returns");
            let (differ, next) = (b.name("differ"), b.name("next"));
            b.line(format!("br i1 {same}, label %{next}, label %{differ}"));
            b.label(&differ);
            let order = self.call(b, &desc, CMP, &[fx, fy, at.to_string()]).expect("cmp returns");
            b.line(format!("ret i32 {order}"));
            b.label(&next);
        }
    }

    /// The hash of fields `first..` of the struct `ty` at `v`, as CPython hashes a tuple of them.
    fn hash_fields(&mut self, b: &mut Body, fields: &[Ty], ty: &str, v: &str, first: usize) -> String {
        let lanes = b.value(format!("alloca [{} x i64]", fields.len().max(1)));
        for (i, f) in fields.iter().enumerate() {
            let at = self.field(b, ty, v, first + i);
            let desc = self.types.desc(f);
            let h = self.runtime(b, "lt_hash_part", &[desc, at]).expect("a hash");
            let lane = b.value(format!("getelementptr [{} x i64], ptr {lanes}, i32 0, i32 {i}", fields.len().max(1)));
            b.line(format!("store i64 {h}, ptr {lane}"));
        }
        self.runtime(b, "lt_hash_tuple", &[lanes, fields.len().to_string()]).expect("a hash")
    }

    /// The text of fields `first..` of the struct `ty` at `v`: `Name(f=…, g=…)`, `Name(_0=…)`, or a
    /// tuple's `(…, …)` with `(x,)` for one; `show` writes them as a test report does.
    #[allow(clippy::too_many_arguments)]
    fn text_fields(
        &mut self,
        b: &mut Body,
        buf: &str,
        name: Option<&str>,
        fields: &[Field],
        ty: &str,
        v: &str,
        first: usize,
        show: bool,
    ) {
        let labelled = name.is_some() && (!show || fields.iter().any(|f| f.name.is_some()));
        self.put(
            b,
            buf,
            &match name {
                Some(name) => format!("{name}("),
                None => "(".to_string(),
            },
        );
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                self.put(b, buf, ", ");
            }
            if labelled {
                let label = f.name.clone().unwrap_or_else(|| format!("_{i}"));
                self.put(b, buf, &format!("{label}="));
            }
            let at = self.field(b, ty, v, first + i);
            let desc = self.types.desc(&f.ty);
            self.call(b, &desc, if show { SHOW } else { REPR }, &[buf.to_string(), at]);
        }
        if name.is_none() && fields.len() == 1 {
            self.put(b, buf, ",");
        }
        self.put(b, buf, ")");
    }

    fn emit(&mut self, head: String, body: Body) {
        self.out.push_str(&body.finish(&head));
    }

    fn tuple(&mut self, items: &[Ty]) {
        let id = self.id;
        let ty = format!("%lt_t{id}");
        for (call, name) in [(INC, "inc"), (DEC, "dec"), (SHARE, "share")] {
            let mut b = Body::new();
            self.each_field(&mut b, items, &ty, "%p", 0, call);
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %p)"), b);
        }
        let mut b = Body::new();
        self.eq_fields(&mut b, items, &ty, ("%a", "%b"), 0, "no");
        b.line("ret i1 true");
        b.label("no");
        b.line("ret i1 false");
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        let mut b = Body::new();
        self.cmp_fields(&mut b, items, &ty, ("%a", "%b"), 0, "%at");
        b.line("ret i32 0");
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let h = self.hash_fields(&mut b, items, &ty, "%p", 0);
        b.line(format!("ret i64 {h}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        let fields: Vec<Field> = items.iter().map(|t| Field { name: None, ty: t.clone() }).collect();
        for (show, name) in [(false, "repr"), (true, "show")] {
            let mut b = Body::new();
            self.text_fields(&mut b, "%b", None, &fields, &ty, "%p", 0, show);
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %b, ptr %p)"), b);
        }
    }

    /// Whether the byte flag at field 0 of the struct `ty` at `v` is set.
    fn flag(&self, b: &mut Body, ty: &str, v: &str) -> String {
        let at = self.field(b, ty, v, 0);
        let byte = b.value(format!("load i8, ptr {at}"));
        b.value(format!("icmp ne i8 {byte}, 0"))
    }

    fn optional(&mut self, t: &Ty) {
        let id = self.id;
        let ty = format!("%lt_t{id}");
        let d = self.types.desc(t);
        for (call, name) in [(INC, "inc"), (DEC, "dec"), (SHARE, "share")] {
            let mut b = Body::new();
            if Types::counted(t) {
                let some = self.flag(&mut b, &ty, "%p");
                b.line("br i1 SOME, label %held, label %done".replace("SOME", &some));
                b.label("held");
                let value = self.field(&mut b, &ty, "%p", 1);
                self.call(&mut b, &d, call, &[value]);
                b.line("br label %done");
                b.label("done");
            }
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %p)"), b);
        }
        let mut b = Body::new();
        let (sx, sy) = (self.flag(&mut b, &ty, "%a"), self.flag(&mut b, &ty, "%b"));
        let both = b.value(format!("and i1 {sx}, {sy}"));
        b.line(format!("br i1 {both}, label %values, label %flags"));
        b.label("flags");
        let same = b.value(format!("icmp eq i1 {sx}, {sy}"));
        b.line(format!("ret i1 {same}"));
        b.label("values");
        let (vx, vy) = (self.field(&mut b, &ty, "%a", 1), self.field(&mut b, &ty, "%b", 1));
        let eq = self.call(&mut b, &d, EQ, &[vx, vy]).expect("eq returns");
        b.line(format!("ret i1 {eq}"));
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        let mut b = Body::new();
        let (sx, sy) = (self.flag(&mut b, &ty, "%a"), self.flag(&mut b, &ty, "%b"));
        let both = b.value(format!("and i1 {sx}, {sy}"));
        b.line(format!("br i1 {both}, label %values, label %none"));
        b.label("none");
        let name = self.module.text_z("NoneType");
        self.runtime(&mut b, "lt_unorderable", &["%at".into(), name]);
        b.label("values");
        let (vx, vy) = (self.field(&mut b, &ty, "%a", 1), self.field(&mut b, &ty, "%b", 1));
        let order = self.call(&mut b, &d, CMP, &[vx, vy, "%at".into()]).expect("cmp returns");
        b.line(format!("ret i32 {order}"));
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let some = self.flag(&mut b, &ty, "%p");
        b.line(format!("br i1 {some}, label %value, label %none"));
        b.label("none");
        b.line(format!("ret i64 {}", 0xFCA8_6420_i64));
        b.label("value");
        let value = self.field(&mut b, &ty, "%p", 1);
        let h = self.runtime(&mut b, "lt_hash_part", &[d.clone(), value]).expect("a hash");
        b.line(format!("ret i64 {h}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        for (slot, name) in [(REPR, "repr"), (STR, "text"), (SHOW, "show")] {
            let mut b = Body::new();
            let some = self.flag(&mut b, &ty, "%p");
            b.line(format!("br i1 {some}, label %value, label %none"));
            b.label("value");
            let value = self.field(&mut b, &ty, "%p", 1);
            self.call(&mut b, &d, slot, &["%b".into(), value]);
            b.line("ret void");
            b.label("none");
            self.put(&mut b, "%b", "None");
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %b, ptr %p)"), b);
        }
    }

    fn result(&mut self, t: &Ty, e: &Ty) {
        let id = self.id;
        let ty = format!("%lt_t{id}");
        let (dt, de) = (self.types.desc(t), self.types.desc(e));
        for (call, name) in [(INC, "inc"), (DEC, "dec"), (SHARE, "share")] {
            let mut b = Body::new();
            let ok = self.flag(&mut b, &ty, "%p");
            b.line(format!("br i1 {ok}, label %okay, label %error"));
            b.label("okay");
            if Types::counted(t) {
                let value = self.field(&mut b, &ty, "%p", 1);
                self.call(&mut b, &dt, call, &[value]);
            }
            b.line("ret void");
            b.label("error");
            if Types::counted(e) {
                let error = self.field(&mut b, &ty, "%p", 2);
                self.call(&mut b, &de, call, &[error]);
            }
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %p)"), b);
        }
        let mut b = Body::new();
        let (ox, oy) = (self.flag(&mut b, &ty, "%a"), self.flag(&mut b, &ty, "%b"));
        let same = b.value(format!("icmp eq i1 {ox}, {oy}"));
        b.line(format!("br i1 {same}, label %alike, label %no"));
        b.label("no");
        b.line("ret i1 false");
        b.label("alike");
        b.line(format!("br i1 {ox}, label %okay, label %error"));
        b.label("okay");
        let (vx, vy) = (self.field(&mut b, &ty, "%a", 1), self.field(&mut b, &ty, "%b", 1));
        let eq = self.call(&mut b, &dt, EQ, &[vx, vy]).expect("eq returns");
        b.line(format!("ret i1 {eq}"));
        b.label("error");
        let (ex, ey) = (self.field(&mut b, &ty, "%a", 2), self.field(&mut b, &ty, "%b", 2));
        let eq = self.call(&mut b, &de, EQ, &[ex, ey]).expect("eq returns");
        b.line(format!("ret i1 {eq}"));
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        let mut b = Body::new();
        let name = self.module.text_z("Ok");
        self.runtime(&mut b, "lt_unorderable", &["%at".into(), name]);
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let ok = self.flag(&mut b, &ty, "%p");
        let lane = b.value("alloca i64");
        b.line(format!("br i1 {ok}, label %okay, label %error"));
        b.label("okay");
        let value = self.field(&mut b, &ty, "%p", 1);
        let h = self.runtime(&mut b, "lt_hash_part", &[dt.clone(), value]).expect("a hash");
        b.line(format!("store i64 {h}, ptr {lane}"));
        b.line("br label %done");
        b.label("error");
        let error = self.field(&mut b, &ty, "%p", 2);
        let h = self.runtime(&mut b, "lt_hash_part", &[de.clone(), error]).expect("a hash");
        b.line(format!("store i64 {h}, ptr {lane}"));
        b.line("br label %done");
        b.label("done");
        let h = self.runtime(&mut b, "lt_hash_tuple", &[lane, "1".into()]).expect("a hash");
        b.line(format!("ret i64 {h}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        for (slot, name, ok_text, err_text) in
            [(REPR, "repr", "Ok(value=", "Err(error="), (SHOW, "show", "Ok(", "Err(")]
        {
            let mut b = Body::new();
            let ok = self.flag(&mut b, &ty, "%p");
            b.line(format!("br i1 {ok}, label %okay, label %error"));
            b.label("okay");
            self.put(&mut b, "%b", ok_text);
            let value = self.field(&mut b, &ty, "%p", 1);
            self.call(&mut b, &dt, slot, &["%b".into(), value]);
            b.line("br label %done");
            b.label("error");
            self.put(&mut b, "%b", err_text);
            let error = self.field(&mut b, &ty, "%p", 2);
            self.call(&mut b, &de, slot, &["%b".into(), error]);
            b.line("br label %done");
            b.label("done");
            self.put(&mut b, "%b", ")");
            b.line("ret void");
            self.emit(format!("define internal void @lt_{name}_t{id}(ptr %b, ptr %p)"), b);
        }
    }

    /// The cell a descriptor function's argument points at: `*(lt_tN **)p`.
    fn cell_at(&self, b: &mut Body, p: &str) -> String {
        b.value(format!("load ptr, ptr {p}"))
    }

    fn record(&mut self, full: &Ty, name: &str, fields: &[Field]) {
        let id = self.id;
        let cell = self.types.cell(full, None);
        let types: Vec<Ty> = fields.iter().map(|f| f.ty.clone()).collect();
        let size = self.types.cell_size(&types);
        self.cell_inc(id);
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%p");
        self.drop_if_last(&mut b, &v);
        self.each_field(&mut b, &types, &cell, &v, 1, DEC);
        self.runtime(&mut b, "lt_free", &[v]);
        b.line("ret void");
        b.label("keep");
        b.line("ret void");
        self.emit(format!("define internal void @lt_dec_t{id}(ptr %p)"), b);
        let mut b = Body::new();
        let v = self.share_cell(&mut b);
        self.each_field(&mut b, &types, &cell, &v, 1, SHARE);
        b.line("ret void");
        b.label("keep");
        b.line("ret void");
        self.emit(format!("define internal void @lt_share_t{id}(ptr %p)"), b);
        let mut b = Body::new();
        let (x, y) = (self.cell_at(&mut b, "%a"), self.cell_at(&mut b, "%b"));
        let same = b.value(format!("icmp eq ptr {x}, {y}"));
        b.line(format!("br i1 {same}, label %yes, label %fields"));
        b.label("fields");
        self.eq_fields(&mut b, &types, &cell, (&x, &y), 1, "no");
        b.line("br label %yes");
        b.label("yes");
        b.line("ret i1 true");
        b.label("no");
        b.line("ret i1 false");
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        let mut b = Body::new();
        let (x, y) = (self.cell_at(&mut b, "%a"), self.cell_at(&mut b, "%b"));
        self.cmp_fields(&mut b, &types, &cell, (&x, &y), 1, "%at");
        b.line("ret i32 0");
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%p");
        let h = self.hash_fields(&mut b, &types, &cell, &v, 1);
        b.line(format!("ret i64 {h}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        for (show, fname) in [(false, "repr"), (true, "show")] {
            let mut b = Body::new();
            let v = self.cell_at(&mut b, "%p");
            self.text_fields(&mut b, "%b", Some(name), fields, &cell, &v, 1, show);
            b.line("ret void");
            self.emit(format!("define internal void @lt_{fname}_t{id}(ptr %b, ptr %p)"), b);
        }
        // Reuse: a cell held only here gives its memory to a constructor further on.
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%slot");
        let unique = self.unique_cell(&mut b, &v);
        b.line(format!("br i1 {unique}, label %mine, label %shared"));
        b.label("mine");
        self.each_field(&mut b, &types, &cell, &v, 1, DEC);
        b.line(format!("store i64 {size}, ptr %size"));
        b.line(format!("ret ptr {v}"));
        b.label("shared");
        b.line(format!("call void @lt_dec_t{id}(ptr %slot)"));
        b.line("store i64 0, ptr %size");
        b.line("ret ptr null");
        self.emit(format!("define internal ptr @lt_reuse_t{id}(ptr %slot, ptr %size)"), b);
        // Ownership: the cell in `slot` made its own before a field of it changes.
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%slot");
        let unique = self.unique_cell(&mut b, &v);
        b.line(format!("br i1 {unique}, label %mine, label %copy"));
        b.label("copy");
        let c = self.runtime(&mut b, "lt_alloc", &[size.to_string()]).expect("a cell");
        let (to, from) = (
            b.value(format!("getelementptr i8, ptr {c}, i64 8")),
            b.value(format!("getelementptr i8, ptr {v}, i64 8")),
        );
        self.module.declare("declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)");
        b.line(format!("call void @llvm.memcpy.p0.p0.i64(ptr {to}, ptr {from}, i64 {}, i1 false)", size - 8));
        self.each_field(&mut b, &types, &cell, &c, 1, INC);
        b.line(format!("call void @lt_dec_t{id}(ptr %slot)"));
        b.line(format!("store ptr {c}, ptr %slot"));
        b.line(format!("ret ptr {c}"));
        b.label("mine");
        b.line(format!("ret ptr {v}"));
        self.emit(format!("define internal ptr @lt_own_t{id}(ptr %slot)"), b);
    }

    /// Whether the cell `v`, not null, holds a count of one.
    fn unique_cell(&mut self, b: &mut Body, v: &str) -> String {
        let null = b.value(format!("icmp eq ptr {v}, null"));
        let (check, out) = (b.name("check"), b.name("checked"));
        let start = b.name("from");
        b.line(format!("br label %{start}"));
        b.label(&start);
        b.line(format!("br i1 {null}, label %{out}, label %{check}"));
        b.label(&check);
        let unique = self.runtime(b, "lt_unique", &[v.to_string()]).expect("a flag");
        b.line(format!("br label %{out}"));
        b.label(&out);
        b.value(format!("phi i1 [ false, %{start} ], [ {unique}, %{check} ]"))
    }

    /// `lt_inc` of the cell in `p`, when there is one.
    fn cell_inc(&mut self, id: usize) {
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%p");
        let null = b.value(format!("icmp eq ptr {v}, null"));
        b.line(format!("br i1 {null}, label %done, label %held"));
        b.label("held");
        self.runtime(&mut b, "lt_inc", &[v]);
        b.line("br label %done");
        b.label("done");
        b.line("ret void");
        self.emit(format!("define internal void @lt_inc_t{id}(ptr %p)"), b);
    }

    /// Continue only when `v` is a cell whose last count this was; else branch to `keep`.
    fn drop_if_last(&mut self, b: &mut Body, v: &str) {
        let null = b.value(format!("icmp eq ptr {v}, null"));
        b.line(format!("br i1 {null}, label %keep, label %held"));
        b.label("held");
        let last = self.runtime(b, "lt_dec", &[v.to_string()]).expect("a flag");
        b.line(format!("br i1 {last}, label %free, label %keep"));
        b.label("free");
    }

    /// The cell of `p` marked shared, continuing only when it was private; else branch to `keep`.
    fn share_cell(&mut self, b: &mut Body) -> String {
        let v = self.cell_at(b, "%p");
        let null = b.value(format!("icmp eq ptr {v}, null"));
        b.line(format!("br i1 {null}, label %keep, label %held"));
        b.label("held");
        let count = self.runtime(b, "lt_count_of", std::slice::from_ref(&v)).expect("a count");
        let private = b.value(format!("icmp sgt i32 {count}, 0"));
        b.line(format!("br i1 {private}, label %mark, label %keep"));
        b.label("mark");
        let negated = b.value(format!("sub i32 0, {count}"));
        let at = b.value(format!("getelementptr %lt_cell, ptr {v}, i32 0, i32 0"));
        b.line(format!("store i32 {negated}, ptr {at}"));
        v
    }

    /// The tag of the sum value `v`: its cell's `aux`.
    fn tag(&self, b: &mut Body, v: &str) -> String {
        let at = b.value(format!("getelementptr %lt_cell, ptr {v}, i32 0, i32 1"));
        b.value(format!("load i32, ptr {at}"))
    }

    /// A `switch` on the tag of `v` to a block per variant that has fields, writing each with `f`.
    fn by_variant(
        &mut self,
        b: &mut Body,
        tag: &str,
        variants: &[(String, Option<Vec<Field>>)],
        f: &mut VariantBody<'_, Self>,
    ) {
        let cases: String = variants
            .iter()
            .enumerate()
            .filter(|(_, (_, fields))| fields.is_some())
            .map(|(k, _)| format!(" i32 {k}, label %variant{k}"))
            .collect();
        b.line(format!("switch i32 {tag}, label %other [{cases} ]"));
        for (k, (_, fields)) in variants.iter().enumerate() {
            if let Some(fields) = fields {
                b.label(&format!("variant{k}"));
                f(self, b, k, fields);
            }
        }
        b.label("other");
    }

    fn sum(&mut self, full: &Ty, variants: &[(String, Option<Vec<Field>>)]) {
        let id = self.id;
        self.cell_inc(id);
        let types_of = |fields: &[Field]| -> Vec<Ty> { fields.iter().map(|f| f.ty.clone()).collect() };
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%p");
        self.drop_if_last(&mut b, &v);
        let tag = self.tag(&mut b, &v);
        self.by_variant(&mut b, &tag, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            g.each_field(b, &types_of(fields), &cell, &v, 1, DEC);
            b.line("br label %other");
        });
        self.runtime(&mut b, "lt_free", std::slice::from_ref(&v));
        b.line("ret void");
        b.label("keep");
        b.line("ret void");
        self.emit(format!("define internal void @lt_dec_t{id}(ptr %p)"), b);
        let mut b = Body::new();
        let v = self.share_cell(&mut b);
        let tag = self.tag(&mut b, &v);
        self.by_variant(&mut b, &tag, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            g.each_field(b, &types_of(fields), &cell, &v, 1, SHARE);
            b.line("br label %other");
        });
        b.line("ret void");
        b.label("keep");
        b.line("ret void");
        self.emit(format!("define internal void @lt_share_t{id}(ptr %p)"), b);
        let mut b = Body::new();
        let (x, y) = (self.cell_at(&mut b, "%a"), self.cell_at(&mut b, "%b"));
        let same = b.value(format!("icmp eq ptr {x}, {y}"));
        b.line(format!("br i1 {same}, label %yes, label %tags"));
        b.label("tags");
        let (tx, ty) = (self.tag(&mut b, &x), self.tag(&mut b, &y));
        let alike = b.value(format!("icmp eq i32 {tx}, {ty}"));
        b.line(format!("br i1 {alike}, label %fields, label %no"));
        b.label("fields");
        self.by_variant(&mut b, &tx, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            g.eq_fields(b, &types_of(fields), &cell, (&x, &y), 1, "no");
            b.line("br label %yes");
        });
        b.line("br label %yes");
        b.label("yes");
        b.line("ret i1 true");
        b.label("no");
        b.line("ret i1 false");
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        // Order: variants without fields by name, as CPython orders their strings; any other pair
        // of different variants is unorderable; one variant by its fields.
        let names: Vec<String> = variants.iter().map(|(n, _)| self.module.text_z(n)).collect();
        let mut b = Body::new();
        let (x, y) = (self.cell_at(&mut b, "%a"), self.cell_at(&mut b, "%b"));
        let (tx, ty) = (self.tag(&mut b, &x), self.tag(&mut b, &y));
        let alike = b.value(format!("icmp eq i32 {tx}, {ty}"));
        b.line(format!("br i1 {alike}, label %fields, label %differ"));
        b.label("differ");
        let units: Vec<String> =
            variants.iter().map(|(_, f)| (if f.is_none() { "1" } else { "0" }).to_string()).collect();
        let unit_table = self.module.text(&units.iter().map(|u| u.parse::<u8>().unwrap_or(0)).collect::<Vec<u8>>());
        let name_table = format!("@lt_names_t{id}");
        let _ = writeln!(
            self.out,
            "{name_table} = internal constant [{} x ptr] [{}]",
            names.len(),
            names.iter().map(|n| format!("ptr {n}")).collect::<Vec<_>>().join(", ")
        );
        let (ux, uy) = (
            b.value(format!("getelementptr i8, ptr {unit_table}, i32 {tx}")),
            b.value(format!("getelementptr i8, ptr {unit_table}, i32 {ty}")),
        );
        let (bx, by) = (b.value(format!("load i8, ptr {ux}")), b.value(format!("load i8, ptr {uy}")));
        let both = b.value(format!("and i8 {bx}, {by}"));
        let both = b.value(format!("icmp ne i8 {both}, 0"));
        let (nx_at, ny_at) = (
            b.value(format!("getelementptr [{} x ptr], ptr {name_table}, i32 0, i32 {tx}", names.len())),
            b.value(format!("getelementptr [{} x ptr], ptr {name_table}, i32 0, i32 {ty}", names.len())),
        );
        let (nx, ny) = (b.value(format!("load ptr, ptr {nx_at}")), b.value(format!("load ptr, ptr {ny_at}")));
        b.line(format!("br i1 {both}, label %names, label %unorderable"));
        b.label("unorderable");
        self.runtime(&mut b, "lt_unorderable", &["%at".into(), nx.clone()]);
        b.label("names");
        self.module.declare("declare i32 @strcmp(ptr, ptr)");
        let c = b.value(format!("call i32 @strcmp(ptr {nx}, ptr {ny})"));
        let below = b.value(format!("icmp slt i32 {c}, 0"));
        let above = b.value(format!("icmp sgt i32 {c}, 0"));
        let up = b.value(format!("zext i1 {above} to i32"));
        let order = b.value(format!("select i1 {below}, i32 -1, i32 {up}"));
        b.line(format!("ret i32 {order}"));
        b.label("fields");
        self.by_variant(&mut b, &tx, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            g.cmp_fields(b, &types_of(fields), &cell, (&x, &y), 1, "%at");
            b.line("ret i32 0");
        });
        b.line("ret i32 0");
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%p");
        let tag = self.tag(&mut b, &v);
        self.by_variant(&mut b, &tag, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            let h = g.hash_fields(b, &types_of(fields), &cell, &v, 1);
            b.line(format!("ret i64 {h}"));
        });
        let wide = b.value(format!("zext i32 {tag} to i64"));
        b.line(format!("ret i64 {wide}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        for (show, fname) in [(false, "repr"), (true, "show")] {
            let mut b = Body::new();
            let v = self.cell_at(&mut b, "%p");
            let tag = self.tag(&mut b, &v);
            let cases: String = (0..variants.len()).map(|k| format!(" i32 {k}, label %variant{k}")).collect();
            b.line(format!("switch i32 {tag}, label %other [{cases} ]"));
            for (k, (name, fields)) in variants.iter().enumerate() {
                b.label(&format!("variant{k}"));
                match fields {
                    Some(fields) => {
                        let cell = self.types.cell(full, Some(k));
                        self.text_fields(&mut b, "%b", Some(name), fields, &cell, &v, 1, show);
                    }
                    None => self.put(&mut b, "%b", name),
                }
                b.line("ret void");
            }
            b.label("other");
            b.line("ret void");
            self.emit(format!("define internal void @lt_{fname}_t{id}(ptr %b, ptr %p)"), b);
        }
        let mut b = Body::new();
        let v = self.cell_at(&mut b, "%slot");
        let unique = self.unique_cell(&mut b, &v);
        b.line(format!("br i1 {unique}, label %mine, label %shared"));
        b.label("mine");
        let tag = self.tag(&mut b, &v);
        self.by_variant(&mut b, &tag, variants, &mut |g, b, k, fields| {
            let cell = g.types.cell(full, Some(k));
            let types = types_of(fields);
            g.each_field(b, &types, &cell, &v, 1, DEC);
            b.line(format!("store i64 {}, ptr %size", g.types.cell_size(&types)));
            b.line(format!("ret ptr {v}"));
        });
        b.line("br label %shared");
        b.label("shared");
        b.line(format!("call void @lt_dec_t{id}(ptr %slot)"));
        b.line("store i64 0, ptr %size");
        b.line("ret ptr null");
        self.emit(format!("define internal ptr @lt_reuse_t{id}(ptr %slot, ptr %size)"), b);
    }

    fn dyn_type(&mut self, name: &str) {
        let id = self.id;
        let ty = format!("%lt_t{id}");
        // The descriptor of the value's own type: `v->v->type`.
        let own = |g: &mut Self, b: &mut Body, v: &str| -> String {
            let vt_at = g.field(b, &ty, v, 1);
            let vt = b.value(format!("load ptr, ptr {vt_at}"));
            b.value(format!("load ptr, ptr {vt}"))
        };
        for (call, fname) in [(INC, "inc"), (DEC, "dec"), (SHARE, "share")] {
            let mut b = Body::new();
            let cell = b.value("load ptr, ptr %p");
            let null = b.value(format!("icmp eq ptr {cell}, null"));
            b.line(format!("br i1 {null}, label %done, label %held"));
            b.label("held");
            let d = own(self, &mut b, "%p");
            self.call(&mut b, &d, call, &["%p".into()]);
            b.line("br label %done");
            b.label("done");
            b.line("ret void");
            self.emit(format!("define internal void @lt_{fname}_t{id}(ptr %p)"), b);
        }
        let mut b = Body::new();
        let (dx, dy) = (own(self, &mut b, "%a"), own(self, &mut b, "%b"));
        let same = b.value(format!("icmp eq ptr {dx}, {dy}"));
        b.line(format!("br i1 {same}, label %cells, label %no"));
        b.label("no");
        b.line("ret i1 false");
        b.label("cells");
        let eq = self.call(&mut b, &dx, EQ, &["%a".into(), "%b".into()]).expect("eq returns");
        b.line(format!("ret i1 {eq}"));
        self.emit(format!("define internal zeroext i1 @lt_eq_t{id}(ptr %a, ptr %b)"), b);
        let mut b = Body::new();
        let (dx, dy) = (own(self, &mut b, "%a"), own(self, &mut b, "%b"));
        let same = b.value(format!("icmp eq ptr {dx}, {dy}"));
        b.line(format!("br i1 {same}, label %cells, label %differ"));
        b.label("differ");
        let label = self.module.text_z(&format!("dyn {name}"));
        self.runtime(&mut b, "lt_unorderable", &["%at".into(), label]);
        b.label("cells");
        let order = self.call(&mut b, &dx, CMP, &["%a".into(), "%b".into(), "%at".into()]).expect("cmp returns");
        b.line(format!("ret i32 {order}"));
        self.emit(format!("define internal i32 @lt_cmp_t{id}(ptr %a, ptr %b, ptr %at)"), b);
        let mut b = Body::new();
        let d = own(self, &mut b, "%p");
        let h = self.call(&mut b, &d, HASH, &["%p".into()]).expect("a hash");
        b.line(format!("ret i64 {h}"));
        self.emit(format!("define internal i64 @lt_hash_t{id}(ptr %p)"), b);
        for (slot, fname) in [(REPR, "repr"), (SHOW, "show")] {
            let mut b = Body::new();
            let d = own(self, &mut b, "%p");
            self.call(&mut b, &d, slot, &["%b".into(), "%p".into()]);
            b.line("ret void");
            self.emit(format!("define internal void @lt_{fname}_t{id}(ptr %b, ptr %p)"), b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types() -> Types<'static> {
        static DECLARED: std::sync::OnceLock<BTreeMap<String, TypeDef>> = std::sync::OnceLock::new();
        static TRAITS: std::sync::OnceLock<BTreeMap<String, usize>> = std::sync::OnceLock::new();
        Types::new(DECLARED.get_or_init(BTreeMap::new), TRAITS.get_or_init(BTreeMap::new))
    }

    #[test]
    fn a_struct_is_laid_out_as_c_lays_it_out() {
        let t = types();
        let (b, i8_, i64_) = (Ty::Bool, Ty::Int(IntKind::I8), Ty::Int(IntKind::I64));
        assert_eq!(t.struct_layout(&[b.clone(), i64_.clone()]), (16, 8));
        assert_eq!(t.offset(&[b.clone(), i64_.clone()], 1), 8);
        assert_eq!(t.struct_layout(&[i8_.clone(), b.clone()]), (2, 1));
        assert_eq!(t.struct_layout(&[Ty::Int(IntKind::I32), b.clone(), Ty::Int(IntKind::I16)]), (8, 4));
        assert_eq!(t.offset(&[Ty::Int(IntKind::I32), b, Ty::Int(IntKind::I16)], 2), 6);
        assert_eq!(t.cell_size(&[i64_]), 16, "a header of two i32, then the field");
    }
}
