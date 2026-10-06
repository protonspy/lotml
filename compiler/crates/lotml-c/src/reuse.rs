//! Reuse (adr:0008; specs/c-backend/design.md, "Counting"): after the counts, a decrement of a
//! record or variant followed in the same block by a constructor becomes a drop-reuse — when the
//! value was unique, its fields are dropped and its memory kept as a token — and the constructor
//! builds in the token's memory when it is large enough (R3.4).

use lotml_check::ty::Ty;

use crate::mir::{Block, Expr, Function, StmtKind};

pub fn insert_reuse(f: &mut Function) {
    let mut tokens = 0;
    let body = std::mem::take(&mut f.body);
    f.body = block(body, f, &mut tokens);
}

fn block(mut stmts: Block, f: &Function, tokens: &mut usize) -> Block {
    for stmt in &mut stmts {
        match &mut stmt.kind {
            StmtKind::If(_, then, otherwise) => {
                *then = block(std::mem::take(then), f, tokens);
                *otherwise = block(std::mem::take(otherwise), f, tokens);
            }
            StmtKind::Loop(body) => *body = block(std::mem::take(body), f, tokens),
            StmtKind::ForRange { body, exit, .. } | StmtKind::ForStr { body, exit, .. } => {
                *body = block(std::mem::take(body), f, tokens);
                *exit = block(std::mem::take(exit), f, tokens);
            }
            _ => {}
        }
    }
    let mut i = 0;
    while i < stmts.len() {
        if let StmtKind::Dec(local) = stmts[i].kind
            && matches!(f.locals[local].ty, Ty::Adt(..))
            && let Some(at) = constructor_after(&stmts[i + 1..])
        {
            let token = *tokens;
            *tokens += 1;
            stmts[i].kind = StmtKind::DropReuse { local, token };
            if let StmtKind::Let(_, Expr::Construct { reuse, .. }) = &mut stmts[i + 1 + at].kind {
                *reuse = Some(token);
            }
        }
        i += 1;
    }
    stmts
}

/// The index of the first constructor in `rest` with no reuse yet, reached on every path: only
/// straight-line statements before it.
fn constructor_after(rest: &[crate::mir::Stmt]) -> Option<usize> {
    for (i, stmt) in rest.iter().enumerate() {
        match &stmt.kind {
            StmtKind::Let(_, Expr::Construct { reuse: None, .. }) => return Some(i),
            StmtKind::Let(..)
            | StmtKind::Do(_)
            | StmtKind::Store(..)
            | StmtKind::Mutate { .. }
            | StmtKind::Inc(_)
            | StmtKind::Dec(_)
            | StmtKind::DropReuse { .. } => {}
            _ => return None,
        }
    }
    None
}
