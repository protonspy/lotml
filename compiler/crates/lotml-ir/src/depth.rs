//! The functions whose calls count toward the recursion limit (specs/recursion-depth R1.1): those
//! a call can nest without bound through. The program is read as lowered, before `mono`, so the
//! Python target, which reads it there, and the native one, whose instances `mono` copies from it,
//! count the same calls.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lotml_check::ty::Ty;

use lotml_syntax::span::Span;

use crate::ir::{Block, Callee, Expr, Stmt, StmtKind, block_exprs};
use crate::lower::Lowered;
use crate::symbol;

/// The most calls of counted functions a thread may have in progress (specs/recursion-depth R1.2).
pub const LIMIT: i32 = 1000;

/// The functions of `lowered`, by name, that a call can nest through without bound: each one in a
/// cycle of the call graph, each one used as a value, and each method of a type made a `dyn`
/// value. A call through a value or through `dyn` has no edge in the graph, so every cycle that
/// passes through one meets a function counted for being reachable that way.
pub fn recursive(lowered: &Lowered) -> BTreeSet<String> {
    let names: Vec<&str> = lowered.functions.iter().map(|f| f.name.as_str()).collect();
    let index: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut by_method: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, name) in names.iter().enumerate() {
        if let Some(method) = method_of(name) {
            by_method.entry(method).or_default().push(i);
        }
    }
    let mut counted = BTreeSet::new();
    let mut edges: Vec<Vec<usize>> = vec![Vec::new(); names.len()];
    let bodies = lowered.functions.iter().map(|f| &f.body).chain(lowered.defaults.iter().map(|d| &d.function.body));
    for (from, body) in bodies.enumerate() {
        let mut found = Found::default();
        each_reference(body, &mut found);
        let methods_named = |method: &str| by_method.get(method).into_iter().flatten().copied();
        let mut called: Vec<usize> = found.calls.iter().filter_map(|n| index.get(n.as_str()).copied()).collect();
        called.extend(found.any_method.iter().flat_map(|m| methods_named(m)));
        let mut values: Vec<usize> = found.values.iter().filter_map(|n| index.get(n.as_str()).copied()).collect();
        for (owner, trait_name) in &found.dyns {
            let slots = lowered.dyn_methods.get(trait_name).into_iter().flatten().flatten();
            for method in slots {
                match owner {
                    Some(owner) => values.extend(index.get(symbol::method(owner, method).as_str())),
                    None => values.extend(methods_named(method)),
                }
            }
        }
        for vtable in &found.vtables {
            let slots = lowered.vtables.get(*vtable).into_iter().flat_map(|t| t.slots.iter().flatten());
            values.extend(slots.filter_map(|s| index.get(s.function.as_str()).copied()));
        }
        counted.extend(values.into_iter().map(|i| names[i].to_string()));
        if let Some(edges) = edges.get_mut(from) {
            *edges = called;
        }
    }
    for component in cycles(&edges) {
        counted.extend(component.into_iter().map(|i| names[i].to_string()));
    }
    counted
}

/// Count the calls of each function [`recursive`] marks: its body begins with `Enter`, and it
/// leaves through `Leave` at each `return` and at the end of a body control can run past
/// (specs/recursion-depth R1.3). A panic leaves through neither: where it is caught, the count is
/// put back (R1.5).
pub fn count(lowered: &mut Lowered) {
    let marked = recursive(lowered);
    for f in lowered.functions.iter_mut().filter(|f| marked.contains(&f.name)) {
        let falls_off = crate::verify::reaches_end(f);
        let span = f.span;
        leave_at_returns(&mut f.body);
        let start = Span { start: span.start, end: span.start };
        f.body.insert(0, Stmt { span: start, at: start, kind: StmtKind::Enter });
        if falls_off {
            let end = Span { start: span.end, end: span.end };
            f.body.push(Stmt { span: end, at: end, kind: StmtKind::Leave });
        }
    }
}

fn leave_at_returns(block: &mut Block) {
    let mut i = 0;
    while i < block.len() {
        match &mut block[i].kind {
            StmtKind::Return(_) => {
                let (span, at) = (block[i].span, block[i].at);
                block.insert(i, Stmt { span, at, kind: StmtKind::Leave });
                i += 1;
            }
            StmtKind::If(_, then, otherwise) => {
                leave_at_returns(then);
                leave_at_returns(otherwise);
            }
            StmtKind::Loop(body) => leave_at_returns(body),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                leave_at_returns(body);
                leave_at_returns(exit);
            }
            _ => {}
        }
        i += 1;
    }
}

/// What a body refers to: the functions it calls, by name, the methods it calls on a type
/// parameter, by method, the functions it takes as values, and the types it makes `dyn` values of.
#[derive(Default)]
struct Found {
    calls: Vec<String>,
    any_method: Vec<String>,
    values: Vec<String>,
    /// The type a `dyn` value is made of, `None` when only known once `mono` has run, with the
    /// trait.
    dyns: Vec<(Option<String>, String)>,
    vtables: Vec<usize>,
}

fn each_reference(body: &Block, found: &mut Found) {
    block_exprs(body, &mut |e| match e {
        Expr::Call(name, _) | Expr::CallSlots(name, _) => found.calls.push(name.clone()),
        Expr::CallGeneric { callee: Callee::Function { name, .. }, .. } => found.calls.push(symbol::function(name)),
        Expr::CallGeneric { callee: Callee::Method { owner: Ty::Adt(owner, _), method, .. }, .. } => {
            found.calls.push(symbol::method(owner, method));
        }
        Expr::CallGeneric { callee: Callee::Method { method, .. }, .. } => found.any_method.push(method.clone()),
        Expr::FnRef(name) => found.values.push(name.clone()),
        Expr::FnRefGeneric { name, .. } => found.values.push(symbol::function(name)),
        Expr::Closure { lambda, .. } => found.values.push(symbol::lambda(*lambda)),
        Expr::ToDynOf { ty: Ty::Dyn(trait_name), from, .. } => {
            let owner = if let Ty::Adt(owner, _) = from { Some(owner.clone()) } else { None };
            found.dyns.push((owner, trait_name.clone()));
        }
        Expr::ToDyn { vtable, .. } => found.vtables.push(*vtable),
        _ => {}
    });
}

/// The method a function's symbol names, when it names one: `push` of `lm5_Stack_push`.
fn method_of(name: &str) -> Option<&str> {
    let rest = name.strip_prefix("lm")?;
    let digits = rest.find('_')?;
    let owner: usize = rest[..digits].parse().ok()?;
    rest.get(digits + 1 + owner..)?.strip_prefix('_')
}

/// The strongly connected components of the graph `edges` that hold a cycle: more than one
/// node, or one with an edge to itself. Tarjan's algorithm, walked with a stack of its own, so a
/// long chain of calls cannot exhaust the compiler's.
fn cycles(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNSEEN: usize = usize::MAX;
    let n = edges.len();
    let (mut order, mut low) = (vec![UNSEEN; n], vec![0; n]);
    let mut on_stack = vec![false; n];
    let (mut stack, mut found, mut next) = (Vec::new(), Vec::new(), 0);
    for root in 0..n {
        if order[root] != UNSEEN {
            continue;
        }
        let mut walk: Vec<(usize, usize)> = vec![(root, 0)];
        order[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut edge)) = walk.last_mut() {
            if let Some(&w) = edges[v].get(*edge) {
                *edge += 1;
                if order[w] == UNSEEN {
                    order[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    walk.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(order[w]);
                }
                continue;
            }
            walk.pop();
            if let Some(&(parent, _)) = walk.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == order[v] {
                let mut component = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                if component.len() > 1 || edges[v].contains(&v) {
                    found.push(component);
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_symbol_gives_its_method() {
        assert_eq!(method_of(&symbol::method("Stack", "push")), Some("push"));
        assert_eq!(method_of(&symbol::method("a_b", "c")), Some("c"));
        assert_eq!(method_of(&symbol::method("a", "b_c")), Some("b_c"));
        assert_eq!(method_of(&symbol::function("lm")), None);
        assert_eq!(method_of(&symbol::lambda(3)), None);
    }

    #[test]
    fn a_cycle_is_a_component_of_two_or_more_or_a_node_calling_itself() {
        let edges = vec![vec![1], vec![2], vec![0], vec![3], vec![0], vec![]];
        let mut found: Vec<Vec<usize>> = cycles(&edges)
            .into_iter()
            .map(|mut c| {
                c.sort_unstable();
                c
            })
            .collect();
        found.sort();
        assert_eq!(found, vec![vec![0, 1, 2], vec![3]]);
    }

    #[test]
    fn a_long_chain_of_calls_is_walked_without_recursion() {
        let n = 200_000;
        let mut edges: Vec<Vec<usize>> = (0..n).map(|i| vec![i + 1]).collect();
        edges[n - 1] = vec![0];
        assert_eq!(cycles(&edges).iter().map(Vec::len).collect::<Vec<_>>(), vec![n]);
    }
}
