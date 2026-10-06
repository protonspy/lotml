//! The compiler as an editor and an agent query it: a workspace of files kept in the incremental
//! engine, and the questions asked of it — diagnostics, where a name is declared, every place it
//! is used, its type, and a file's outline — with the edits made through it: symbol-addressed
//! replacements and atomic renames. The language server and the MCP server are two transports
//! over these.
//!
//! Every file is a module of its own, so a name refers to something declared in the same file.

pub mod edit;
pub mod lines;
pub mod mutate;
pub mod symbols;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use lotml_db::{Database, SourceFile, checked, diagnostics, parse};
use lotml_diag::Diagnostic;
use lotml_syntax::ast::{FnDef, Item, Module};
use lotml_syntax::span::Span;
use salsa::Setter;

pub use crate::edit::Refused;
pub use crate::symbols::{Occurrence, Symbol};

/// A rename worked out but not yet written: the file's new text, where the new name stands, and
/// the places the old name is still written that no reference resolves to — comments, strings,
/// other files — which a rename is a textual operation for, and a person or agent decides on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Renamed {
    pub text: String,
    pub sites: Vec<Span>,
    pub mentions: Vec<(PathBuf, Span)>,
}

/// Every name in a file and what it refers to, in source order.
#[salsa::tracked(returns(ref))]
pub fn index(db: &dyn salsa::Database, file: SourceFile) -> Vec<Occurrence> {
    symbols::occurrences(&parse(db, file).module, checked(db, file))
}

/// The files an editor or agent works on, each kept as an input of the incremental engine, so a
/// question about a file that did not change is answered from memory.
#[derive(Default)]
pub struct Workspace {
    db: Database,
    files: BTreeMap<PathBuf, SourceFile>,
}

/// A declaration in a file's outline, with what it declares inside.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outline {
    pub name: String,
    pub kind: Kind,
    /// The whole declaration.
    pub span: Span,
    /// Its name.
    pub name_span: Span,
    pub children: Vec<Outline>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Function,
    Record,
    Sum,
    Variant,
    Field,
    Trait,
    Impl,
    Method,
    Test,
}

impl Workspace {
    pub fn new() -> Workspace {
        Workspace::default()
    }

    /// Add a file, or give it new text; true when its text changed.
    pub fn set(&mut self, path: &Path, text: String) -> bool {
        match self.files.get(path) {
            Some(&file) if *file.text(&self.db) == text => false,
            Some(&file) => {
                file.set_text(&mut self.db).to(text);
                true
            }
            None => {
                let file = SourceFile::new(&self.db, path.display().to_string(), text, Vec::new());
                self.files.insert(path.to_path_buf(), file);
                true
            }
        }
    }

    /// Give a file the interfaces of the Python modules it may import, each module's name with
    /// its `.lotmli` text; true when they changed.
    pub fn set_interfaces(&mut self, path: &Path, interfaces: Vec<(String, String)>) -> bool {
        match self.files.get(path) {
            Some(&file) if *file.interfaces(&self.db) != interfaces => {
                file.set_interfaces(&mut self.db).to(interfaces);
                true
            }
            _ => false,
        }
    }

    /// What `text` would check to as the file at `path`, with the file's interfaces.
    fn check_as(&self, path: &Path, text: &str) -> (lotml_syntax::Parsed, lotml_check::Checked) {
        let interfaces = self.files.get(path).map(|&f| lotml_db::interfaces(&self.db, f)).unwrap_or_default();
        let parsed = lotml_syntax::parse(text);
        let checked = lotml_check::check_resolved_with(&parsed.module, text, &interfaces);
        (parsed, checked)
    }

    /// Forget a file; true when it was there.
    pub fn remove(&mut self, path: &Path) -> bool {
        self.files.remove(path).is_some()
    }

    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }

    pub fn text(&self, path: &Path) -> Option<&str> {
        self.files.get(path).map(|&f| f.text(&self.db).as_str())
    }

    /// Answer every query once for every file, so the first question an editor or agent asks
    /// is answered from memory rather than from a cold index.
    pub fn warm(&self) {
        for &file in self.files.values() {
            diagnostics(&self.db, file);
            index(&self.db, file);
        }
    }

    /// A file's diagnostics, root causes first as `check` orders them within a file.
    pub fn diagnostics(&self, path: &Path) -> &[Diagnostic] {
        self.files.get(path).map_or(&[], |&f| diagnostics(&self.db, f).as_slice())
    }

    /// Every name in a file and what it refers to.
    pub fn occurrences(&self, path: &Path) -> &[Occurrence] {
        self.files.get(path).map_or(&[], |&f| index(&self.db, f).as_slice())
    }

    fn module(&self, path: &Path) -> Option<&Module> {
        self.files.get(path).map(|&f| &parse(&self.db, f).module)
    }

    /// The name at a byte offset: the one the offset is inside, or else one ending there, as a
    /// cursor just after a name points at it.
    pub fn at(&self, path: &Path, offset: u32) -> Option<&Occurrence> {
        let found = self.occurrences(path);
        found
            .iter()
            .find(|o| o.span.start <= offset && offset < o.span.end)
            .or_else(|| found.iter().find(|o| o.span.end == offset))
    }

    /// Where a symbol of a file is declared.
    pub fn declarations(&self, path: &Path, symbol: &Symbol) -> Vec<Span> {
        self.occurrences(path).iter().filter(|o| o.declaration && o.symbol == *symbol).map(|o| o.span).collect()
    }

    /// Every place a symbol of a file is named, its declarations included or not.
    pub fn references(&self, path: &Path, symbol: &Symbol, declarations: bool) -> Vec<Span> {
        self.occurrences(path)
            .iter()
            .filter(|o| o.symbol == *symbol && (declarations || !o.declaration))
            .map(|o| o.span)
            .collect()
    }

    /// The symbols declared in the workspace under a name a reader would use: `area`, `Shape`,
    /// `Circle`, or `Counter.get` for a field or method. Locals have no such name.
    pub fn find(&self, name: &str) -> Vec<(PathBuf, Symbol)> {
        let wanted: Vec<Symbol> = match name.split_once('.') {
            Some((owner, member)) => vec![Symbol::Member(owner.to_string(), member.to_string())],
            None => vec![Symbol::Item(name.to_string()), Symbol::Variant(name.to_string())],
        };
        let mut found = Vec::new();
        for path in self.files.keys() {
            for symbol in &wanted {
                if self.occurrences(path).iter().any(|o| o.declaration && o.symbol == *symbol) {
                    found.push((path.clone(), symbol.clone()));
                }
            }
        }
        found
    }

    /// What to show for the name at an offset: a local's type, or a declaration's signature.
    pub fn hover(&self, path: &Path, offset: u32) -> Option<(Span, String)> {
        let occurrence = self.at(path, offset)?;
        let text = self.text(path)?;
        let module = self.module(path)?;
        let shown = match &occurrence.symbol {
            Symbol::Local(declared) => {
                let name = &text[declared.range()];
                match self.local_type(path, &occurrence.symbol) {
                    Some(ty) => format!("{name}: {ty}"),
                    None => param_text(module, text, *declared).unwrap_or_else(|| name.to_string()),
                }
            }
            symbol => describe(module, text, symbol)?,
        };
        Some((occurrence.span, shown))
    }

    /// The diagnostics a file would have with the text `new` that it does not have now: the
    /// check on every edit, against the state before it.
    pub fn introduced(&self, path: &Path, new: &str) -> Vec<Diagnostic> {
        let old = self.text(path).unwrap_or_default();
        let (parsed, checked) = self.check_as(path, new);
        let mut found: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
        found.extend(checked.diagnostics);
        found.sort_by_key(|d| d.span.start);
        lotml_diag::introduced(found, new, self.diagnostics(path), old)
    }

    /// Rename a symbol of a file everywhere it is referred to, or refuse: when the new name is
    /// not a name, when it would make a reference resolve to something else, or when it would
    /// add an error. Nothing is written; the caller writes `text`.
    pub fn rename(&self, path: &Path, symbol: &Symbol, new_name: &str) -> Result<Renamed, Refused> {
        edit::valid_name(new_name)?;
        let text = self.text(path).ok_or_else(|| Refused(format!("{} is not in the workspace", path.display())))?;
        let old = self.references(path, symbol, true);
        let Some(first) = old.first() else { return Err(Refused("nothing here can be renamed".into())) };
        let old_name = &text[first.range()];
        if old_name == new_name {
            return Err(Refused(format!("it is already called `{new_name}`")));
        }
        let mut renamed_text = String::with_capacity(text.len());
        let mut sites = Vec::with_capacity(old.len());
        let mut at = 0;
        for span in &old {
            renamed_text += &text[at..span.start as usize];
            let start = renamed_text.len();
            renamed_text += new_name;
            sites.push(Span::new(start, renamed_text.len()));
            at = span.end as usize;
        }
        renamed_text += &text[at..];
        let renamed = match symbol {
            Symbol::Item(_) => Symbol::Item(new_name.to_string()),
            Symbol::Variant(_) => Symbol::Variant(new_name.to_string()),
            Symbol::Member(owner, _) => Symbol::Member(owner.clone(), new_name.to_string()),
            Symbol::Local(declared) => {
                Symbol::Local(old.iter().position(|s| s == declared).map_or(*declared, |i| sites[i]))
            }
        };
        // The new text must refer to the renamed symbol at exactly the renamed places.
        let (parsed, resolved) = self.check_as(path, &renamed_text);
        let found = symbols::occurrences(&parsed.module, &resolved);
        let now: Vec<Span> = found.iter().filter(|o| o.symbol == renamed).map(|o| o.span).collect();
        if now != sites {
            let moved: Vec<String> = now
                .iter()
                .filter(|s| !sites.contains(s))
                .chain(sites.iter().filter(|s| !now.contains(s)))
                .map(|s| lotml_syntax::span::line_column(&renamed_text, s.start).0.to_string())
                .collect();
            return Err(Refused(format!(
                "renaming `{old_name}` to `{new_name}` would change what the names at lines {} refer to",
                moved.join(", ")
            )));
        }
        let mut errors = parsed.errors.iter().map(lotml_check::syntax).collect::<Vec<_>>();
        errors.extend(resolved.diagnostics);
        let counts = |ds: &[Diagnostic]| {
            let mut by_code: HashMap<&str, usize> = HashMap::new();
            for d in ds {
                *by_code.entry(d.code).or_default() += 1;
            }
            by_code
        };
        let (before, after) = (counts(self.diagnostics(path)), counts(&errors));
        if let Some((code, _)) = after.iter().find(|(code, n)| before.get(*code).is_none_or(|b| *n > b)) {
            let d = errors.iter().find(|d| d.code == *code).expect("a diagnostic of that code");
            return Err(Refused(format!(
                "renaming `{old_name}` to `{new_name}` would add an error: {} {}",
                d.code, d.message
            )));
        }
        let mut textual: Vec<(PathBuf, Span)> =
            mentions(&renamed_text, &found, old_name).into_iter().map(|s| (path.to_path_buf(), s)).collect();
        for other in self.files.keys().filter(|p| p.as_path() != path) {
            let text = self.text(other).unwrap_or_default();
            textual.extend(mentions(text, self.occurrences(other), old_name).into_iter().map(|s| (other.clone(), s)));
        }
        Ok(Renamed { text: renamed_text, sites, mentions: textual })
    }

    /// The type the checker gave a local, from any of the places it is used.
    fn local_type(&self, path: &Path, symbol: &Symbol) -> Option<String> {
        let file = *self.files.get(path)?;
        let types = &checked(&self.db, file).types;
        self.references(path, symbol, true)
            .iter()
            .filter_map(|span| types.get(span))
            .find(|ty| !matches!(ty, lotml_check::ty::Ty::Error | lotml_check::ty::Ty::Var(_)))
            .map(ToString::to_string)
    }

    /// A file's declarations, with the fields, variants and methods inside them.
    pub fn outline(&self, path: &Path) -> Vec<Outline> {
        let Some(module) = self.module(path) else { return Vec::new() };
        let text = self.text(path).unwrap_or_default();
        let leaf = |name: String, kind: Kind, span: Span, name_span: Span| Outline {
            name,
            kind,
            span,
            name_span,
            children: Vec::new(),
        };
        let method = |m: &FnDef| {
            leaf(m.name.name.clone(), Kind::Method, Span { start: m.span.start, end: m.end() }, m.name.span)
        };
        let fields = |fields: &[lotml_syntax::ast::Field]| -> Vec<Outline> {
            fields
                .iter()
                .filter_map(|f| f.name.as_ref().map(|n| leaf(n.name.clone(), Kind::Field, f.span, n.span)))
                .collect()
        };
        let mut out = Vec::new();
        for item in &module.items {
            let span = Span { start: item.span().start, end: item.end() };
            let entry = match item {
                Item::Fn(f) => leaf(f.name.name.clone(), Kind::Function, span, f.name.span),
                Item::Record(r) => Outline {
                    children: fields(&r.fields),
                    ..leaf(r.name.name.clone(), Kind::Record, span, r.name.span)
                },
                Item::Sum(s) => Outline {
                    children: s
                        .variants
                        .iter()
                        .map(|v| Outline {
                            children: fields(v.fields.as_deref().unwrap_or_default()),
                            ..leaf(v.name.name.clone(), Kind::Variant, v.span, v.name.span)
                        })
                        .collect(),
                    ..leaf(s.name.name.clone(), Kind::Sum, span, s.name.span)
                },
                Item::Trait(t) => Outline {
                    children: t.methods.iter().map(method).collect(),
                    ..leaf(t.name.name.clone(), Kind::Trait, span, t.name.span)
                },
                Item::Impl(imp) => {
                    let target = &text[imp.target.span.range()];
                    let name = match &imp.trait_name {
                        Some(t) => format!("impl {} for {target}", &text[t.span.range()]),
                        None => format!("impl {target}"),
                    };
                    Outline {
                        children: imp.methods.iter().map(method).collect(),
                        ..leaf(name, Kind::Impl, span, imp.target.span)
                    }
                }
                Item::Test(t) => leaf(format!("test \"{}\"", t.name), Kind::Test, span, t.name_span),
                Item::Import(_) | Item::Error(_) => continue,
            };
            out.push(entry);
        }
        out
    }
}

/// The symbols an outline declares, named as `show` and `replace` take them: `area`, `Shape`, a
/// variant `Circle`, a field `Point.x`, a method `Counter.get` (a generic impl's type without its
/// parameters), a trait's method `Show.show`, and a test block as `test "name"`.
pub fn symbols(outline: &[Outline]) -> Vec<String> {
    let mut found = Vec::new();
    for entry in outline {
        match entry.kind {
            Kind::Impl => {
                let target = entry.name.rsplit(" for ").next().unwrap_or("").trim_start_matches("impl ");
                let owner = target.split('[').next().unwrap_or(target).trim();
                found.extend(entry.children.iter().map(|m| format!("{owner}.{}", m.name)));
            }
            Kind::Sum => {
                found.push(entry.name.clone());
                found.extend(entry.children.iter().map(|v| v.name.clone()));
            }
            _ => {
                found.push(entry.name.clone());
                found.extend(entry.children.iter().map(|c| format!("{}.{}", entry.name, c.name)));
            }
        }
    }
    found
}

/// The whole-word occurrences of `name` in `text` that are no reference to anything: in a
/// comment or a string, or a name nothing declares.
fn mentions(text: &str, occurrences: &[Occurrence], name: &str) -> Vec<Span> {
    let referenced: HashSet<u32> = occurrences.iter().map(|o| o.span.start).collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name)
        .filter(|(i, _)| {
            !text[..*i].chars().next_back().is_some_and(is_word)
                && !text[i + name.len()..].chars().next().is_some_and(is_word)
        })
        .map(|(i, _)| Span::new(i, i + name.len()))
        .filter(|s| !referenced.contains(&s.start))
        .collect()
}

/// A parameter as written, `n: int`, when `declared` is the name of one.
fn param_text(module: &Module, text: &str, declared: Span) -> Option<String> {
    let functions = module.items.iter().flat_map(|item| match item {
        Item::Fn(f) => std::slice::from_ref(f),
        Item::Impl(imp) => imp.methods.as_slice(),
        Item::Trait(t) => t.methods.as_slice(),
        _ => &[],
    });
    functions.flat_map(|f| &f.params).find(|p| p.name.span == declared).map(|p| text[p.span.range()].to_string())
}

/// A declaration as a reader wants it shown: a function's signature with its documentation, a
/// type as declared, or a trait's header; a variant, field or method under the line naming the
/// type or trait it belongs to.
fn describe(module: &Module, text: &str, symbol: &Symbol) -> Option<String> {
    let documented = |f: &FnDef| {
        let signature = lotml_fmt::signature(text, f);
        match &f.doc {
            Some(doc) => format!("{signature}\n\n{}", doc.trim()),
            None => signature,
        }
    };
    for item in &module.items {
        match (item, symbol) {
            (Item::Fn(f), Symbol::Item(name)) if f.name.name == *name => return Some(documented(f)),
            (Item::Record(r), Symbol::Item(name)) if r.name.name == *name => {
                return Some(lotml_fmt::item(text, item).trim_end().to_string());
            }
            (Item::Sum(s), Symbol::Item(name)) if s.name.name == *name => {
                return Some(lotml_fmt::item(text, item).trim_end().to_string());
            }
            (Item::Sum(s), Symbol::Variant(name)) => {
                if let Some(v) = s.variants.iter().find(|v| v.name.name == *name) {
                    return Some(format!("{}\n{}", s.name.name, &text[v.span.range()]));
                }
            }
            (Item::Trait(t), Symbol::Item(name)) if t.name.name == *name => {
                return Some(format!("trait {}", t.name.name));
            }
            (Item::Record(r), Symbol::Member(owner, field)) if r.name.name == *owner => {
                // A method of the record is in its `impl`, further on.
                if let Some(f) = r.fields.iter().find(|f| f.name.as_ref().is_some_and(|n| n.name == *field)) {
                    return Some(format!("{owner}\n{}", &text[f.span.range()]));
                }
            }
            (Item::Sum(s), Symbol::Member(owner, field)) => {
                let fields = s.variants.iter().find(|v| v.name.name == *owner).and_then(|v| v.fields.as_ref());
                if let Some(f) =
                    fields.into_iter().flatten().find(|f| f.name.as_ref().is_some_and(|n| n.name == *field))
                {
                    return Some(format!("{owner}\n{}", &text[f.span.range()]));
                }
            }
            (Item::Impl(imp), Symbol::Member(owner, method))
                if symbols::named(&imp.target).is_some_and(|n| n.name == *owner) =>
            {
                if let Some(m) = imp.methods.iter().find(|m| m.name.name == *method) {
                    return Some(format!("{owner}\n{}", documented(m)));
                }
            }
            (Item::Trait(t), Symbol::Member(owner, method)) if t.name.name == *owner => {
                if let Some(m) = t.methods.iter().find(|m| m.name.name == *method) {
                    return Some(format!("{owner}\n{}", documented(m)));
                }
            }
            _ => {}
        }
    }
    None
}
