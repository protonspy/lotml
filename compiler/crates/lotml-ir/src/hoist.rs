//! Uniqueness out of loops: a list a loop stores into by index, and otherwise only reads, changes
//! in place or measures, is made its slot's own once before the loop, and each store in the loop
//! skips the check. Nothing in such a loop can share the list, so it stays unique (R3.5): the check
//! the copy-on-write needs runs once instead of once per element.

use std::collections::BTreeSet;

use lotml_check::ty::Ty;

use crate::ir::{Arg, Block, Builtin, Expr, Function, Local, Operand, Place, Proj, Stmt, StmtKind, place_operands};

pub fn hoist_uniqueness(f: &mut Function) {
    let body = std::mem::take(&mut f.body);
    f.body = block(body, f);
}

fn block(stmts: Block, f: &Function) -> Block {
    let mut out = Vec::with_capacity(stmts.len());
    for mut stmt in stmts {
        match &mut stmt.kind {
            StmtKind::If(_, then, otherwise) => {
                *then = block(std::mem::take(then), f);
                *otherwise = block(std::mem::take(otherwise), f);
            }
            StmtKind::Loop(body) | StmtKind::ForRange { body, .. } | StmtKind::ForStr { body, .. } => {
                *body = block(std::mem::take(body), f);
                for local in candidates(body, f) {
                    own(body, local);
                    out.push(Stmt {
                        span: stmt.span,
                        at: stmt.span,
                        kind: StmtKind::Mutate {
                            op: Builtin::ListUnique,
                            place: Place { local, proj: Vec::new() },
                            args: Vec::new(),
                            at: false,
                            result: None,
                        },
                    });
                }
            }
            _ => {}
        }
        out.push(stmt);
    }
    out
}

/// The lists a loop body stores into by index and uses in no way that could share them.
fn candidates(body: &Block, f: &Function) -> BTreeSet<Local> {
    let mut stored = BTreeSet::new();
    stores(body, &mut stored);
    stored
        .into_iter()
        .filter(|&l| matches!(f.locals[l].ty, Ty::List(_)) && !f.locals[l].by_ref && kept(body, l))
        .collect()
}

fn stores(body: &Block, out: &mut BTreeSet<Local>) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Store(place, _) if matches!(place.proj.first(), Some(Proj::Index(_))) => {
                out.insert(place.local);
            }
            StmtKind::If(_, then, otherwise) => {
                stores(then, out);
                stores(otherwise, out);
            }
            StmtKind::Loop(b) | StmtKind::ForRange { body: b, .. } | StmtKind::ForStr { body: b, .. } => stores(b, out),
            _ => {}
        }
    }
}

/// Whether every use of `l` in `body` keeps it the loop's own: an element stored or read, its
/// length, or a change in place through its own slot; never a count, a move, a copy, a new value
/// or a slot handed to a callee.
fn kept(body: &Block, l: Local) -> bool {
    let is = |o: &Operand| matches!(o, Operand::Local(x) if *x == l);
    let free = |p: &Place| {
        let mut found = false;
        place_operands(p, &mut |o| found |= is(o));
        !found
    };
    let indexed = |p: &Place| p.local != l || matches!(p.proj.first(), Some(Proj::Index(_)));
    let expr = |e: &Expr| match e {
        Expr::ListGet { index, .. } => !is(index),
        Expr::Len(..) => true,
        Expr::ReadPlace(p) => free(p) && indexed(p),
        other => {
            let mut found = false;
            other.operands(&mut |o| found |= is(o));
            !found
        }
    };
    body.iter().all(|stmt| match &stmt.kind {
        StmtKind::Let(x, e) => *x != l && expr(e),
        StmtKind::Do(e) => expr(e),
        StmtKind::Store(place, value) => !is(value) && free(place) && indexed(place),
        StmtKind::Mutate { place, args, result, .. } => {
            free(place)
                && *result != Some(l)
                && args.iter().all(|a| match a {
                    Arg::Value(o) | Arg::Address(o, _) => !is(o),
                    Arg::Out(x, _) => *x != l,
                    Arg::Slot(p) => p.local != l && free(p),
                    Arg::Desc(_) | Arg::Offset(_) => true,
                })
        }
        StmtKind::If(test, then, otherwise) => !is(test) && kept(then, l) && kept(otherwise, l),
        StmtKind::Loop(b) => kept(b, l),
        StmtKind::ForRange { var, start, stop, step, body, exit } => {
            *var != l && !is(start) && !is(stop) && !is(step) && kept(body, l) && kept(exit, l)
        }
        StmtKind::ForStr { var, over, body, exit } => *var != l && !is(over) && kept(body, l) && kept(exit, l),
        StmtKind::Return(Some(v)) => !is(v),
        StmtKind::Inc(x) | StmtKind::Dec(x) | StmtKind::DropReuse { local: x, .. } => *x != l,
        StmtKind::Break | StmtKind::Continue | StmtKind::Return(None) | StmtKind::Panic(_) => true,
    })
}

/// The stores into `l` by index in `body`, marked as stores into a list known to be unique.
fn own(body: &mut Block, l: Local) {
    for stmt in body {
        match &mut stmt.kind {
            StmtKind::Store(place, _) if place.local == l => {
                if let Some(Proj::Index(i)) = place.proj.first().cloned() {
                    place.proj[0] = Proj::OwnedIndex(i);
                }
            }
            StmtKind::If(_, then, otherwise) => {
                own(then, l);
                own(otherwise, l);
            }
            StmtKind::Loop(b) | StmtKind::ForRange { body: b, .. } | StmtKind::ForStr { body: b, .. } => own(b, l),
            _ => {}
        }
    }
}
