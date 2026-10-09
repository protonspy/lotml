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
}

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
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(STACK)
            .spawn_scoped(scope, || bind(module, stub, said))
            .map_err(|e| Refused(format!("cannot start the binder: {e}")))?
            .join()
            .map_err(|_| Refused("the binder stopped on this stub".into()))?
    })
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
fn bind(module: &str, stub: &str, said: &str) -> Result<String, Refused> {
    let parsed = ruff_python_parser::parse_unchecked_source(stub, PySourceType::Stub);
    if let Some(error) = parsed.errors().first() {
        let at = usize::from(error.location.start()).min(stub.len());
        let line = stub.as_bytes()[..at].iter().filter(|&&b| b == b'\n').count() + 1;
        return Err(Refused(format!("the stub does not parse, at line {line}: {}", error.error)));
    }
    let body = &parsed.syntax().body;
    let mut bound = Vec::new();
    let mut skipped = Vec::new();
    let (classes, refused) = bindable_classes(body);
    skipped.extend(refused);
    let cx = Cx { classes: &classes, this: None };
    let mut seen = HashSet::new();
    for (name, lines, left_out) in functions(body, cx) {
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
            let (block, left_out) = class_block(class, &classes);
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
        return Err(Refused(format!("its interface would be past the {LARGEST} bytes lotml reads")));
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
    let mut found: Vec<(String, Vec<String>, Vec<String>)> = definitions(body)
        .into_iter()
        .map(|f| {
            let name = f.name.as_str();
            let (lines, left_out) = match runs.get(name) {
                Some(run) if is_overload(f) => overloads(run, name, |o| written(o, name, false, cx), |_| false),
                _ => one(name, written(f, name, false, cx)),
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
        let index = methods.get(class.as_str());
        let first = index.and_then(|(index, _)| index.get(method.as_str()));
        let run = index.and_then(|(_, runs)| runs.get(method.as_str()));
        let (lines, left_out) = match (first, run) {
            (None, _) => one(&name, Err(format!("`{class}` holds no `{method}` in this stub"))),
            (Some(m), Some(run)) if is_overload(m) => {
                overloads(run, &name, |o| written(o, &name, !is_static(o), cx), |o| !is_static(o))
            }
            (Some(m), _) => one(&name, written(m, &name, !is_static(m), cx)),
        };
        found.push((name, lines, left_out));
    }
    found
}

/// A name's one signature as lines of the interface, or the comment saying why it is left out.
fn one(name: &str, written: Result<String, String>) -> (Vec<String>, Vec<String>) {
    match written {
        Ok(line) => (vec![line], Vec::new()),
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
    write: impl Fn(&ast::StmtFunctionDef) -> Result<String, String>,
    receiver: impl Fn(&ast::StmtFunctionDef) -> bool,
) -> (Vec<String>, Vec<String>) {
    let mut lines: Vec<String> = Vec::new();
    let mut taken = HashSet::new();
    let mut skipped = Vec::new();
    let total = run.len();
    for (k, f) in run.iter().enumerate() {
        let which = format!("#   {label}: overload {} of {total}", k + 1);
        if lines.len() == OVERLOADS {
            skipped.push(format!("#   {label}: overloads {} to {total}: past the {OVERLOADS} lotml reads", k + 1));
            break;
        }
        if receiver(f) != receiver(run[0]) {
            skipped.push(format!("{which}: it takes `self` where the first does not; it and those after are left out"));
            break;
        }
        match write(f) {
            Ok(line) => {
                let parameters = line.split(" -> ").next().unwrap_or(&line).to_string();
                if taken.insert(parameters) {
                    lines.push(line);
                } else {
                    skipped.push(format!("{which}: an earlier one takes the same parameters, so no call is given it"));
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
    Ok(format!("fn {name}({}) -> {returns} ! PyError", params.join(", ")))
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
        let name = p.parameter.name.as_str();
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

/// The classes of the stub the interface declares, and a comment for each public one it does
/// not: generic, a protocol, of a name LotML keeps, or of a function's name.
fn bindable_classes(body: &[Stmt]) -> (HashSet<String>, Vec<String>) {
    let type_vars = type_vars(body);
    let functions: HashSet<&str> = definitions(body).iter().map(|f| f.name.as_str()).collect();
    let mut bound = HashSet::new();
    let mut refused = Vec::new();
    for class in module_level(body).into_iter().filter_map(Stmt::as_class_def_stmt) {
        let name = class.name.as_str();
        if name.starts_with('_') || bound.contains(name) {
            continue;
        }
        let why = if class.type_params.is_some() || class.bases().iter().any(|b| mentions(b, &type_vars)) {
            Some("it is generic, which specs/python-generics/ binds".to_string())
        } else if class.bases().iter().any(|b| name_of(head(b)) == Some("Protocol")) {
            Some("it is a protocol, a shape rather than a class".to_string())
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
            }
        }
    }
    (bound, refused)
}

/// The names the stub declares as type variables: `T = TypeVar("T")`.
fn type_vars(body: &[Stmt]) -> HashSet<String> {
    let mut found = HashSet::new();
    for statement in module_level(body) {
        if let Stmt::Assign(assigned) = statement
            && let [Expr::Name(target)] = assigned.targets.as_slice()
            && let Expr::Call(call) = &*assigned.value
            && matches!(name_of(&call.func), Some("TypeVar" | "ParamSpec" | "TypeVarTuple"))
        {
            found.insert(target.id.to_string());
        }
    }
    found
}

/// The expression a subscript is of: `Generic` of `Generic[T]`, or the expression itself.
fn head(expr: &Expr) -> &Expr {
    match expr {
        Expr::Subscript(subscript) => &subscript.value,
        other => other,
    }
}

/// Whether `expr` is `Generic[…]`, or names one of `type_vars` anywhere inside it.
fn mentions(expr: &Expr, type_vars: &HashSet<String>) -> bool {
    match expr {
        Expr::Name(name) => type_vars.contains(name.id.as_str()),
        Expr::Subscript(subscript) => {
            name_of(&subscript.value) == Some("Generic") || mentions(&subscript.slice, type_vars)
        }
        Expr::Tuple(tuple) => tuple.elts.iter().any(|e| mentions(e, type_vars)),
        _ => false,
    }
}

/// A class as an interface writes it: its bases the interface declares, its attributes (annotated,
/// and `@property`s), its constructor (its `__init__`, else `__new__`, else a base's), its methods
/// and its static and class methods; and a comment for each member it leaves out.
fn class_block(class: &ast::StmtClassDef, classes: &HashSet<String>) -> (Vec<String>, Vec<String>) {
    let name = class.name.as_str();
    let cx = Cx { classes, this: Some(name) };
    let bases: Vec<&str> =
        class.bases().iter().filter_map(name_of).filter(|b| classes.contains(*b) && *b != name).collect();
    let mut lines =
        vec![if bases.is_empty() { format!("class {name}:") } else { format!("class {name}({}):", bases.join(", ")) }];
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
        let signature = |f: &ast::StmtFunctionDef| {
            params(f, !is_static(f), cx).map(|params| {
                let returns = f.returns.as_deref().map_or_else(|| OBJECT.to_string(), |r| lotml_type(r, false, cx));
                let params =
                    if is_method(f) { std::iter::once("self".to_string()).chain(params).collect() } else { params };
                format!("    fn {member}({}) -> {returns} ! PyError", params.join(", "))
            })
        };
        let label = format!("{name}.{member}");
        let (written, left_out) = match runs.get(member) {
            Some(run) if is_overload(f) => overloads(run, &label, signature, is_method),
            _ => one(&label, signature(f)),
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
        let constructor = |f: &ast::StmtFunctionDef| {
            params(f, true, cx).map(|params| format!("    fn {name}({}) -> {name} ! PyError", params.join(", ")))
        };
        let label = format!("{name}.{}", init.name.as_str());
        let (written, left_out) = match runs.get(init.name.as_str()) {
            Some(run) if is_overload(init) => overloads(run, &label, constructor, |_| false),
            _ => one(&label, constructor(init)),
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
            return this.to_string();
        }
        if cx.classes.contains(name) {
            return name.to_string();
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
            "#   Box: it is generic",
            "#   Sized: it is a protocol",
            "#   str: `str` is a name LotML keeps",
            "#   f: a function of the stub has its name",
            "#   Odd.given: its parameter `var` is a LotML keyword",
            "#   Odd.match: `match` is a LotML keyword",
        ] {
            assert!(text.contains(reason), "{reason}: {text}");
        }
        assert!(text.contains("\nclass Odd\n"), "{text}");
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
