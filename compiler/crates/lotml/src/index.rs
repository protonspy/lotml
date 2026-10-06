//! `lotml digest` and `lotml show` (R15): the project's index in context, and bodies on demand.
//!
//! The digest holds the types in full and every function's signature with its documentation
//! as comments above it — never a bodiless stub, which models read as a function to complete.
//! `show` gives the text of one symbol as written, then that of the functions and types it
//! uses directly. lotml has no visibility modifiers, so every top-level item is public.

use std::collections::BTreeSet;

use lotml_syntax::ast::*;
use lotml_syntax::parse;

/// One source file: what to call it, and its text.
pub struct Source {
    pub name: String,
    pub text: String,
}

pub fn digest(sources: &[Source]) -> String {
    let mut out = String::new();
    for source in sources {
        let module = parse(&source.text).module;
        if sources.len() > 1 {
            out += &format!("# {}\n\n", source.name);
        }
        let mut sections: Vec<String> = Vec::new();
        for item in &module.items {
            let section = match item {
                Item::Fn(f) => documented(&source.text, f, ""),
                Item::Record(_) | Item::Sum(_) => lotml_fmt::item(&source.text, item),
                Item::Trait(t) => {
                    let header = format!("trait {}{}:\n", t.name.name, type_params(&t.type_params));
                    header + &t.methods.iter().map(|m| documented(&source.text, m, "    ")).collect::<String>()
                }
                Item::Impl(imp) => {
                    let target = &source.text[imp.target.span.range()];
                    let header = match &imp.trait_name {
                        Some(t) => format!("impl {} for {target}:\n", &source.text[t.span.range()]),
                        None => format!("impl {target}:\n"),
                    };
                    header + &imp.methods.iter().map(|m| documented(&source.text, m, "    ")).collect::<String>()
                }
                Item::Import(_) | Item::Test(_) | Item::Error(_) => continue,
            };
            sections.push(section);
        }
        out += &sections.join("\n");
        if sources.len() > 1 {
            out.push('\n');
        }
    }
    out
}

/// A signature, its documentation as comment lines above it.
fn documented(text: &str, f: &FnDef, indent: &str) -> String {
    let mut out = String::new();
    if let Some(doc) = &f.doc {
        for line in dedent(doc) {
            out += format!("{indent}# {line}").trim_end();
            out.push('\n');
        }
    }
    out += &format!("{indent}{}\n", lotml_fmt::signature(text, f));
    out
}

/// A docstring's lines without their common indentation or the blank lines around them.
fn dedent(doc: &str) -> Vec<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let margin = lines.iter().skip(1).filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min();
    let mut out: Vec<String> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 0 {
                l.trim().to_string()
            } else {
                l.get(margin.unwrap_or(0)..).unwrap_or("").trim_end().to_string()
            }
        })
        .collect();
    while out.first().is_some_and(String::is_empty) {
        out.remove(0);
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

fn type_params(params: &[TypeParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let params: Vec<String> = params
        .iter()
        .map(|p| match &p.bound {
            Some(b) => format!("{}: {}", p.name.name, b.name),
            None => p.name.name.clone(),
        })
        .collect();
    format!("[{}]", params.join(", "))
}

/// What a symbol names: an item, or one method of an impl.
enum Found<'a> {
    Item(&'a Item),
    Method(&'a FnDef),
}

/// The text of `symbol` — `name` or `Type.method` — and of what it uses; or the names that
/// come closest when nothing is called that.
pub fn show(symbol: &str, sources: &[Source]) -> Result<String, Vec<String>> {
    let modules: Vec<Module> = sources.iter().map(|s| parse(&s.text).module).collect();
    let mut known = Vec::new();
    let mut target = None;
    for (i, module) in modules.iter().enumerate() {
        for item in &module.items {
            for (name, found) in names(item) {
                if name == symbol && target.is_none() {
                    target = Some((i, found));
                }
                known.push(name);
            }
        }
    }
    let Some((file, found)) = target else {
        known.sort();
        known.dedup();
        let mut close: Vec<String> = known.into_iter().filter(|k| near(k, symbol)).collect();
        close.truncate(5);
        return Err(close);
    };
    // From the start of the line, so an indented method keeps its shape once dedented.
    let text = |i: usize, start: u32, end: u32| {
        let source = &sources[i].text;
        let line = source[..start as usize].rfind('\n').map_or(0, |n| n + 1);
        dedent_block(source[line..end as usize].trim_end())
    };
    let (body, uses) = match found {
        Found::Item(item) => (text(file, item.span().start, item.end()), used(item)),
        Found::Method(m) => {
            let mut uses = BTreeSet::new();
            function_uses(m, &mut uses);
            // The type the method belongs to, which `self` names without spelling it.
            uses.insert(symbol.split('.').next().unwrap_or(symbol).to_string());
            (text(file, m.span.start, m.end()), uses)
        }
    };
    let mut out = format!("# {}\n{body}\n", sources[file].name);
    // Each item once, however many of its names are used: a sum type and its variants. A
    // method's own type is worth showing; an item is not shown twice.
    let mut shown: BTreeSet<(usize, u32)> = BTreeSet::new();
    if let Found::Item(item) = found {
        shown.insert((file, item.span().start));
    }
    for name in uses {
        for (i, module) in modules.iter().enumerate() {
            if let Some(item) = module.items.iter().find(|item| declares(item, &name)) {
                if shown.insert((i, item.span().start)) {
                    let text = text(i, item.span().start, item.end());
                    let declared = declared_name(item).unwrap_or(&name);
                    out += &format!("\n# uses {declared}, from {}\n{text}\n", sources[i].name);
                }
                break;
            }
        }
    }
    Ok(out)
}

fn near(a: &str, b: &str) -> bool {
    let (a, b) = (a.to_lowercase(), b.to_lowercase());
    a.contains(&b) || b.contains(&a) || {
        let common = a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count();
        common >= 3
    }
}

/// A block of lines without the indentation they all share.
fn dedent_block(text: &str) -> String {
    let margin =
        text.lines().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    text.lines().map(|l| l.get(margin..).unwrap_or(l.trim_start())).collect::<Vec<_>>().join("\n")
}

/// The name an item is declared under: a sum type's, not its variant's.
fn declared_name(item: &Item) -> Option<&str> {
    match item {
        Item::Fn(f) => Some(&f.name.name),
        Item::Record(r) => Some(&r.name.name),
        Item::Sum(s) => Some(&s.name.name),
        Item::Trait(t) => Some(&t.name.name),
        _ => None,
    }
}

/// The symbols an item answers to.
fn names(item: &Item) -> Vec<(String, Found<'_>)> {
    match item {
        Item::Fn(f) => vec![(f.name.name.clone(), Found::Item(item))],
        Item::Record(r) => vec![(r.name.name.clone(), Found::Item(item))],
        Item::Sum(s) => {
            let mut out = vec![(s.name.name.clone(), Found::Item(item))];
            out.extend(s.variants.iter().map(|v| (v.name.name.clone(), Found::Item(item))));
            out
        }
        Item::Trait(t) => vec![(t.name.name.clone(), Found::Item(item))],
        Item::Impl(imp) => {
            let target = match &imp.target.kind {
                TypeKind::Named { name, .. } => name.name.clone(),
                _ => return vec![],
            };
            imp.methods.iter().map(|m| (format!("{target}.{}", m.name.name), Found::Method(m))).collect()
        }
        Item::Import(_) | Item::Test(_) | Item::Error(_) => vec![],
    }
}

/// Whether `item` declares `name`: a function, a type, a trait, or the sum type a variant is of.
fn declares(item: &Item, name: &str) -> bool {
    match item {
        Item::Fn(f) => f.name.name == name,
        Item::Record(r) => r.name.name == name,
        Item::Sum(s) => s.name.name == name || s.variants.iter().any(|v| v.name.name == name),
        Item::Trait(t) => t.name.name == name,
        _ => false,
    }
}

/// The names an item mentions: in its types, its calls and its patterns.
fn used(item: &Item) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    match item {
        Item::Fn(f) => function_uses(f, &mut out),
        Item::Record(r) => r.fields.iter().for_each(|f| type_uses(&f.ty, &mut out)),
        Item::Sum(s) => {
            s.variants.iter().flat_map(|v| v.fields.iter().flatten()).for_each(|f| type_uses(&f.ty, &mut out))
        }
        Item::Trait(t) => t.methods.iter().for_each(|m| function_uses(m, &mut out)),
        _ => {}
    }
    out
}

fn function_uses(f: &FnDef, out: &mut BTreeSet<String>) {
    for bound in f.type_params.iter().filter_map(|p| p.bound.as_ref()) {
        out.insert(bound.name.clone());
    }
    for p in &f.params {
        if let Some(t) = &p.ty {
            type_uses(t, out);
        }
    }
    for t in f.returns.iter().chain(&f.error) {
        type_uses(t, out);
    }
    if let Some(body) = &f.body {
        block_uses(body, out);
    }
}

fn type_uses(t: &TypeExpr, out: &mut BTreeSet<String>) {
    match &t.kind {
        TypeKind::Named { name, args } => {
            out.insert(name.name.clone());
            args.iter().for_each(|a| type_uses(a, out));
        }
        TypeKind::List(a) | TypeKind::Set(a) | TypeKind::Optional(a) => type_uses(a, out),
        TypeKind::Dict(k, v) => {
            type_uses(k, out);
            type_uses(v, out);
        }
        TypeKind::Tuple(items) => items.iter().for_each(|a| type_uses(a, out)),
        TypeKind::Dyn(name) => {
            out.insert(name.name.clone());
        }
        TypeKind::Unit | TypeKind::Error => {}
    }
}

fn block_uses(block: &Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Return(Some(e)) => expr_uses(e, out),
            StmtKind::Var { ty, value, .. } => {
                ty.iter().for_each(|t| type_uses(t, out));
                expr_uses(value, out);
            }
            StmtKind::Annotated { ty, value, .. } => {
                type_uses(ty, out);
                expr_uses(value, out);
            }
            StmtKind::Assign { target, value } | StmtKind::AugAssign { target, value, .. } => {
                expr_uses(target, out);
                expr_uses(value, out);
            }
            StmtKind::Assert { test, message } => {
                expr_uses(test, out);
                message.iter().for_each(|m| expr_uses(m, out));
            }
            StmtKind::If { branches, orelse } => {
                for (test, body) in branches {
                    expr_uses(test, out);
                    block_uses(body, out);
                }
                orelse.iter().for_each(|b| block_uses(b, out));
            }
            StmtKind::While { test, body } => {
                expr_uses(test, out);
                block_uses(body, out);
            }
            StmtKind::For { iter, body, .. } => {
                expr_uses(iter, out);
                block_uses(body, out);
            }
            StmtKind::Match { subject, arms } => {
                expr_uses(subject, out);
                for arm in arms {
                    pattern_uses(&arm.pattern, out);
                    block_uses(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

fn pattern_uses(p: &Pattern, out: &mut BTreeSet<String>) {
    match &p.kind {
        PatternKind::Name(n) => {
            out.insert(n.name.clone());
        }
        PatternKind::Variant { name, args } => {
            out.insert(name.name.clone());
            args.iter().for_each(|a| pattern_uses(a, out));
        }
        PatternKind::Tuple(items) => items.iter().for_each(|a| pattern_uses(a, out)),
        _ => {}
    }
}

fn expr_uses(e: &Expr, out: &mut BTreeSet<String>) {
    let mut go = |x: &Expr| expr_uses(x, out);
    match &e.kind {
        ExprKind::Name(n) => {
            out.insert(n.clone());
        }
        ExprKind::Tuple(items) | ExprKind::List(items) | ExprKind::Set(items) => items.iter().for_each(go),
        ExprKind::Dict(pairs) => pairs.iter().for_each(|(k, v)| {
            expr_uses(k, out);
            expr_uses(v, out);
        }),
        ExprKind::ListComp { element, loops }
        | ExprKind::SetComp { element, loops }
        | ExprKind::Generator { element, loops } => {
            go(element);
            for l in loops {
                expr_uses(&l.iter, out);
                l.conditions.iter().for_each(|c| expr_uses(c, out));
            }
        }
        ExprKind::DictComp { key, value, loops } => {
            expr_uses(key, out);
            expr_uses(value, out);
            for l in loops {
                expr_uses(&l.iter, out);
                l.conditions.iter().for_each(|c| expr_uses(c, out));
            }
        }
        ExprKind::Unary { operand: x, .. } | ExprKind::Not(x) | ExprKind::Try(x) | ExprKind::Fail(x) => go(x),
        ExprKind::Lambda { body, .. } => go(body),
        ExprKind::Binary { left, right, .. } => {
            expr_uses(left, out);
            expr_uses(right, out);
        }
        ExprKind::Coalesce { value, default } => {
            expr_uses(value, out);
            expr_uses(default, out);
        }
        ExprKind::Compare { first, rest } => {
            expr_uses(first, out);
            rest.iter().for_each(|(_, r)| expr_uses(r, out));
        }
        ExprKind::Logical { operands, .. } => operands.iter().for_each(go),
        ExprKind::IfExp { test, then, orelse } => {
            expr_uses(test, out);
            expr_uses(then, out);
            expr_uses(orelse, out);
        }
        ExprKind::Call { func, args } => {
            expr_uses(func, out);
            args.iter().for_each(|a| expr_uses(a.expr(), out));
        }
        ExprKind::Index { object, index } => {
            expr_uses(object, out);
            expr_uses(index, out);
        }
        ExprKind::Slice { object, lower, upper, step } => {
            expr_uses(object, out);
            for part in [lower, upper, step].into_iter().flatten() {
                expr_uses(part, out);
            }
        }
        ExprKind::Attr { object, .. } => go(object),
        ExprKind::Str(literals) => {
            for literal in literals {
                for part in &literal.parts {
                    if let StrPart::Expr { expr, .. } = part {
                        expr_uses(expr, out);
                    }
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_docstring_loses_its_margin_and_blank_edges() {
        assert_eq!(dedent("\n    First line.\n\n    More.\n    "), vec!["First line.", "", "More."]);
        assert_eq!(dedent("One line."), vec!["One line."]);
    }
}
