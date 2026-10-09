//! A Python stub read as a LotML interface (specs/rust-binder R1, adr:0012): its module-level
//! functions, the names typeshed writes as methods of a module-level instance, `randint =
//! _inst.randint`, bound from those methods, and its classes (specs/python-classes, adr:0034). A function whose types LotML cannot express is listed
//! in a comment with the reason, never bound half-way; a type no LotML type describes is `PyObject`
//! (adr:0031).

use std::collections::{HashMap, HashSet};

use ruff_python_ast::{self as ast, Expr, Number, Operator, PySourceType, Stmt};

/// Why a stub was not bound.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused(pub String);

/// The type of a value no LotML type describes: opaque, taken out only through a conversion the
/// boundary checks (adr:0031).
const OBJECT: &str = "PyObject";

/// What an annotation is read against: the classes of the stub the interface declares, which a
/// type names by name, and the class a member belongs to, which `Self` names.
#[derive(Clone, Copy)]
struct Cx<'a> {
    classes: &'a HashSet<String>,
    this: Option<&'a str>,
    /// The stub's type variables, its own and those it imports (adr:0036).
    vars: &'a Vars,
    /// The type each constrained variable is given for the overload being written.
    fixed: &'a [(&'a str, &'a str)],
    /// The bytes of signatures written so far, so a stub whose bindings outgrow [`LARGEST`] stops
    /// being written as soon as they do (adr:0032).
    spent: &'a std::cell::Cell<usize>,
    /// The type parameters of the class a member belongs to.
    own: &'a [String],
    /// The type parameters of each generic class of the stub the interface declares.
    params: &'a HashMap<String, Vec<String>>,
}

/// What a type variable is (adr:0036).
#[derive(Clone, Debug, PartialEq)]
enum Var {
    /// One type a call infers: plain, or bounded, the bound left to Python.
    Plain,
    /// One of the types it lists, as an interface writes them: an overload each.
    Constrained(Vec<String>),
    /// A `ParamSpec` or a `TypeVarTuple`, which no LotML type describes.
    Spread,
}

/// Type variables by the name a stub gives them.
type Vars = HashMap<String, Var>;

/// LotML's keywords, which no name of an interface may be.
const KEYWORDS: &[&str] = &[
    "fn", "type", "impl", "trait", "for", "test", "from", "import", "var", "inout", "sink", "return", "if", "elif",
    "else", "while", "in", "match", "case", "lambda", "and", "or", "not", "is", "None", "True", "False", "fail",
    "pass", "break", "continue", "assert", "dyn",
];

/// The names a class of an interface may not take: LotML's keywords and the names of its own
/// types, which a type named so would shadow.
fn reserved(name: &str) -> bool {
    KEYWORDS.contains(&name)
        || matches!(
            name,
            "int"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "u8"
                | "f32"
                | "f64"
                | "str"
                | "bool"
                | "bytes"
                | "PyObject"
                | "PyError"
                | "Heap"
                | "Ok"
                | "Err"
                | "Self"
        )
}

/// The binder's own source: what an interface kept from a run of lotml is keyed by, so a binder
/// that writes differently never reads one an earlier binder wrote.
pub const SOURCE: &str = include_str!("binder.rs");

/// Past this, a file is no stub lotml reads.
pub const LARGEST: usize = 8 << 20;
/// The deepest a stub may nest brackets, or indent blocks.
pub const DEEPEST: usize = 100;
/// The most tokens one logical line of a stub may hold.
pub const LONGEST_LINE: usize = 20_000;
/// The stack the parse and the walk run on, which a tree within the limits above cannot overflow.
const STACK: usize = 64 << 20;

/// The interface of `module` from the text of its stub, the first line naming `said`, the source.
/// A stub is hostile input (adr:0032): one past [`LARGEST`], [`DEEPEST`] or [`LONGEST_LINE`] is
/// refused before it is parsed, which bounds the depth of the tree built and walked.
pub fn interface(module: &str, stub: &str, said: &str) -> Result<String, Refused> {
    within_limits(stub)?;
    on_big_stack(|| bind(module, stub, said, None))
}

/// [`interface`], the names the stub re-exports bound from `parts`, the stubs they come from
/// (specs/python-reexports).
pub fn interface_with(module: &str, stub: &str, said: &str, parts: &Parts) -> Result<String, Refused> {
    within_limits(stub)?;
    on_big_stack(|| bind(module, stub, said, Some(parts)))
}

/// `work` run on a thread with [`STACK`] of stack, which a stub's tree is walked on.
fn on_big_stack<T: Send>(work: impl FnOnce() -> Result<T, Refused> + Send) -> Result<T, Refused> {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(scope, work)
            .map_err(|e| Refused(format!("cannot start the binder: {e}")))?
            .join()
            .map_err(|_| Refused("the binder stopped on this stub".into()))?
    })
}

/// The stubs a stub re-exports names from (specs/python-reexports), and the package the stub is
/// of, which its relative imports are read against.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    pub package: String,
    pub parts: Vec<Part>,
}

/// A stub a module re-exports names from: its module, the package it is of, and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub module: String,
    pub package: String,
    pub text: String,
}

/// The package a stub at `path` (a file name is enough) of `module` is of, which its relative
/// imports are read against: the module itself for an `__init__`, else the module above it.
pub fn package_of(module: &str, path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if file.starts_with("__init__.") {
        return module.to_string();
    }
    module.rsplit_once('.').map_or_else(String::new, |(above, _)| above.to_string())
}

/// The most modules a re-exported name is followed through, the stub's own first.
pub const REEXPORT_DEPTH: usize = 4;

/// The most modules a stub and those it re-exports from may name to re-export from, each a look
/// for a file, so a stub cannot make binding it search thousands.
pub const REEXPORT_MODULES: usize = 256;

/// The stubs a stub of `package` re-exports from, through theirs, breadth first: each module asked
/// of `read` once (its text and its package, or nothing when it has no stub), none further than
/// [`REEXPORT_DEPTH`] from the stub, and all of them within [`LARGEST`] and the stub's limits.
pub fn sources(
    package: &str,
    text: &str,
    read: &mut (dyn FnMut(&str) -> Option<(String, String)> + Send),
) -> Result<Parts, Refused> {
    within_limits(text)?;
    on_big_stack(|| {
        let mut parts = Vec::new();
        let mut asked = HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        let mut total = text.len();
        let parsed = ruff_python_parser::parse_unchecked_source(text, PySourceType::Stub);
        for re in reexports(&parsed.syntax().body, package) {
            if asked.insert(re.from.clone()) {
                queue.push_back((re.from, 1));
            }
        }
        while let Some((module, depth)) = queue.pop_front() {
            if asked.len() > REEXPORT_MODULES {
                return Err(Refused(format!("it re-exports from more than {REEXPORT_MODULES} modules")));
            }
            let Some((text, of)) = read(&module) else { continue };
            total += text.len();
            if total > LARGEST {
                return Err(Refused(format!("the stubs it re-exports from are past the {LARGEST} bytes lotml reads")));
            }
            within_limits(&text)
                .map_err(|Refused(why)| Refused(format!("`{module}`, which it re-exports from: {why}")))?;
            if depth < REEXPORT_DEPTH {
                let parsed = ruff_python_parser::parse_unchecked_source(&text, PySourceType::Stub);
                for re in reexports(&parsed.syntax().body, &of) {
                    if asked.insert(re.from.clone()) {
                        queue.push_back((re.from, depth + 1));
                    }
                }
            }
            parts.push(Part { module, package: of, text });
        }
        Ok(Parts { package: package.to_string(), parts })
    })
}

/// An import of a stub that re-exports: the module, made absolute, and each name as it has it and
/// as the stub gives it; no names for `*`.
struct Reexport {
    from: String,
    names: Option<Vec<(String, String)>>,
}

/// The re-exporting imports of a stub of `package`: `from m import n as n`, a name its `__all__`
/// lists, and `from m import *`. An import of a module itself (`from . import m`) is not one.
fn reexports(body: &[Stmt], package: &str) -> Vec<Reexport> {
    let listed: HashSet<String> = all_names(body).unwrap_or_default().into_iter().collect();
    let mut found = Vec::new();
    for statement in module_level(body) {
        let Stmt::ImportFrom(import) = statement else { continue };
        let Some(from) = absolute(package, import.level, import.module.as_ref().map(|m| m.as_str())) else { continue };
        if import.names.iter().any(|a| a.name.as_str() == "*") {
            found.push(Reexport { from, names: None });
            continue;
        }
        let names: Vec<(String, String)> = import
            .names
            .iter()
            .filter_map(|alias| {
                let given = alias.asname.as_ref().unwrap_or(&alias.name).as_str();
                let exported =
                    alias.asname.as_ref().is_some_and(|a| a.as_str() == alias.name.as_str()) || listed.contains(given);
                exported.then(|| (alias.name.to_string(), given.to_string()))
            })
            .collect();
        if !names.is_empty() {
            found.push(Reexport { from, names: Some(names) });
        }
    }
    found
}

/// `module` as imported from a stub of `package` at relative `level`, made absolute: `.core` of
/// `certifi` is `certifi.core`; none for a level past the package's root or a bare `from .`.
fn absolute(package: &str, level: u32, module: Option<&str>) -> Option<String> {
    if level == 0 {
        return module.map(ToString::to_string);
    }
    let mut base: Vec<&str> = package.split('.').collect();
    for _ in 1..level {
        base.pop()?;
    }
    if base.is_empty() || base == [""] {
        return None;
    }
    let module = module?;
    Some(format!("{}.{module}", base.join(".")))
}

/// The names a stub's `__all__` lists, `__all__ = [...]` and `__all__ += [...]` added, if it has
/// one: identifiers only, since a name it lists may be written into the interface.
fn all_names(body: &[Stmt]) -> Option<Vec<String>> {
    let mut names: Option<Vec<String>> = None;
    let strings = |value: &Expr| -> Vec<String> {
        let items: &[Expr] = match value {
            Expr::List(list) => &list.elts,
            Expr::Tuple(tuple) => &tuple.elts,
            _ => &[],
        };
        items
            .iter()
            .filter_map(|e| e.as_string_literal_expr().map(|s| s.value.to_str().to_string()))
            .filter(|name| identifier(name))
            .collect()
    };
    for statement in module_level(body) {
        match statement {
            Stmt::Assign(assigned) if matches!(assigned.targets.as_slice(), [Expr::Name(n)] if n.id.as_str() == "__all__") =>
            {
                names = Some(strings(&assigned.value));
            }
            Stmt::AugAssign(added) if matches!(&*added.target, Expr::Name(n) if n.id.as_str() == "__all__") => {
                names.get_or_insert_with(Vec::new).extend(strings(&added.value));
            }
            _ => {}
        }
    }
    names
}

/// Whether `name` is a Python identifier: a letter or `_`, then letters, digits and `_`.
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// A stub read once for its re-exports (specs/python-reexports): the statements defining each
/// name, its public names, and what it re-exports by name and by `*`, so a name is looked up in it
/// without reading it again.
struct Indexed<'s> {
    defs: HashMap<&'s str, Vec<&'s Stmt>>,
    /// Its `__all__`, else each module-level `def` and `class` not starting with `_`, in order.
    public: Vec<String>,
    exported: HashSet<String>,
    /// Each name it re-exports, the first import of it winning: the module and the name there.
    named: HashMap<String, (String, String)>,
    /// The modules it re-exports `*` from.
    stars: Vec<String>,
    reexports: Vec<Reexport>,
}

fn indexed<'s>(body: &'s [Stmt], package: &str) -> Indexed<'s> {
    let mut defs: HashMap<&str, Vec<&Stmt>> = HashMap::new();
    let mut defined = Vec::new();
    for statement in module_level(body) {
        let name = match statement {
            Stmt::FunctionDef(f) => f.name.as_str(),
            Stmt::ClassDef(c) => c.name.as_str(),
            _ => continue,
        };
        let entry = defs.entry(name).or_default();
        if entry.is_empty() && !name.starts_with('_') {
            defined.push(name.to_string());
        }
        entry.push(statement);
    }
    let public = all_names(body).unwrap_or(defined);
    let exported = public.iter().cloned().collect();
    let reexports = reexports(body, package);
    let mut named = HashMap::new();
    let mut stars = Vec::new();
    for re in &reexports {
        match &re.names {
            Some(names) => {
                for (had, given) in names {
                    named.entry(given.clone()).or_insert_with(|| (re.from.clone(), had.clone()));
                }
            }
            None => stars.push(re.from.clone()),
        }
    }
    Indexed { defs, public, exported, named, stars, reexports }
}

/// The statements a stub's re-exports bring, each name's definitions in the module that makes it,
/// renamed as the stub gives it, and a comment for each name it does not find; a name the stub
/// defines itself keeps its own.
fn reexported(own: &Indexed<'_>, parts: &HashMap<&str, Indexed<'_>>) -> (Vec<Stmt>, Vec<String>) {
    let mut brought = Vec::new();
    let mut skipped = Vec::new();
    let mut given = HashSet::new();
    for re in &own.reexports {
        let names: Vec<(String, String)> = match &re.names {
            Some(names) => names.clone(),
            None => match parts.get(re.from.as_str()) {
                Some(source) => source.public.iter().map(|n| (n.clone(), n.clone())).collect(),
                None => {
                    skipped.push(format!("#   *: `{}`, which it re-exports from, has no stub lotml found", re.from));
                    continue;
                }
            },
        };
        for (name, local) in names {
            if local.starts_with('_') || own.defs.contains_key(local.as_str()) || !given.insert(local.clone()) {
                continue;
            }
            // Each name is followed on its own, so one definition re-exported under two names
            // gives both.
            match defined(parts, &re.from, &name, 1, &mut HashSet::new()) {
                Ok(statements) => brought.extend(statements.into_iter().map(|s| renamed(s.clone(), &local))),
                Err(why) => skipped.push(format!("#   {local}: {why}")),
            }
        }
    }
    (brought, skipped)
}

/// The module-level statements that define `name` in `module`, or, where `module` re-exports it,
/// those of the module it comes from, followed through at most [`REEXPORT_DEPTH`] modules, a
/// cycle ending where it repeats.
fn defined<'s>(
    parts: &HashMap<&str, Indexed<'s>>,
    module: &str,
    name: &str,
    depth: usize,
    followed: &mut HashSet<(String, String)>,
) -> Result<Vec<&'s Stmt>, String> {
    if !followed.insert((module.to_string(), name.to_string())) {
        return Ok(Vec::new());
    }
    let Some(part) = parts.get(module) else {
        return Err(format!("`{module}`, which it re-exports from, has no stub lotml found"));
    };
    if let Some(found) = part.defs.get(name) {
        return Ok(found.clone());
    }
    if depth >= REEXPORT_DEPTH {
        return Err(format!("it is re-exported through more than {REEXPORT_DEPTH} modules"));
    }
    if let Some((from, had)) = part.named.get(name) {
        return defined(parts, from, had, depth + 1, followed);
    }
    let star = part.stars.iter().find(|from| parts.get(from.as_str()).is_some_and(|p| p.exported.contains(name)));
    if let Some(from) = star {
        return defined(parts, from, name, depth + 1, followed);
    }
    Err(format!("`{module}` neither defines nor re-exports it"))
}

/// `statement`, a `def` or `class`, under the name `name`.
fn renamed(mut statement: Stmt, name: &str) -> Stmt {
    match &mut statement {
        Stmt::FunctionDef(f) if f.name.as_str() != name => f.name = ast::Identifier::new(name, f.name.range),
        Stmt::ClassDef(c) if c.name.as_str() != name => c.name = ast::Identifier::new(name, c.name.range),
        _ => {}
    }
    statement
}

/// Whether `stub` stays within the sizes lotml reads, by one pass of the lexer.
fn within_limits(stub: &str) -> Result<(), Refused> {
    use ruff_python_ast::token::TokenKind;
    if stub.len() > LARGEST {
        return Err(Refused(format!("the stub is {} bytes, past the {LARGEST} lotml reads", stub.len())));
    }
    let line = |at: usize| stub.as_bytes()[..at.min(stub.len())].iter().filter(|&&b| b == b'\n').count() + 1;
    let mut lexer = ruff_python_parser::lexer::lex(stub, ruff_python_parser::Mode::Module);
    let (mut brackets, mut blocks, mut tokens) = (0usize, 0usize, 0usize);
    loop {
        let kind = lexer.next_token();
        let at = usize::from(lexer.current_range().start());
        match kind {
            TokenKind::EndOfFile => return Ok(()),
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => brackets += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => brackets = brackets.saturating_sub(1),
            TokenKind::Indent => blocks += 1,
            TokenKind::Dedent => blocks = blocks.saturating_sub(1),
            TokenKind::Newline => tokens = 0,
            TokenKind::NonLogicalNewline if brackets == 0 => tokens = 0,
            _ => {}
        }
        tokens += 1;
        if brackets > DEEPEST || blocks > DEEPEST {
            return Err(Refused(format!("the stub nests deeper than {DEEPEST} levels, at line {}", line(at))));
        }
        if tokens > LONGEST_LINE {
            return Err(Refused(format!(
                "a line of the stub holds more than {LONGEST_LINE} tokens, at line {}",
                line(at)
            )));
        }
    }
}

/// [`interface`], once the stub is known to be within the limits.
fn bind(module: &str, stub: &str, said: &str, parts: Option<&Parts>) -> Result<String, Refused> {
    let parsed = ruff_python_parser::parse_unchecked_source(stub, PySourceType::Stub);
    if let Some(error) = parsed.errors().first() {
        let at = usize::from(error.location.start()).min(stub.len());
        let line = stub.as_bytes()[..at].iter().filter(|&&b| b == b'\n').count() + 1;
        return Err(Refused(format!("the stub does not parse, at line {line}: {}", error.error)));
    }
    let own = &parsed.syntax().body;
    let mut bound = Vec::new();
    let mut skipped = Vec::new();
    // The stubs it re-exports from: their names it re-exports are bound as its own, after its own
    // statements, so a name it defines wins; their type variables are its too (specs/python-reexports).
    let read: Vec<_> = parts
        .map_or(&[][..], |p| p.parts.as_slice())
        .iter()
        .map(|p| (p, ruff_python_parser::parse_unchecked_source(&p.text, PySourceType::Stub)))
        .collect();
    let index: HashMap<&str, Indexed<'_>> =
        read.iter().map(|(p, parsed)| (p.module.as_str(), indexed(&parsed.syntax().body, &p.package))).collect();
    // Re-exports are bound only where their stubs were looked for: with none looked for, a stub's
    // imports are left as they were, rather than each listed as having no stub.
    let (brought, left_out) = match parts {
        Some(parts) => reexported(&indexed(own, &parts.package), &index),
        None => (Vec::new(), Vec::new()),
    };
    skipped.extend(left_out);
    let combined: Vec<Stmt>;
    let body: &[Stmt] = if brought.is_empty() {
        own
    } else {
        combined = own.iter().cloned().chain(brought).collect();
        &combined
    };
    let mut vars = type_vars(body);
    for (_, parsed) in &read {
        for (name, var) in type_vars(&parsed.syntax().body) {
            vars.entry(name).or_insert(var);
        }
    }
    let (classes, params, refused) = bindable_classes(body, &vars);
    skipped.extend(refused);
    let spent = std::cell::Cell::new(0);
    let cx = Cx { classes: &classes, this: None, vars: &vars, fixed: &[], own: &[], params: &params, spent: &spent };
    let past = || Refused(format!("its interface would be past the {LARGEST} bytes lotml reads"));
    let mut seen = HashSet::new();
    let functions = functions(body, cx);
    if spent.get() > LARGEST {
        return Err(past());
    }
    for (name, lines, left_out) in functions {
        if name.starts_with('_') || !seen.insert(name.clone()) {
            continue;
        }
        bound.extend(lines);
        skipped.extend(left_out);
    }
    let source: String = said.chars().map(|c| if printable(c) { c } else { '?' }).collect();
    let mut lines = vec![
        format!("# The Python module `{module}`, bound by `lotml bind` from {source}."),
        "# Do not edit: run `lotml bind` again.".to_string(),
        "# Every function returns `T ! PyError`: a stub does not say what a call raises.".to_string(),
        "# A parameter written `= todo()` is optional: Python supplies its default.".to_string(),
        "# A `PyObject` is a value no LotML type describes: convert it with `value()`.".to_string(),
    ];
    lines.extend(bound);
    let mut written = HashSet::new();
    for class in module_level(body).into_iter().filter_map(Stmt::as_class_def_stmt) {
        // A class written in two `sys.version_info` branches is bound from the first, as a function is.
        if classes.contains(class.name.as_str()) && written.insert(class.name.as_str()) {
            let (block, left_out) = class_block(class, cx);
            if spent.get() > LARGEST {
                return Err(past());
            }
            lines.push(String::new());
            lines.extend(block);
            skipped.extend(left_out);
        }
    }
    if !skipped.is_empty() {
        lines.push(String::new());
        lines.push("# Not bound:".to_string());
        lines.extend(skipped);
    }
    let text = lines.join("\n") + "\n";
    if text.len() > LARGEST {
        return Err(past());
    }
    Ok(text)
}

/// Each module-level function by name, in the order written: the lines of its signatures (one,
/// or one per overload), and a comment for what is left out. The functions come first, then the
/// names written as an instance's methods, each found in an index of its class's methods built
/// once, the first written of a name winning, so a stub of many aliases binds in time linear in
/// its size.
fn functions(body: &[Stmt], cx: Cx<'_>) -> Vec<(String, Vec<String>, Vec<String>)> {
    let runs = overload_runs(body);
    // Each name once, the first written winning, so a stub repeating one overload thousands of
    // times binds it once rather than once per repetition.
    let mut named: HashSet<String> = HashSet::new();
    let mut found: Vec<(String, Vec<String>, Vec<String>)> = definitions(body)
        .into_iter()
        .filter(|f| named.insert(f.name.to_string()))
        .map(|f| {
            let name = f.name.as_str();
            let (lines, left_out) = match runs.get(name) {
                Some(run) if is_overload(f) => {
                    overloads(run, name, |o| expanded(o, cx, |cx| written(o, name, false, cx)), |_| false)
                }
                _ => one(name, expanded(f, cx, |cx| written(f, name, false, cx))),
            };
            (name.to_string(), lines, left_out)
        })
        .collect();
    let mut methods: HashMap<&str, (HashMap<&str, &ast::StmtFunctionDef>, Runs<'_>)> = HashMap::new();
    for class in module_level(body).into_iter().filter_map(Stmt::as_class_def_stmt) {
        let mut index = HashMap::new();
        for method in definitions(&class.body) {
            index.entry(method.name.as_str()).or_insert(method);
        }
        methods.insert(class.name.as_str(), (index, overload_runs(&class.body)));
    }
    for (name, class, method) in aliases(body) {
        if !named.insert(name.clone()) {
            continue;
        }
        let index = methods.get(class.as_str());
        let first = index.and_then(|(index, _)| index.get(method.as_str()));
        let run = index.and_then(|(_, runs)| runs.get(method.as_str()));
        let (lines, left_out) = match (first, run) {
            (None, _) => one(&name, Err(format!("`{class}` holds no `{method}` in this stub"))),
            (Some(m), Some(run)) if is_overload(m) => {
                let write = |o: &ast::StmtFunctionDef| expanded(o, cx, |cx| written(o, &name, !is_static(o), cx));
                overloads(run, &name, write, |o| !is_static(o))
            }
            (Some(m), _) => one(&name, expanded(m, cx, |cx| written(m, &name, !is_static(m), cx))),
        };
        found.push((name, lines, left_out));
    }
    found
}

/// A name's one signature as lines of the interface, or the comment saying why it is left out.
fn one(name: &str, written: Result<Vec<String>, String>) -> (Vec<String>, Vec<String>) {
    match written {
        Ok(lines) => (lines, Vec::new()),
        Err(why) => (Vec::new(), vec![format!("#   {name}: {why}")]),
    }
}

/// The most overloads of one name the binder writes: as many as the checker reads (adr:0035).
const OVERLOADS: usize = 64;

/// Runs of `@overload` definitions by name.
type Runs<'s> = HashMap<&'s str, Vec<&'s ast::StmtFunctionDef>>;

/// The `@overload` definitions of each name in `body`: those written one after another in one
/// statement list, the first such run of a name in the order written winning, as the first
/// definition of a function does.
fn overload_runs(body: &[Stmt]) -> Runs<'_> {
    fn walk<'s>(body: &'s [Stmt], runs: &mut Runs<'s>) {
        let mut at = 0;
        while let Some(statement) = body.get(at) {
            at += 1;
            match statement {
                Stmt::If(branch) => {
                    walk(&branch.body, runs);
                    for clause in &branch.elif_else_clauses {
                        walk(&clause.body, runs);
                    }
                }
                Stmt::FunctionDef(f) if is_overload(f) => {
                    let mut run = vec![f];
                    while let Some(Stmt::FunctionDef(next)) = body.get(at)
                        && next.name.as_str() == f.name.as_str()
                        && is_overload(next)
                    {
                        run.push(next);
                        at += 1;
                    }
                    runs.entry(f.name.as_str()).or_insert(run);
                }
                _ => {}
            }
        }
    }
    let mut runs = HashMap::new();
    walk(body, &mut runs);
    runs
}

/// The overloads of `run` as `write` writes each, in order, and a comment for each left out
/// (adr:0035): one that cannot be written, or whose receiver differs from the first's, and every
/// one after it, since a call it would take would go to a later one of another result; one whose
/// parameters are an earlier one's, which no call would be given; and those past [`OVERLOADS`].
fn overloads(
    run: &[&ast::StmtFunctionDef],
    label: &str,
    write: impl Fn(&ast::StmtFunctionDef) -> Result<Vec<String>, String>,
    receiver: impl Fn(&ast::StmtFunctionDef) -> bool,
) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    let mut taken = HashSet::new();
    let mut skipped = Vec::new();
    let total = run.len();
    // Counted as read, repeats and a constrained variable's expansions included, so no run costs
    // more than the limit to write.
    let mut read = 0;
    for (k, f) in run.iter().enumerate() {
        let which = format!("#   {label}: overload {} of {total}", k + 1);
        if read >= OVERLOADS {
            skipped.push(format!("#   {label}: overloads {} to {total}: past the {OVERLOADS} lotml reads", k + 1));
            break;
        }
        if receiver(f) != receiver(run[0]) {
            skipped.push(format!("{which}: it takes `self` where the first does not; it and those after are left out"));
            break;
        }
        match write(f) {
            Ok(written) => {
                for line in written.into_iter().take(OVERLOADS - read) {
                    read += 1;
                    let parameters = line.split(" -> ").next().unwrap_or(&line).to_string();
                    if taken.insert(parameters) {
                        lines.push(line);
                    } else {
                        skipped
                            .push(format!("{which}: an earlier one takes the same parameters, so no call is given it"));
                    }
                }
            }
            Err(why) => {
                skipped.push(format!("{which}: {why}; it and those after are left out"));
                break;
            }
        }
    }
    (lines, skipped)
}

/// The functions at `body`'s level, those under `if sys.version_info …` blocks included.
fn definitions(body: &[Stmt]) -> Vec<&ast::StmtFunctionDef> {
    module_level(body).into_iter().filter_map(Stmt::as_function_def_stmt).collect()
}

/// The statements at `body`'s level in the order written, each branch of an `if` where the `if`
/// stands.
fn module_level(body: &[Stmt]) -> Vec<&Stmt> {
    let mut found = Vec::new();
    for statement in body {
        match statement {
            Stmt::If(branch) => {
                found.extend(module_level(&branch.body));
                for clause in &branch.elif_else_clauses {
                    found.extend(module_level(&clause.body));
                }
            }
            other => found.push(other),
        }
    }
    found
}

/// Each name typeshed writes as a method of an instance it declares, `randint = _inst.randint`
/// after `_inst: Random`: the name, the instance's class and the method.
fn aliases(body: &[Stmt]) -> Vec<(String, String, String)> {
    let mut instances: HashMap<&str, String> = HashMap::new();
    for statement in module_level(body) {
        if let Stmt::AnnAssign(declared) = statement
            && let Expr::Name(target) = &*declared.target
            && let Some(class) = name_of(&declared.annotation)
        {
            instances.insert(target.id.as_str(), class.to_string());
        }
    }
    let mut found = Vec::new();
    for statement in module_level(body) {
        if let Stmt::Assign(assigned) = statement
            && let [Expr::Name(target)] = assigned.targets.as_slice()
            && let Expr::Attribute(attribute) = &*assigned.value
            && let Expr::Name(instance) = &*attribute.value
            && let Some(class) = instances.get(instance.id.as_str())
        {
            found.push((target.id.to_string(), class.clone(), attribute.attr.to_string()));
        }
    }
    found
}

fn is_overload(function: &ast::StmtFunctionDef) -> bool {
    function.decorator_list.iter().any(|d| name_of(&d.expression) == Some("overload"))
}

fn is_static(function: &ast::StmtFunctionDef) -> bool {
    function.decorator_list.iter().any(|d| name_of(&d.expression) == Some("staticmethod"))
}

/// A name, or an attribute's last part: `int`, or `Optional` of `typing.Optional`.
fn name_of(expr: &Expr) -> Option<&str> {
    match expr {
        Expr::Name(name) => Some(name.id.as_str()),
        Expr::Attribute(attribute) => Some(attribute.attr.as_str()),
        _ => None,
    }
}

/// The signature of `function` under `name`, its first parameter dropped when it is a method
/// bound to its instance or class; `*args` and `**kwargs` take nothing when not given, so the named
/// parameters are bound and those are left to Python.
fn written(function: &ast::StmtFunctionDef, name: &str, bound_method: bool, cx: Cx<'_>) -> Result<String, String> {
    let params = params(function, bound_method, cx)?;
    let returns = function.returns.as_deref().map_or_else(|| OBJECT.to_string(), |r| lotml_type(r, false, cx));
    let params = params.join(", ");
    let generics = generics(&format!("{params} {returns}"), cx)?;
    Ok(format!("fn {name}{generics}({params}) -> {returns} ! PyError"))
}

/// The parameters of `function` as an interface writes them, its first dropped when it is bound
/// to an instance or a class; or why it is not bound: a coroutine, or a name LotML keeps.
fn params(function: &ast::StmtFunctionDef, drop_first: bool, cx: Cx<'_>) -> Result<Vec<String>, String> {
    if function.is_async {
        return Err("it is a coroutine".to_string());
    }
    if KEYWORDS.contains(&function.name.as_str()) {
        return Err(format!("`{}` is a LotML keyword", function.name.as_str()));
    }
    let parameters = &function.parameters;
    let mut positional: Vec<&ast::ParameterWithDefault> =
        parameters.posonlyargs.iter().chain(parameters.args.iter()).collect();
    if drop_first && !positional.is_empty() {
        positional.remove(0);
    }
    let mut written = Vec::new();
    for p in positional.into_iter().chain(parameters.kwonlyargs.iter()) {
        // `__x` is PEP 484's positional-only parameter, which no call names: it is written `x`,
        // since a name starting with `__` is the compiler's (E0220).
        let name = p.parameter.name.as_str();
        let name = match name.strip_prefix("__") {
            Some(bare) if !name.ends_with("__") && !bare.is_empty() => bare,
            _ => name,
        };
        if KEYWORDS.contains(&name) {
            return Err(format!("its parameter `{name}` is a LotML keyword"));
        }
        let annotated =
            p.parameter.annotation.as_deref().map_or_else(|| OBJECT.to_string(), |a| lotml_type(a, true, cx));
        written.push(match &p.default {
            Some(given) => format!("{name}: {annotated} = {}", default(given)),
            None => format!("{name}: {annotated}"),
        });
    }
    Ok(written)
}

/// The classes of the stub the interface declares, the type parameters of each generic one, and a
/// comment for each public one it does not: a protocol, of a name LotML keeps, or of a function's
/// name.
fn bindable_classes(body: &[Stmt], type_vars: &Vars) -> (HashSet<String>, HashMap<String, Vec<String>>, Vec<String>) {
    let functions: HashSet<&str> = definitions(body).iter().map(|f| f.name.as_str()).collect();
    let mut bound = HashSet::new();
    let mut params = HashMap::new();
    let mut refused = Vec::new();
    for class in module_level(body).into_iter().filter_map(Stmt::as_class_def_stmt) {
        let name = class.name.as_str();
        if name.starts_with('_') || bound.contains(name) {
            continue;
        }
        let own = class_params(class, type_vars);
        let why = if class.bases().iter().any(|b| name_of(head(b)) == Some("Protocol")) {
            Some("it is a protocol, a shape rather than a class".to_string())
        } else if own.len() > TYPE_PARAMS {
            Some(format!("it has more than {TYPE_PARAMS} type parameters"))
        } else if reserved(name) {
            Some(format!("`{name}` is a name LotML keeps"))
        } else if functions.contains(name) {
            Some("a function of the stub has its name".to_string())
        } else {
            None
        };
        match why {
            Some(why) => refused.push(format!("#   {name}: {why}")),
            None => {
                bound.insert(name.to_string());
                if !own.is_empty() {
                    params.insert(name.to_string(), own);
                }
            }
        }
    }
    (bound, params, refused)
}

/// A class's type parameters in the order PEP 484 gives them: its PEP 695 list, else those of its
/// `Generic[...]` base, else every type variable its bases name, in the order named.
fn class_params(class: &ast::StmtClassDef, type_vars: &Vars) -> Vec<String> {
    if let Some(declared) = &class.type_params {
        return declared.type_params.iter().map(|p| p.name().to_string()).collect();
    }
    let mut named = Vec::new();
    let generic =
        class.bases().iter().find(|b| matches!(b, Expr::Subscript(s) if name_of(&s.value) == Some("Generic")));
    match generic {
        Some(base) => names_in(base, &mut named),
        None => class.bases().iter().for_each(|b| names_in(b, &mut named)),
    }
    let mut params: Vec<String> = Vec::new();
    for name in named {
        if type_vars.contains_key(name) && !params.iter().any(|p| p == name) {
            params.push(name.to_string());
        }
    }
    params
}

/// The type variables of a stub: those it declares, and those it imports from `typing`,
/// `typing_extensions` or `_typeshed`, read from the typeshed lotml carries (adr:0036).
fn type_vars(body: &[Stmt]) -> Vars {
    let mut found = declared_vars(body);
    for statement in module_level(body) {
        if let Stmt::ImportFrom(import) = statement
            && let Some(vars) = import.module.as_ref().and_then(|m| shared_vars(m.as_str()))
        {
            for alias in &import.names {
                if let Some(var) = vars.get(alias.name.as_str()) {
                    let local = alias.asname.as_ref().unwrap_or(&alias.name);
                    found.entry(local.to_string()).or_insert_with(|| var.clone());
                }
            }
        }
    }
    found
}

/// The type variables `body` declares: `T = TypeVar("T")`, `AnyStr = TypeVar("AnyStr", str,
/// bytes)`, `P = ParamSpec("P")`. A `Self` one is the class's own, which `Self` already names.
fn declared_vars(body: &[Stmt]) -> Vars {
    let (classes, vars, params, spent) = (HashSet::new(), Vars::new(), HashMap::new(), std::cell::Cell::new(0));
    let cx = Cx { classes: &classes, this: None, vars: &vars, fixed: &[], own: &[], params: &params, spent: &spent };
    let mut found = Vars::new();
    for statement in module_level(body) {
        if let Stmt::Assign(assigned) = statement
            && let [Expr::Name(target)] = assigned.targets.as_slice()
            && let Expr::Call(call) = &*assigned.value
            && target.id.as_str() != "Self"
        {
            let var = match name_of(&call.func) {
                // Past the limit a constrained variable would stand for more overloads than a name
                // keeps, so it is a `PyObject`, as a `ParamSpec` is.
                Some("TypeVar") if call.arguments.args.len() > TYPE_PARAMS + 1 => Var::Spread,
                Some("TypeVar") if call.arguments.args.len() > 1 => {
                    Var::Constrained(call.arguments.args[1..].iter().map(|c| lotml_type(c, false, cx)).collect())
                }
                Some("TypeVar") => Var::Plain,
                Some("ParamSpec" | "TypeVarTuple") => Var::Spread,
                _ => continue,
            };
            found.insert(target.id.to_string(), var);
        }
    }
    found
}

/// The type variables of `module`, one of those stubs import them from, parsed from the typeshed
/// lotml carries once a process.
fn shared_vars(module: &str) -> Option<&'static Vars> {
    static SHARED: std::sync::OnceLock<HashMap<&'static str, Vars>> = std::sync::OnceLock::new();
    let shared = SHARED.get_or_init(|| {
        ["typing", "typing_extensions", "_typeshed"]
            .into_iter()
            .map(|name| {
                let vars = match crate::typeshed::find(name) {
                    crate::typeshed::Found::Stub { text, .. } => {
                        let parsed = ruff_python_parser::parse_unchecked_source(text, PySourceType::Stub);
                        declared_vars(&parsed.syntax().body)
                    }
                    _ => Vars::new(),
                };
                (name, vars)
            })
            .collect()
    });
    shared.get(module)
}

/// `function` written by `write` once, or, where it names constrained type variables that are not
/// its class's, once for each choice of their types, the first varying fastest, at most
/// [`OVERLOADS`]: the overloads a constrained variable stands for (adr:0036).
fn expanded(
    function: &ast::StmtFunctionDef,
    cx: Cx<'_>,
    write: impl Fn(Cx<'_>) -> Result<String, String>,
) -> Result<Vec<String>, String> {
    let mut named = Vec::new();
    let parameters = &function.parameters;
    let annotations = parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
        .filter_map(|p| p.parameter.annotation.as_deref())
        .chain(function.returns.as_deref());
    for annotation in annotations {
        names_in(annotation, &mut named);
    }
    let mut constrained: Vec<(&str, &[String])> = Vec::new();
    for name in named {
        if let Some(Var::Constrained(types)) = cx.vars.get(name)
            && !cx.own.iter().any(|o| o == name)
            && !constrained.iter().any(|(seen, _)| *seen == name)
            && !types.is_empty()
        {
            constrained.push((name, types));
        }
    }
    let spend = |line: String| {
        cx.spent.set(cx.spent.get() + line.len() + 1);
        if cx.spent.get() > LARGEST {
            Err(format!("the interface is past the {LARGEST} bytes lotml reads"))
        } else {
            Ok(line)
        }
    };
    if constrained.is_empty() {
        return write(cx).and_then(spend).map(|line| vec![line]);
    }
    let mut lines = Vec::new();
    let mut choice = vec![0; constrained.len()];
    loop {
        let fixed: Vec<(&str, &str)> =
            constrained.iter().zip(&choice).map(|((name, types), &i)| (*name, types[i].as_str())).collect();
        lines.push(write(Cx { fixed: &fixed, ..cx }).and_then(spend)?);
        if lines.len() == OVERLOADS {
            return Ok(lines);
        }
        let mut at = 0;
        loop {
            let Some(i) = choice.get_mut(at) else { return Ok(lines) };
            *i += 1;
            if *i < constrained[at].1.len() {
                break;
            }
            *i = 0;
            at += 1;
        }
    }
}

/// Every name `expr` mentions, in the order written.
fn names_in<'e>(expr: &'e Expr, out: &mut Vec<&'e str>) {
    match expr {
        Expr::Name(name) => out.push(name.id.as_str()),
        Expr::Subscript(subscript) => {
            names_in(&subscript.value, out);
            names_in(&subscript.slice, out);
        }
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|e| names_in(e, out)),
        Expr::List(list) => list.elts.iter().for_each(|e| names_in(e, out)),
        Expr::BinOp(op) => {
            names_in(&op.left, out);
            names_in(&op.right, out);
        }
        _ => {}
    }
}

/// The type parameters a signature written as `text` declares: the plain type variables it names
/// that are not its class's, in the order written, `[T, S]`, or nothing (adr:0036).
fn generics(text: &str, cx: Cx<'_>) -> Result<String, String> {
    let mut found: Vec<&str> = Vec::new();
    for word in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        if matches!(cx.vars.get(word), Some(Var::Plain))
            && !reserved(word)
            && !cx.own.iter().any(|o| o == word)
            && !found.contains(&word)
        {
            if found.len() == TYPE_PARAMS {
                return Err(format!("it has more than {TYPE_PARAMS} type parameters"));
            }
            found.push(word);
        }
    }
    Ok(if found.is_empty() { String::new() } else { format!("[{}]", found.join(", ")) })
}

/// The most type parameters a class or a signature is bound with, and the most types a
/// constrained variable is expanded into (adr:0036).
const TYPE_PARAMS: usize = 16;

/// The expression a subscript is of: `Generic` of `Generic[T]`, or the expression itself.
fn head(expr: &Expr) -> &Expr {
    match expr {
        Expr::Subscript(subscript) => &subscript.value,
        other => other,
    }
}

/// A class as an interface writes it: its bases the interface declares, its attributes (annotated,
/// and `@property`s), its constructor (its `__init__`, else `__new__`, else a base's), its methods
/// and its static and class methods; and a comment for each member it leaves out.
fn class_block(class: &ast::StmtClassDef, cx: Cx<'_>) -> (Vec<String>, Vec<String>) {
    let name = class.name.as_str();
    let classes = cx.classes;
    let own: &[String] = cx.params.get(name).map_or(&[], Vec::as_slice);
    let cx = Cx { this: Some(name), own, ..cx };
    // The class as its members name it: `Pattern[AnyStr]` for a generic one (adr:0036).
    let named = if own.is_empty() { name.to_string() } else { format!("{name}[{}]", own.join(", ")) };
    let bases: Vec<&str> =
        class.bases().iter().filter_map(name_of).filter(|b| classes.contains(*b) && *b != name).collect();
    let mut lines = vec![if bases.is_empty() {
        format!("class {named}:")
    } else {
        format!("class {named}({}):", bases.join(", "))
    }];
    let mut skipped = Vec::new();
    let mut dunders = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let members = module_level(&class.body);
    for statement in &members {
        if let Stmt::AnnAssign(declared) = statement
            && let Expr::Name(target) = &*declared.target
        {
            let attribute = target.id.as_str();
            if attribute.starts_with('_') || !seen.insert(attribute.to_string()) {
                continue;
            }
            if KEYWORDS.contains(&attribute) {
                skipped.push(format!("#   {name}.{attribute}: `{attribute}` is a LotML keyword"));
                continue;
            }
            let annotation = match &*declared.annotation {
                Expr::Subscript(wrapped) if matches!(name_of(&wrapped.value), Some("ClassVar" | "Final")) => {
                    &*wrapped.slice
                }
                other => other,
            };
            lines.push(format!("    {attribute}: {}", lotml_type(annotation, false, cx)));
        }
    }
    let defs: Vec<&ast::StmtFunctionDef> = members.iter().filter_map(|s| s.as_function_def_stmt()).collect();
    let runs = overload_runs(&class.body);
    let decorated = |f: &ast::StmtFunctionDef, what: &str| {
        f.decorator_list.iter().any(|d| match &d.expression {
            Expr::Attribute(attribute) => attribute.attr.as_str() == what,
            other => name_of(other) == Some(what),
        })
    };
    for f in &defs {
        let member = f.name.as_str();
        if member.starts_with('_') && !member.starts_with("__") {
            continue;
        }
        if decorated(f, "setter") || decorated(f, "deleter") || !seen.insert(member.to_string()) {
            continue;
        }
        if decorated(f, "property") || decorated(f, "cached_property") {
            if KEYWORDS.contains(&member) {
                skipped.push(format!("#   {name}.{member}: `{member}` is a LotML keyword"));
            } else {
                let returns = f.returns.as_deref().map_or_else(|| OBJECT.to_string(), |r| lotml_type(r, false, cx));
                lines.push(format!("    {member}: {returns}"));
            }
            continue;
        }
        if member == "__init__" || member == "__new__" {
            continue;
        }
        if member.starts_with("__") {
            dunders.push(member.to_string());
            continue;
        }
        let is_method = |f: &ast::StmtFunctionDef| !is_static(f) && !decorated(f, "classmethod");
        let signature = |f: &ast::StmtFunctionDef, cx: Cx<'_>| {
            params(f, !is_static(f), cx).and_then(|params| {
                let returns = f.returns.as_deref().map_or_else(|| OBJECT.to_string(), |r| lotml_type(r, false, cx));
                let params =
                    if is_method(f) { std::iter::once(receiver(f, name, cx)).chain(params).collect() } else { params };
                let params = params.join(", ");
                let generics = generics(&format!("{params} {returns}"), cx)?;
                Ok(format!("    fn {member}{generics}({params}) -> {returns} ! PyError"))
            })
        };
        let label = format!("{name}.{member}");
        let (written, left_out) = match runs.get(member) {
            Some(run) if is_overload(f) => {
                overloads(run, &label, |o| expanded(o, cx, |cx| signature(o, cx)), is_method)
            }
            _ => one(&label, expanded(f, cx, |cx| signature(f, cx))),
        };
        lines.extend(written);
        skipped.extend(left_out);
    }
    // The class's own constructor, its `__init__` else its `__new__`. One a class inherits is not
    // written again: the checker takes it from the base (adr:0034), so a stub of many subclasses
    // of one wide base writes each of them once.
    let own = defs
        .iter()
        .find(|f| f.name.as_str() == "__init__")
        .or_else(|| defs.iter().find(|f| f.name.as_str() == "__new__"));
    if let Some(init) = own {
        let constructor = |f: &ast::StmtFunctionDef, cx: Cx<'_>| {
            params(f, true, cx).and_then(|params| {
                let params = params.join(", ");
                Ok(format!("    fn {name}{}({params}) -> {named} ! PyError", generics(&params, cx)?))
            })
        };
        let label = format!("{name}.{}", init.name.as_str());
        let (written, left_out) = match runs.get(init.name.as_str()) {
            Some(run) if is_overload(init) => {
                overloads(run, &label, |o| expanded(o, cx, |cx| constructor(o, cx)), |_| false)
            }
            _ => one(&label, expanded(init, cx, |cx| constructor(init, cx))),
        };
        for (k, line) in written.into_iter().enumerate() {
            lines.insert(1 + k, line);
        }
        skipped.extend(left_out);
    }
    if !dunders.is_empty() {
        skipped.push(format!("#   {name}.{}: dunder methods, which operators would call", dunders.join(", ")));
    }
    if lines.len() == 1 {
        // A class with no member written: its header alone, which the parser reads as such.
        lines[0].pop();
    }
    (lines, skipped)
}

/// The LotML type of an annotation. A parameter may take an abstract collection, which a LotML
/// list, dict or set satisfies, while a result must be the concrete one LotML receives; a type
/// LotML has none for is a `PyObject`.
fn lotml_type(expr: &Expr, parameter: bool, cx: Cx<'_>) -> String {
    if let Expr::NoneLiteral(_) = expr {
        return "None".to_string();
    }
    if let Expr::BinOp(union) = expr
        && union.op == Operator::BitOr
    {
        let rest: Vec<&Expr> =
            [&*union.left, &*union.right].into_iter().filter(|side| !matches!(side, Expr::NoneLiteral(_))).collect();
        return match rest.as_slice() {
            [one] => optional(lotml_type(one, parameter, cx)),
            _ => OBJECT.to_string(),
        };
    }
    if let Some(name) = name_of(expr) {
        if name == "Self"
            && let Some(this) = cx.this
        {
            return if cx.own.is_empty() { this.to_string() } else { format!("{this}[{}]", cx.own.join(", ")) };
        }
        if cx.classes.contains(name) {
            // A generic class named bare takes a `PyObject` for each parameter (adr:0036).
            return instantiated(name, &[], parameter, cx);
        }
        // A type variable: the class's own parameter or a plain one by its name, a constrained one
        // once its overloads give it each type, any other a `PyObject` (adr:0036).
        if cx.own.iter().any(|o| o == name) {
            return name.to_string();
        }
        if let Some((_, ty)) = cx.fixed.iter().find(|(var, _)| *var == name) {
            return (*ty).to_string();
        }
        if let Some(var) = cx.vars.get(name) {
            return if *var == Var::Plain && !reserved(name) { name.to_string() } else { OBJECT.to_string() };
        }
        return match name {
            "int" => "int",
            "float" => "f64",
            "str" => "str",
            "bool" => "bool",
            "bytes" => "bytes",
            "SupportsIndex" | "SupportsInt" if parameter => "int",
            "SupportsFloat" if parameter => "f64",
            "StrPath" | "StrOrBytesPath" if parameter => "str",
            "ReadableBuffer" if parameter => "bytes",
            _ => OBJECT,
        }
        .to_string();
    }
    let Expr::Subscript(subscript) = expr else { return OBJECT.to_string() };
    let head = name_of(&subscript.value).unwrap_or_default();
    let args: Vec<&Expr> = match &*subscript.slice {
        Expr::Tuple(tuple) => tuple.elts.iter().collect(),
        one => vec![one],
    };
    let of = |arg: &Expr| lotml_type(arg, parameter, cx);
    match (head, args.as_slice()) {
        ("Optional", [inner]) => optional(of(inner)),
        ("list" | "List", [item, ..]) => format!("[{}]", of(item)),
        ("Sequence" | "MutableSequence" | "Iterable" | "Collection", [item, ..]) if parameter => {
            format!("[{}]", of(item))
        }
        ("set" | "Set" | "frozenset" | "FrozenSet", [item, ..]) => format!("{{{}}}", of(item)),
        ("AbstractSet" | "MutableSet", [item, ..]) if parameter => format!("{{{}}}", of(item)),
        ("dict" | "Dict", [key, value]) => format!("{{{}: {}}}", of(key), of(value)),
        ("Mapping" | "MutableMapping", [key, value]) if parameter => format!("{{{}: {}}}", of(key), of(value)),
        (class, given) if cx.classes.contains(class) => instantiated(class, given, parameter, cx),
        ("tuple" | "Tuple", items) => {
            if items.iter().any(|a| matches!(a, Expr::EllipsisLiteral(_))) {
                OBJECT.to_string()
            } else {
                format!("({})", items.iter().map(|a| of(a)).collect::<Vec<_>>().join(", "))
            }
        }
        _ => OBJECT.to_string(),
    }
}

/// The stub's class `class` with the arguments `given`, as many as it has parameters, a
/// `PyObject` for each missing (adr:0036).
fn instantiated(class: &str, given: &[&Expr], parameter: bool, cx: Cx<'_>) -> String {
    let wanted = cx.params.get(class).map_or(0, Vec::len);
    if wanted == 0 {
        return class.to_string();
    }
    let args: Vec<String> = (0..wanted)
        .map(|i| given.get(i).map_or_else(|| OBJECT.to_string(), |a| lotml_type(a, parameter, cx)))
        .collect();
    format!("{class}[{}]", args.join(", "))
}

/// A method's `self` as an interface writes it: with its annotation when that names the class,
/// `self: Pattern[str]`, which the checker fits the receiver to (adr:0036).
fn receiver(method: &ast::StmtFunctionDef, class: &str, cx: Cx<'_>) -> String {
    let parameters = &method.parameters;
    let first = parameters.posonlyargs.iter().chain(&parameters.args).next();
    match first.and_then(|p| p.parameter.annotation.as_deref()) {
        Some(annotation) if name_of(head(annotation)) == Some(class) => {
            format!("self: {}", lotml_type(annotation, true, cx))
        }
        _ => "self".to_string(),
    }
}

fn optional(inner: String) -> String {
    if inner.ends_with('?') || inner == "None" { inner } else { format!("{inner}?") }
}

/// A default LotML can write, as Python's `repr` writes it; any other is Python's, and the
/// parameter is only optional.
fn default(expr: &Expr) -> String {
    match expr {
        Expr::NoneLiteral(_) => "None".to_string(),
        Expr::BooleanLiteral(b) => if b.value { "True" } else { "False" }.to_string(),
        Expr::NumberLiteral(number) => match &number.value {
            Number::Int(int) => integer(&int.to_string()).unwrap_or_else(|| "todo()".to_string()),
            Number::Float(float) => float_repr(*float),
            Number::Complex { .. } => "todo()".to_string(),
        },
        Expr::StringLiteral(string) => format!("\"{}\"", escape(string.value.to_str())),
        _ => "todo()".to_string(),
    }
}

/// An integer literal's value in decimal: a small one ruff gives as digits, a large one as the
/// token written, which is read here when it is decimal.
fn integer(written: &str) -> Option<String> {
    let digits: String = written.chars().filter(|&c| c != '_').collect();
    digits.chars().all(|c| c.is_ascii_digit()).then(|| {
        let trimmed = digits.trim_start_matches('0');
        if trimmed.is_empty() { "0".to_string() } else { trimmed.to_string() }
    })
}

/// Python's `repr` of a float: the shortest digits that read back, in positional notation from
/// 1e-4 up to 1e16 and in exponent notation, signed and of two digits at least, outside it.
fn float_repr(value: f64) -> String {
    if value.is_infinite() {
        return if value > 0.0 { "inf" } else { "-inf" }.to_string();
    }
    if value.is_nan() {
        return "nan".to_string();
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let negative = mantissa.starts_with('-');
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if (-4..16).contains(&exponent) {
        let point = exponent + 1;
        let text = if point <= 0 {
            format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
        } else {
            let point = point as usize;
            let whole = format!("{digits:0<point$}");
            let (integer, fraction) = whole.split_at(point);
            format!("{integer}.{}", if fraction.is_empty() { "0" } else { fraction })
        };
        return format!("{sign}{text}");
    }
    let (first, rest) = digits.split_at(1);
    let mantissa = if rest.is_empty() { first.to_string() } else { format!("{first}.{rest}") };
    let exponent_sign = if exponent < 0 { '-' } else { '+' };
    format!("{sign}{mantissa}e{exponent_sign}{:02}", exponent.unsigned_abs())
}

/// A string from a stub as the inside of a LotML string literal: `\`, `"` and every character that
/// could end a line or hide one escaped, so a stub cannot add a line to the interface (R1.3).
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x100 && (c.is_control()) => out.push_str(&format!("\\x{:02x}", c as u32)),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}') || is_format(c) => {
                if (c as u32) > 0xffff {
                    out.push_str(&format!("\\U{:08x}", c as u32));
                } else {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// Whether `c` may stand in the interface's first line as it is: no control character, no line or
/// paragraph separator, and no space but the plain one.
fn printable(c: char) -> bool {
    !(c.is_control() || (c.is_whitespace() && c != ' ') || matches!(c, '\u{2028}' | '\u{2029}') || is_format(c))
}

/// Whether `c` is a Unicode format character (category Cf), the bidirectional controls among them:
/// invisible, and able to show a reader text in another order than it is read.
fn is_format(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(stub: &str) -> String {
        interface("m", stub, "m.pyi, the stub given").unwrap_or_else(|Refused(why)| panic!("{why}"))
    }

    fn functions(stub: &str) -> Vec<String> {
        bound(stub).lines().filter(|l| l.starts_with("fn ")).map(str::to_string).collect()
    }

    fn one(stub: &str) -> String {
        let mut found = functions(stub);
        assert_eq!(found.len(), 1, "{found:?}");
        found.remove(0)
    }

    #[test]
    fn the_interface_says_where_it_came_from_and_how_to_read_it() {
        let text = bound("def f(x: int) -> int: ...\n");
        assert_eq!(
            text,
            "# The Python module `m`, bound by `lotml bind` from m.pyi, the stub given.\n\
             # Do not edit: run `lotml bind` again.\n\
             # Every function returns `T ! PyError`: a stub does not say what a call raises.\n\
             # A parameter written `= todo()` is optional: Python supplies its default.\n\
             # A `PyObject` is a value no LotML type describes: convert it with `value()`.\n\
             fn f(x: int) -> int ! PyError\n"
        );
    }

    #[test]
    fn simple_types_map_to_lotml_s() {
        assert_eq!(
            one("def f(a: int, b: float, c: str, d: bool, e: bytes) -> None: ...\n"),
            "fn f(a: int, b: f64, c: str, d: bool, e: bytes) -> None ! PyError"
        );
    }

    #[test]
    fn an_optional_is_marked_once_and_any_other_union_is_a_pyobject() {
        assert_eq!(
            one("def f(a: int | None, b: Optional[str]) -> int | None: ...\n"),
            "fn f(a: int?, b: str?) -> int? ! PyError"
        );
        assert_eq!(one("def f(a: int | str) -> None: ...\n"), "fn f(a: PyObject) -> None ! PyError");
        assert_eq!(one("def f(a: int | str | None) -> None: ...\n"), "fn f(a: PyObject?) -> None ! PyError");
        assert_eq!(one("def f(a: Optional[int | None]) -> None: ...\n"), "fn f(a: int?) -> None ! PyError");
    }

    #[test]
    fn a_parameter_takes_an_abstract_collection_and_a_result_is_the_concrete_one() {
        assert_eq!(
            one(
                "def f(a: Sequence[int], b: Mapping[str, float], c: AbstractSet[str], d: Iterable[str]) -> list[int]: ...\n"
            ),
            "fn f(a: [int], b: {str: f64}, c: {str}, d: [str]) -> [int] ! PyError"
        );
        assert_eq!(one("def f() -> Sequence[int]: ...\n"), "fn f() -> PyObject ! PyError");
        assert_eq!(
            one("def f(a: dict[str, int], b: set[int], c: frozenset[str], d: List[str]) -> Dict[str, int]: ...\n"),
            "fn f(a: {str: int}, b: {int}, c: {str}, d: [str]) -> {str: int} ! PyError"
        );
    }

    #[test]
    fn a_tuple_is_fixed_and_one_of_any_length_is_a_pyobject() {
        assert_eq!(one("def f(a: tuple[int, str]) -> Tuple[int]: ...\n"), "fn f(a: (int, str)) -> (int) ! PyError");
        assert_eq!(one("def f(a: tuple[int, ...]) -> None: ...\n"), "fn f(a: PyObject) -> None ! PyError");
    }

    #[test]
    fn what_a_parameter_accepts_is_what_lotml_passes_and_a_result_of_it_is_a_pyobject() {
        assert_eq!(
            one("def f(a: SupportsIndex, b: SupportsFloat, c: StrPath, d: ReadableBuffer) -> SupportsInt: ...\n"),
            "fn f(a: int, b: f64, c: str, d: bytes) -> PyObject ! PyError"
        );
    }

    #[test]
    fn a_name_lotml_has_no_type_for_is_a_pyobject_and_so_is_an_annotation_left_out() {
        assert_eq!(
            one("def f(a: Callable[[int], int], b, c: 'int') -> Any: ...\n"),
            "fn f(a: PyObject, b: PyObject, c: PyObject) -> PyObject ! PyError"
        );
        assert_eq!(one("def f(a: typing.Optional[builtins.int]): ...\n"), "fn f(a: int?) -> PyObject ! PyError");
    }

    #[test]
    fn defaults_are_written_as_python_writes_them_or_left_to_python() {
        assert_eq!(
            one("def f(a: int = 1, b: float = 1.5, c: str = 'x', d: bool = True, e: int | None = None) -> None: ...\n"),
            "fn f(a: int = 1, b: f64 = 1.5, c: str = \"x\", d: bool = True, e: int? = None) -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: int = -1, b: bytes = b'x', c: int = ..., d: int = SOME, e: complex = 1j) -> None: ...\n"),
            "fn f(a: int = todo(), b: bytes = todo(), c: int = todo(), d: int = todo(), e: PyObject = todo()) -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: int = 0x10, b: int = 1_000) -> None: ...\n"),
            "fn f(a: int = 16, b: int = 1000) -> None ! PyError"
        );
    }

    #[test]
    fn a_float_default_is_python_s_repr() {
        let written = |literal: &str| {
            let line = one(&format!("def f(a: float = {literal}) -> None: ...\n"));
            line["fn f(a: f64 = ".len()..line.len() - ") -> None ! PyError".len()].to_string()
        };
        for (literal, repr) in [
            ("1.0", "1.0"),
            ("100.0", "100.0"),
            ("0.5", "0.5"),
            ("0.0001", "0.0001"),
            ("0.00001", "1e-05"),
            ("1e16", "1e+16"),
            ("1e15", "1000000000000000.0"),
            ("123456789012345678.0", "1.2345678901234568e+17"),
            ("1.5e-7", "1.5e-07"),
            ("1e300", "1e+300"),
            ("0.0", "0.0"),
            ("1e999", "inf"),
        ] {
            assert_eq!(written(literal), repr, "{literal}");
        }
    }

    #[test]
    fn a_string_default_escapes_what_would_end_it_or_its_line() {
        assert_eq!(
            one("def f(a: str = 'say \"hi\"\\\\\\n') -> None: ...\n"),
            "fn f(a: str = \"say \\\"hi\\\"\\\\\\n\") -> None ! PyError"
        );
        assert_eq!(
            one("def f(a: str = '\\t\\r\\x07\\u2028\\x85') -> None: ...\n"),
            "fn f(a: str = \"\\x09\\x0d\\x07\\u2028\\x85\") -> None ! PyError"
        );
    }

    #[test]
    fn star_parameters_are_left_to_python_and_the_named_ones_bound() {
        assert_eq!(
            one("def f(a: int, /, b: str, *args: int, c: bool = False, **kw: int) -> None: ...\n"),
            "fn f(a: int, b: str, c: bool = False) -> None ! PyError"
        );
    }

    #[test]
    fn version_blocks_are_walked_and_the_first_definition_wins() {
        let stub = "import sys\nif sys.version_info >= (3, 12):\n    def f(a: int) -> int: ...\nelif sys.version_info >= (3, 10):\n    def f(a: str) -> str: ...\n    def g() -> None: ...\nelse:\n    def h() -> None: ...\n";
        assert_eq!(
            functions(stub),
            ["fn f(a: int) -> int ! PyError", "fn g() -> None ! PyError", "fn h() -> None ! PyError"]
        );
    }

    const DATES: &str = "\
from typing import Self, overload
class date:
    min: ClassVar[date]
    year: int
    def __init__(self, year: int, month: int) -> None: ...
    @classmethod
    def today(cls) -> Self: ...
    @staticmethod
    def parse(text: str) -> date: ...
    def isoformat(self) -> str: ...
    @property
    def month(self) -> int: ...
    @month.setter
    def month(self, value: int) -> None: ...
    def __eq__(self, other: object) -> bool: ...
    def __lt__(self, other: date) -> bool: ...
    def _hidden(self) -> None: ...
class datetime(date):
    def now(self) -> datetime: ...
class empty: ...
def combine(d: date, t: datetime) -> datetime: ...
";

    #[test]
    fn a_class_is_written_with_its_constructor_attributes_and_methods() {
        let text = bound(DATES);
        let block: Vec<&str> = text.lines().skip_while(|l| *l != "class date:").take_while(|l| !l.is_empty()).collect();
        assert_eq!(
            block,
            [
                "class date:",
                "    fn date(year: int, month: int) -> date ! PyError",
                "    min: date",
                "    year: int",
                "    fn today() -> date ! PyError",
                "    fn parse(text: str) -> date ! PyError",
                "    fn isoformat(self) -> str ! PyError",
                "    month: int",
            ]
        );
        assert!(
            text.contains("\nclass datetime(date):\n    fn now(self) -> datetime ! PyError\n"),
            "an inherited constructor is not written again; the checker takes it from the base: {text}"
        );
        assert!(text.contains("\nclass empty\n"), "a class with no member is its header alone: {text}");
        assert!(text.contains("#   date.__eq__, __lt__: dunder methods, which operators would call"), "{text}");
        assert!(!text.contains("_hidden"), "a private member is left out");
    }

    #[test]
    fn many_subclasses_of_a_wide_base_write_their_constructor_once() {
        let params: Vec<String> = (0..200).map(|i| format!("p{i}: int")).collect();
        let mut stub = format!("class B:\n    def __init__(self, {}) -> None: ...\n", params.join(", "));
        for i in 0..500 {
            stub += &format!("class S{i}(B): ...\n");
        }
        let text = bound(&stub);
        assert_eq!(text.matches("p199: int").count(), 1, "the base's constructor, written once");
        assert!(text.len() < stub.len() * 2, "the interface grows as the stub does: {} for {}", text.len(), stub.len());
    }

    #[test]
    fn a_long_chain_of_classes_binds_in_time_linear_in_its_length() {
        let mut stub = String::from("class C0:\n    def __init__(self, n: int) -> None: ...\n");
        for i in 1..8000 {
            stub += &format!("class C{i}(C{}): ...\n", i - 1);
        }
        let started = std::time::Instant::now();
        let text = bound(&stub);
        assert!(text.contains("\nclass C7999(C7998)\n"), "every class bound");
        assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    }

    #[test]
    fn a_class_of_the_stub_is_written_by_its_name_instead_of_pyobject() {
        assert!(bound(DATES).contains("fn combine(d: date, t: datetime) -> datetime ! PyError"));
        let without = "def combine(d: date) -> date: ...\n";
        assert_eq!(one(without), "fn combine(d: PyObject) -> PyObject ! PyError", "a class the stub does not define");
    }

    #[test]
    fn a_class_or_member_lotml_cannot_hold_is_listed_with_the_reason() {
        let stub = "\
from typing import Generic, TypeVar, Protocol
T = TypeVar(\"T\")
class Box(Generic[T]):
    def get(self) -> T: ...
class Sized(Protocol):
    def size(self) -> int: ...
class str: ...
class Odd:
    def given(self, var: int) -> None: ...
    match: int
def f() -> None: ...
class f: ...
";
        let text = bound(stub);
        for reason in [
            "#   Sized: it is a protocol",
            "#   str: `str` is a name LotML keeps",
            "#   f: a function of the stub has its name",
            "#   Odd.given: its parameter `var` is a LotML keyword",
            "#   Odd.match: `match` is a LotML keyword",
        ] {
            assert!(text.contains(reason), "{reason}: {text}");
        }
        assert!(text.contains("\nclass Odd\n"), "{text}");
        assert!(
            text.contains("\nclass Box[T]:\n    fn get(self) -> T ! PyError\n"),
            "a generic class is bound: {text}"
        );
    }

    #[test]
    fn a_generic_class_is_bound_with_its_parameters_and_named_with_its_arguments() {
        let stub = "\
from typing import AnyStr, Generic, TypeVar
_T = TypeVar(\"_T\")
_KT = TypeVar(\"_KT\")
_VT = TypeVar(\"_VT\")
class Pattern(Generic[AnyStr]):
    pattern: AnyStr
    def __init__(self, pattern: AnyStr) -> None: ...
    @overload
    def search(self: Pattern[str], string: str) -> str | None: ...
    @overload
    def search(self: Pattern[bytes], string: bytes) -> bytes | None: ...
    def split(self, string: AnyStr) -> list[AnyStr]: ...
    def copy(self) -> Self: ...
class Table(Mapping[_KT, _VT], Generic[_VT, _KT]):
    def get(self, key: _KT) -> _VT: ...
class Items(Iterator[_T]):
    def first(self) -> _T: ...
def compile(pattern: AnyStr) -> Pattern[AnyStr]: ...
def loose() -> Pattern: ...
def table() -> Table[str, int]: ...
";
        let text = bound(stub);
        let pattern: Vec<&str> = text[text.find("class Pattern").unwrap()..].lines().take(7).collect();
        assert_eq!(
            pattern,
            [
                "class Pattern[AnyStr]:",
                "    fn Pattern(pattern: AnyStr) -> Pattern[AnyStr] ! PyError",
                "    pattern: AnyStr",
                "    fn search(self: Pattern[str], string: str) -> str? ! PyError",
                "    fn search(self: Pattern[bytes], string: bytes) -> bytes? ! PyError",
                "    fn split(self, string: AnyStr) -> [AnyStr] ! PyError",
                "    fn copy(self) -> Pattern[AnyStr] ! PyError",
            ],
            "{text}"
        );
        assert!(text.contains("\nclass Table[_VT, _KT]:\n    fn get(self, key: _KT) -> _VT ! PyError\n"), "{text}");
        assert!(text.contains("\nclass Items[_T]:\n    fn first(self) -> _T ! PyError\n"), "{text}");
        assert_eq!(
            functions(stub),
            [
                "fn compile(pattern: str) -> Pattern[str] ! PyError",
                "fn compile(pattern: bytes) -> Pattern[bytes] ! PyError",
                "fn loose() -> Pattern[PyObject] ! PyError",
                "fn table() -> Table[str, int] ! PyError",
            ]
        );
    }

    #[test]
    fn private_names_classes_and_nested_functions_are_not_bound() {
        let stub = "def _p() -> None: ...\nclass C:\n    def m(self) -> None: ...\ndef f() -> None:\n    def inner() -> None: ...\n";
        assert_eq!(functions(stub), ["fn f() -> None ! PyError"]);
    }

    #[test]
    fn a_coroutine_is_listed_with_the_reason_and_an_overload_bound_in_order() {
        let stub = "@overload\ndef p(x: int) -> int: ...\n@typing.overload\ndef p(x: str) -> str: ...\nasync def c() -> None: ...\ndef f() -> None: ...\n";
        let text = bound(stub);
        assert!(
            text.ends_with(
                "fn p(x: int) -> int ! PyError\nfn p(x: str) -> str ! PyError\nfn f() -> None ! PyError\n\n\
                 # Not bound:\n#   c: it is a coroutine\n"
            ),
            "{text}"
        );
    }

    #[test]
    fn overloads_are_bound_in_order_and_the_implementation_a_typed_module_writes_after_them_is_not() {
        let stub = "\
from typing import overload
@overload
def listdir(path: str | None = None) -> list[str]: ...
@overload
def listdir(path: bytes) -> list[bytes]: ...
@overload
def listdir(path: int) -> list[str]: ...
def listdir(path=None): ...
";
        assert_eq!(
            functions(stub),
            [
                "fn listdir(path: str? = None) -> [str] ! PyError",
                "fn listdir(path: bytes) -> [bytes] ! PyError",
                "fn listdir(path: int) -> [str] ! PyError",
            ]
        );
    }

    #[test]
    fn an_overload_that_cannot_be_bound_ends_the_ones_written_and_a_repeat_is_left_out() {
        let stub = "\
@overload
def p(x: int) -> int: ...
@overload
def p(x: StrPath) -> str: ...
@overload
def p(x: str) -> bytes: ...
@overload
def p(match: bytes) -> bytes: ...
@overload
def p(x: float) -> float: ...
";
        let text = bound(stub);
        let written: Vec<&str> = text.lines().filter(|l| l.starts_with("fn ")).collect();
        assert_eq!(written, ["fn p(x: int) -> int ! PyError", "fn p(x: str) -> str ! PyError"], "{text}");
        assert!(text.contains("#   p: overload 3 of 5: an earlier one takes the same parameters"), "{text}");
        assert!(
            text.contains(
                "#   p: overload 4 of 5: its parameter `match` is a LotML keyword; it and those after are left out"
            ),
            "{text}"
        );
        assert!(!text.contains("overload 5 of 5"), "one after the cut is not listed alone: {text}");
    }

    #[test]
    fn the_overloads_of_the_first_branch_that_declares_them_are_bound() {
        let stub = "\
import sys
if sys.version_info >= (3, 12):
    @overload
    def f(x: int) -> int: ...
    @overload
    def f(x: str) -> str: ...
else:
    @overload
    def f(x: int) -> int: ...
    @overload
    def f(x: bytes) -> bytes: ...
";
        assert_eq!(functions(stub), ["fn f(x: int) -> int ! PyError", "fn f(x: str) -> str ! PyError"]);
    }

    #[test]
    fn a_name_is_bound_with_at_most_its_limit_of_overloads() {
        let stub: String =
            (0..OVERLOADS + 6).map(|i| format!("@overload\ndef f(x: int, n{i}: int) -> int: ...\n")).collect();
        let text = bound(&stub);
        assert_eq!(functions(&stub).len(), OVERLOADS);
        assert!(text.contains("#   f: overloads 65 to 70: past the 64 lotml reads"), "{text}");
    }

    #[test]
    fn thousands_of_one_overload_repeated_bind_in_time_linear_in_the_stub() {
        let unit = "@overload\ndef f(x: int) -> int: ...\n";
        let method = "    @overload\n    def m(self, x: int) -> int: ...\n";
        let stub =
            format!("{}class C:\n{}_c: C\n{}", unit.repeat(20_000), method.repeat(20_000), "g = _c.m\n".repeat(5_000));
        let started = std::time::Instant::now();
        let text = bound(&stub);
        assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
        assert_eq!(functions(&stub), ["fn f(x: int) -> int ! PyError", "fn g(x: int) -> int ! PyError"], "{text}");
    }

    #[test]
    fn a_plain_or_bounded_type_variable_is_a_type_parameter_and_its_bound_is_not_written() {
        let stub = "\
from typing import TypeVar, Iterable
T = TypeVar(\"T\")
B = TypeVar(\"B\", bound=int)
def first(xs: list[T]) -> T: ...
def largest(xs: Iterable[B], default: T) -> B | T: ...
def keep(x: B) -> list[B]: ...
";
        assert_eq!(
            functions(stub),
            [
                "fn first[T](xs: [T]) -> T ! PyError",
                "fn largest[B, T](xs: [B], default: T) -> PyObject ! PyError",
                "fn keep[B](x: B) -> [B] ! PyError",
            ]
        );
    }

    #[test]
    fn a_type_variable_a_stub_imports_is_read_from_the_typeshed_lotml_carries() {
        let stub = "\
from _typeshed import SupportsRichComparisonT as _T
from typing import AnyStr
def nlargest(n: int, iterable: Iterable[_T]) -> list[_T]: ...
";
        assert_eq!(functions(stub), ["fn nlargest[_T](n: int, iterable: [_T]) -> [_T] ! PyError"]);
        let vars = type_vars(&ruff_python_parser::parse_unchecked_source(stub, PySourceType::Stub).syntax().body);
        assert_eq!(vars["AnyStr"], Var::Constrained(vec!["str".into(), "bytes".into()]));
        assert_eq!(vars["_T"], Var::Plain);
    }

    #[test]
    fn a_constrained_type_variable_is_an_overload_per_type() {
        let stub = "\
from typing import AnyStr
A = TypeVar(\"A\", int, str)
B = TypeVar(\"B\", bytes, float)
def escape(pattern: AnyStr, flags: int = 0) -> AnyStr: ...
def pair(a: A, b: B) -> A: ...
";
        assert_eq!(
            functions(stub),
            [
                "fn escape(pattern: str, flags: int = 0) -> str ! PyError",
                "fn escape(pattern: bytes, flags: int = 0) -> bytes ! PyError",
                "fn pair(a: int, b: bytes) -> int ! PyError",
                "fn pair(a: str, b: bytes) -> str ! PyError",
                "fn pair(a: int, b: f64) -> int ! PyError",
                "fn pair(a: str, b: f64) -> str ! PyError",
            ]
        );
    }

    #[test]
    fn a_constrained_overload_set_expands_within_the_limit() {
        let stub: String = (0..40)
            .map(|i| format!("@overload\ndef f(x: AnyStr, n{i}: int) -> AnyStr: ...\n"))
            .collect::<String>()
            .replace("@overload\ndef f(x: AnyStr, n0", "from typing import AnyStr\n@overload\ndef f(x: AnyStr, n0");
        let text = bound(&stub);
        assert_eq!(functions(&stub).len(), OVERLOADS, "{text}");
        assert!(text.contains("#   f: overloads 33 to 40: past the 64 lotml reads"), "{text}");
    }

    #[test]
    fn type_parameters_and_constraints_past_their_limit_are_not_bound() {
        let wide: Vec<String> = (0..=TYPE_PARAMS).map(|i| format!("T{i}")).collect();
        let declared: String = wide.iter().map(|t| format!("{t} = TypeVar(\"{t}\")\n")).collect();
        let stub = format!(
            "{declared}class Wide(Generic[{}]): ...\ndef f({}) -> None: ...\n\
             C = TypeVar(\"C\", {})\ndef g(x: C) -> C: ...\n",
            wide.join(", "),
            wide.iter().map(|t| format!("p{t}: {t}")).collect::<Vec<_>>().join(", "),
            (0..=TYPE_PARAMS).map(|_| "int").collect::<Vec<_>>().join(", "),
        );
        let text = bound(&stub);
        assert!(text.contains("#   Wide: it has more than 16 type parameters"), "{text}");
        assert!(text.contains("#   f: it has more than 16 type parameters"), "{text}");
        assert_eq!(functions(&stub), ["fn g(x: PyObject) -> PyObject ! PyError"], "too many constraints: a `PyObject`");
    }

    #[test]
    fn a_stub_whose_expansions_outgrow_the_limit_is_refused_while_written() {
        let def = "def f{i}(x: AnyStr, y: B, z: C, w: D, v: E, u: F) -> AnyStr: ...\n";
        let vars: String =
            ["B", "C", "D", "E", "F"].iter().map(|v| format!("{v} = TypeVar(\"{v}\", int, str)\n")).collect();
        let defs: String = (0..60_000).map(|i| def.replace("{i}", &i.to_string())).collect();
        let stub = format!("from typing import AnyStr\n{vars}{defs}");
        let started = std::time::Instant::now();
        let refused = interface("m", &stub, "m.pyi").unwrap_err();
        assert!(refused.0.contains("past the"), "{}", refused.0);
        assert!(started.elapsed().as_secs() < 10, "took {:?}", started.elapsed());
    }

    /// The interface of `module`, a stub of `package`, its re-exports read from `files`: each
    /// (module, package, text).
    fn reexporting(module: &str, package: &str, stub: &str, files: &[(&str, &str, &str)]) -> String {
        let mut read = |m: &str| {
            files.iter().find(|(name, _, _)| *name == m).map(|(_, p, t)| ((*t).to_string(), (*p).to_string()))
        };
        let parts = sources(package, stub, &mut read).unwrap_or_else(|Refused(why)| panic!("{why}"));
        interface_with(module, stub, "m.pyi", &parts).unwrap_or_else(|Refused(why)| panic!("{why}"))
    }

    fn fns(text: &str) -> Vec<&str> {
        text.lines().filter(|l| l.starts_with("fn ")).collect()
    }

    #[test]
    fn a_name_re_exported_from_a_submodule_is_bound_as_the_module_s_own() {
        let stub = "from .core import contents, where, _private\n__all__ = [\"contents\", \"where\"]\n";
        let core = "def where() -> str: ...\ndef contents() -> str: ...\ndef _private() -> int: ...\n";
        let text = reexporting("certifi", "certifi", stub, &[("certifi.core", "certifi", core)]);
        assert_eq!(fns(&text), ["fn contents() -> str ! PyError", "fn where() -> str ! PyError"], "{text}");
        let names = "from .x import parse as parse, other\nfrom .x import helper as renamed\n__all__ = [\"renamed\"]\n";
        let x = "def parse(s: str) -> int: ...\ndef other() -> None: ...\ndef helper(n: int) -> int: ...\n";
        let text = reexporting("p", "p", names, &[("p.x", "p", x)]);
        assert_eq!(fns(&text), ["fn parse(s: str) -> int ! PyError", "fn renamed(n: int) -> int ! PyError"], "{text}");
    }

    #[test]
    fn a_star_import_brings_the_source_s_public_names_followed_through_its_own_re_exports() {
        let stub =
            "import sys\nif sys.platform == \"win32\":\n    from ntpath import *\nelse:\n    from posixpath import *\n";
        let posix = "from genericpath import commonprefix as commonprefix\n__all__ = [\"join\", \"commonprefix\"]\ndef join(a: str, *paths: str) -> str: ...\ndef hidden() -> None: ...\n";
        let generic = "def commonprefix(m: list[str]) -> str: ...\n";
        let files = [("posixpath", "", posix), ("genericpath", "", generic)];
        let text = reexporting("os.path", "os", stub, &files);
        assert_eq!(
            fns(&text),
            ["fn join(a: str) -> str ! PyError", "fn commonprefix(m: [str]) -> str ! PyError"],
            "{text}"
        );
        assert!(text.contains("#   *: `ntpath`, which it re-exports from, has no stub lotml found"), "{text}");
    }

    #[test]
    fn a_re_exported_class_and_type_variable_are_the_module_s_own_and_its_own_name_wins() {
        let stub = "from .models import Match, first, where\n__all__ = [\"Match\", \"first\", \"where\"]\ndef where() -> int: ...\n";
        let models = "T = TypeVar(\"T\")\nclass Match:\n    def text(self) -> str: ...\ndef first(xs: list[T]) -> T: ...\ndef where() -> str: ...\n";
        let text = reexporting("cn", "cn", stub, &[("cn.models", "cn", models)]);
        assert!(text.contains("\nclass Match:\n    fn text(self) -> str ! PyError\n"), "{text}");
        assert_eq!(fns(&text), ["fn where() -> int ! PyError", "fn first[T](xs: [T]) -> T ! PyError"], "{text}");
    }

    #[test]
    fn a_name_followed_past_the_depth_or_to_no_stub_is_listed_with_the_reason() {
        let chain = |next: &str| format!("from {next} import f as f\n");
        let files = [
            ("m1", "", chain("m2")),
            ("m2", "", chain("m3")),
            ("m3", "", chain("m4")),
            ("m4", "", chain("m5")),
            ("m5", "", "def f() -> int: ...\n".to_string()),
        ];
        let files: Vec<(&str, &str, &str)> = files.iter().map(|(m, p, t)| (*m, *p, t.as_str())).collect();
        let text = reexporting("m", "", &chain("m1"), &files);
        assert!(text.contains("#   f: it is re-exported through more than 4 modules"), "{text}");
        let text = reexporting("m", "m", "from .gone import g as g\n", &[]);
        assert!(text.contains("#   g: `m.gone`, which it re-exports from, has no stub lotml found"), "{text}");
    }

    #[test]
    fn the_stubs_a_module_re_exports_from_are_read_once_each_and_within_the_limit() {
        let mut asked = Vec::new();
        let mut read = |m: &str| {
            asked.push(m.to_string());
            Some((format!("from {m} import *\nfrom .other import x as x\n"), String::new()))
        };
        let parts = sources("p", "from a import *\nfrom a import y as y\n", &mut read).unwrap();
        assert_eq!(parts.parts.len(), 1);
        assert_eq!(asked, ["a"], "a module re-exporting from itself is asked for once");
        let big = "#".repeat(LARGEST / 2 + 1);
        let mut huge = |_: &str| Some((big.clone(), String::new()));
        let Err(Refused(why)) = sources("p", "from a import *\nfrom b import *\n", &mut huge) else {
            panic!("read past the limit")
        };
        assert!(why.contains("past the"), "{why}");
    }

    #[test]
    fn many_names_looked_up_through_a_wide_star_import_take_time_linear_in_the_stubs() {
        let wanted: Vec<String> = (0..40).map(|i| format!("n{i}")).collect();
        let stub = format!(
            "from .a import {}\n__all__ = [{}]\n",
            wanted.join(", "),
            wanted.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ")
        );
        let wide: String = (0..20_000).map(|i| format!("def d{i}() -> None: ...\n")).collect();
        let files = [("p.a", "p", "from .b import *\n"), ("p.b", "p", wide.as_str())];
        let started = std::time::Instant::now();
        let text = reexporting("p", "p", &stub, &files);
        assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
        assert!(text.contains("#   n39: `p.a` neither defines nor re-exports it"), "{text}");
    }

    #[test]
    fn a_name_all_lists_that_is_no_identifier_never_reaches_the_interface() {
        let stub = "from .a import *\n";
        let a = "__all__ = [\"q\\nfn evil() -> int ! PyError #\", \"ok\"]\ndef ok() -> int: ...\n";
        let text = reexporting("p", "p", stub, &[("p.a", "p", a)]);
        assert_eq!(fns(&text), ["fn ok() -> int ! PyError"], "{text}");
        assert!(!text.contains("evil"), "{text}");
    }

    #[test]
    fn one_definition_re_exported_under_two_names_gives_both() {
        let stub = "from .x import f as a, f as b\n__all__ = [\"a\", \"b\"]\n";
        let text = reexporting("p", "p", stub, &[("p.x", "p", "def f(n: int) -> int: ...\n")]);
        assert_eq!(fns(&text), ["fn a(n: int) -> int ! PyError", "fn b(n: int) -> int ! PyError"], "{text}");
    }

    #[test]
    fn a_stub_naming_more_modules_than_the_limit_to_re_export_from_is_refused() {
        let stub: String = (0..=REEXPORT_MODULES).map(|i| format!("from m{i} import *\n")).collect();
        let mut read = |_: &str| Some((String::new(), String::new()));
        let Err(Refused(why)) = sources("", &stub, &mut read) else { panic!("read past the limit") };
        assert!(why.contains("more than 256 modules"), "{why}");
    }

    #[test]
    fn a_relative_import_is_made_absolute_against_the_package() {
        assert_eq!(absolute("certifi", 1, Some("core")).as_deref(), Some("certifi.core"));
        assert_eq!(absolute("dateutil.parser", 1, Some("_parser")).as_deref(), Some("dateutil.parser._parser"));
        assert_eq!(absolute("a.b", 2, Some("c")).as_deref(), Some("a.c"));
        assert_eq!(absolute("a", 2, Some("c")), None, "past the root");
        assert_eq!(absolute("a", 1, None), None, "`from . import m` imports a module");
        assert_eq!(absolute("", 0, Some("posixpath")).as_deref(), Some("posixpath"));
    }

    #[test]
    fn a_positional_only_parameter_written_with_two_underscores_is_written_without() {
        assert_eq!(
            one("def f(__type1: int, __x__: str) -> int: ...\n"),
            "fn f(type1: int, __x__: str) -> int ! PyError"
        );
    }

    #[test]
    fn a_param_spec_or_type_var_tuple_is_a_py_object() {
        let stub = "\
P = ParamSpec(\"P\")
Ts = TypeVarTuple(\"Ts\")
def spread(x: Ts, p: P) -> Ts: ...
";
        assert_eq!(functions(stub), ["fn spread(x: PyObject, p: PyObject) -> PyObject ! PyError"]);
    }

    #[test]
    fn a_generic_method_declares_its_own_type_parameters() {
        let stub = "T = TypeVar(\"T\")\nclass C:\n    def __init__(self, seed: T) -> None: ...\n    def pick(self, xs: list[T]) -> T: ...\n";
        let text = bound(stub);
        assert!(text.contains("    fn C[T](seed: T) -> C ! PyError\n"), "{text}");
        assert!(text.contains("    fn pick[T](self, xs: [T]) -> T ! PyError\n"), "{text}");
    }

    #[test]
    fn a_class_s_overloaded_constructor_methods_and_aliased_methods_are_bound() {
        let stub = "\
class Path:
    @overload
    def __init__(self, text: str) -> None: ...
    @overload
    def __init__(self, parts: list[str]) -> None: ...
    @overload
    def joined(self, other: str) -> Path: ...
    @overload
    def joined(self, other: Path) -> Path: ...
    @overload
    def pick(self, x: int) -> int: ...
    @overload
    @staticmethod
    def pick(x: str) -> str: ...
_inst: Path
joined = _inst.joined
";
        let text = bound(stub);
        let class: Vec<&str> = text[text.find("class Path:").unwrap()..].lines().take(6).collect();
        assert_eq!(
            class,
            [
                "class Path:",
                "    fn Path(text: str) -> Path ! PyError",
                "    fn Path(parts: [str]) -> Path ! PyError",
                "    fn joined(self, other: str) -> Path ! PyError",
                "    fn joined(self, other: Path) -> Path ! PyError",
                "    fn pick(self, x: int) -> int ! PyError",
            ],
            "{text}"
        );
        assert!(text.contains("#   Path.pick: overload 2 of 2: it takes `self` where the first does not"), "{text}");
        assert_eq!(
            functions(stub),
            ["fn joined(other: str) -> Path ! PyError", "fn joined(other: Path) -> Path ! PyError"],
            "an instance's overloaded method, bound as a function"
        );
    }

    #[test]
    fn a_name_written_as_an_instance_s_method_is_bound_from_the_method() {
        let stub = "class Random:\n    def randint(self, a: int, b: int) -> int: ...\n    @staticmethod\n    def make(seed: int) -> float: ...\n    @classmethod\n    def named(cls, name: str) -> str: ...\n    def __init__(self) -> None: ...\n_inst: Random\nrandint = _inst.randint\nmake = _inst.make\nnamed = _inst.named\ngone = _inst.missing\n";
        let text = bound(stub);
        let found: Vec<&str> = text.lines().filter(|l| l.starts_with("fn ")).collect();
        assert_eq!(
            found,
            [
                "fn randint(a: int, b: int) -> int ! PyError",
                "fn make(seed: int) -> f64 ! PyError",
                "fn named(name: str) -> str ! PyError"
            ]
        );
        assert!(text.contains("#   gone: `Random` holds no `missing` in this stub\n"), "{text}");
    }

    #[test]
    fn a_method_of_an_instance_a_version_block_declares_counts_the_first_written() {
        let stub = "import sys\nclass R:\n    if sys.version_info >= (3, 12):\n        def r(self, a: int) -> int: ...\n    else:\n        def r(self, a: str) -> str: ...\n_i: R\nr = _i.r\n";
        assert_eq!(functions(stub), ["fn r(a: int) -> int ! PyError"]);
    }

    #[test]
    fn a_name_bound_once_is_not_bound_again() {
        assert_eq!(
            functions("def f(a: int) -> int: ...\ndef f(a: str) -> str: ...\n"),
            ["fn f(a: int) -> int ! PyError"]
        );
    }

    #[test]
    fn the_source_named_can_never_add_a_line() {
        let text = interface("m", "def f() -> None: ...\n", "x\nfn evil() -> int ! PyError\r\u{85}y").unwrap();
        assert_eq!(
            text.lines().next(),
            Some("# The Python module `m`, bound by `lotml bind` from x?fn evil() -> int ! PyError??y.")
        );
        assert_eq!(text.lines().filter(|l| l.starts_with("fn ")).count(), 1);
    }

    fn refused(stub: &str) -> String {
        match interface("m", stub, "m.pyi") {
            Err(Refused(why)) => why,
            Ok(text) => panic!("bound:\n{text}"),
        }
    }

    #[test]
    fn a_stub_past_the_size_lotml_reads_is_refused() {
        let stub = format!("def f() -> None: ...\n{}", "#".repeat(LARGEST));
        assert!(refused(&stub).contains("bytes, past the"));
    }

    #[test]
    fn a_stub_nesting_past_the_limit_is_refused_saying_where() {
        let deep = format!("def f() -> None: ...\nx: {}int{}\n", "list[".repeat(DEEPEST + 1), "]".repeat(DEEPEST + 1));
        let why = refused(&deep);
        assert!(why.contains("nests deeper") && why.contains("line 2"), "{why}");
        let at_limit =
            format!("def f(a: {}int{}) -> None: ...\n", "list[".repeat(DEEPEST - 1), "]".repeat(DEEPEST - 1));
        assert_eq!(functions(&at_limit).len(), 1, "the parameters' parenthesis and the lists, at the limit, bind");
    }

    #[test]
    fn blocks_nested_past_the_limit_are_refused() {
        let mut stub = String::new();
        for depth in 0..=DEEPEST {
            stub.push_str(&format!("{}if sys.version_info >= (3, {depth}):\n", "  ".repeat(depth)));
        }
        stub.push_str(&format!("{}def f() -> None: ...\n", "  ".repeat(DEEPEST + 1)));
        assert!(refused(&stub).contains("nests deeper"));
    }

    #[test]
    fn a_line_past_the_token_limit_is_refused_and_one_within_it_binds() {
        let long = format!("def f(a: int = {}1) -> None: ...\n", "1 + ".repeat(LONGEST_LINE));
        assert!(refused(&long).contains("more than"), "a line that long builds a tree that deep");
        let within = format!("def f(a: int = {}1) -> None: ...\n", "1 + ".repeat(LONGEST_LINE / 2 - 20));
        assert_eq!(one(&within), "fn f(a: int = todo()) -> None ! PyError", "a deep tree is walked and dropped");
    }

    #[test]
    fn a_class_of_many_methods_aliased_many_times_binds_in_linear_time() {
        let count = 20_000;
        let mut stub = String::from("class C:\n");
        for k in 0..count {
            stub.push_str(&format!("    def m{k}(self) -> int: ...\n"));
        }
        stub.push_str("_i: C\n");
        for k in 0..count {
            stub.push_str(&format!("a{k} = _i.m{k}\n"));
        }
        let started = std::time::Instant::now();
        assert_eq!(functions(&stub).len(), count);
        assert!(started.elapsed().as_secs() < 10, "{:?}", started.elapsed());
    }

    #[test]
    fn blank_and_comment_lines_count_as_lines_of_their_own() {
        let stub = format!("{}def f() -> None: ...\n", "# c\n\n".repeat(LONGEST_LINE));
        assert_eq!(functions(&stub), ["fn f() -> None ! PyError"]);
    }

    #[test]
    fn an_invisible_or_reordering_character_is_escaped_and_kept_off_the_first_line() {
        assert_eq!(
            one("def f(a: str = '\u{202e}x\u{200b}\u{e0041}') -> None: ...\n"),
            "fn f(a: str = \"\\u202ex\\u200b\\U000e0041\") -> None ! PyError"
        );
        let text = interface("m", "def f() -> None: ...\n", "a\u{2066}b").unwrap();
        assert!(text.lines().next().unwrap().ends_with("from a?b."), "{text}");
    }

    #[test]
    fn a_stub_that_does_not_parse_is_refused_saying_where() {
        let Err(Refused(why)) = interface("m", "def f(:\n", "m.pyi") else { panic!("not Python") };
        assert!(why.contains("line 1"), "{why}");
    }
}
