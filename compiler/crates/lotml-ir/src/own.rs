//! Counting (adr:0016; specs/c-backend/design.md, "Counting"): the counts inserted into a
//! function so that, at every point, the counted locals holding a count are exactly the live
//! ones. A use that stores a value moves it at its last use and increments it before; a use
//! that only reads it lets it be decremented right after, when it was the last. A branch
//! decrements on entry what it never uses; a spent loop, on exit, what is not used after it.

use lotml_syntax::span::Span;

use crate::ir::{
    Arg, Block, Builtin, Expr, Function, Local, LocalInfo, Operand, Place, Proj, Stmt, StmtKind, counted, outs,
};

/// A set of locals, as bits.
#[derive(Clone, PartialEq, Eq)]
struct Set(Vec<u64>);

impl Set {
    fn new(size: usize) -> Set {
        Set(vec![0; size.div_ceil(64)])
    }

    fn contains(&self, l: Local) -> bool {
        self.0.get(l / 64).is_some_and(|w| w & (1 << (l % 64)) != 0)
    }

    fn insert(&mut self, l: Local) {
        if l / 64 >= self.0.len() {
            self.0.resize(l / 64 + 1, 0);
        }
        self.0[l / 64] |= 1 << (l % 64);
    }

    fn remove(&mut self, l: Local) {
        if let Some(w) = self.0.get_mut(l / 64) {
            *w &= !(1 << (l % 64));
        }
    }

    fn union(&mut self, other: &Set) {
        if other.0.len() > self.0.len() {
            self.0.resize(other.0.len(), 0);
        }
        for (w, o) in self.0.iter_mut().zip(&other.0) {
            *w |= o;
        }
    }

    /// The locals of `self` not in `other`, in order.
    fn minus(&self, other: &Set) -> Vec<Local> {
        let mut out = Vec::new();
        for (i, w) in self.0.iter().enumerate() {
            let o = other.0.get(i).copied().unwrap_or(0);
            let mut bits = w & !o;
            while bits != 0 {
                let b = bits.trailing_zeros() as usize;
                out.push(i * 64 + b);
                bits &= bits - 1;
            }
        }
        out
    }
}

pub fn insert_counts(f: &mut Function) {
    let mut pass = Pass { locals: std::mem::take(&mut f.locals) };
    let empty = Set::new(pass.locals.len());
    let body = std::mem::take(&mut f.body);
    let live_in = pass.live_block(&body, &empty, &empty, &empty);
    let mut out = Vec::new();
    for &p in &f.params {
        if pass.counted(p) && !live_in.contains(p) {
            out.push(Stmt { span: f.span, kind: StmtKind::Dec(p) });
        }
    }
    out.extend(pass.block(body, &empty, &empty, &empty));
    f.body = out;
    f.locals = pass.locals;
}

struct Pass {
    locals: Vec<LocalInfo>,
}

/// How a statement uses each counted local: the uses that store it and those that only read it,
/// and the `inout` parameters it stores, which always need a count of their own.
#[derive(Default)]
struct Uses {
    stored: Vec<Local>,
    read: Vec<Local>,
    by_ref_stored: Vec<Local>,
}

impl Pass {
    /// Whether the local holds a count of its own: an `inout` parameter does not.
    fn counted(&self, l: Local) -> bool {
        counted(&self.locals[l].ty) && !self.locals[l].by_ref
    }

    /// Whether the local is an `inout` parameter of a counted type.
    fn by_ref(&self, l: Local) -> bool {
        counted(&self.locals[l].ty) && self.locals[l].by_ref
    }

    fn add(&self, set: &mut Set, operand: &Operand) {
        if let Operand::Local(l) = operand
            && self.counted(*l)
        {
            set.insert(*l);
        }
    }

    fn add_expr(&self, set: &mut Set, e: &Expr) {
        e.operands(&mut |o| self.add(set, o));
    }

    fn add_args(&self, set: &mut Set, args: &[Arg]) {
        for a in args {
            if let Some(o) = a.operand() {
                self.add(set, o);
            }
        }
    }

    // Liveness ----------------------------------------------------------------------------

    fn live_block(&self, block: &[Stmt], out: &Set, brk: &Set, cont: &Set) -> Set {
        let mut live = out.clone();
        for stmt in block.iter().rev() {
            live = self.live_stmt(stmt, &live, brk, cont);
        }
        live
    }

    fn live_stmt(&self, stmt: &Stmt, out: &Set, brk: &Set, cont: &Set) -> Set {
        match &stmt.kind {
            StmtKind::Let(x, e) => {
                let mut s = out.clone();
                s.remove(*x);
                for o in e.outs() {
                    s.remove(o);
                }
                self.add_expr(&mut s, e);
                s
            }
            StmtKind::Do(e) => {
                let mut s = out.clone();
                for o in e.outs() {
                    s.remove(o);
                }
                self.add_expr(&mut s, e);
                s
            }
            StmtKind::Store(place, v) => {
                let mut s = out.clone();
                self.add(&mut s, &Operand::Local(place.local));
                crate::ir::place_operands(place, &mut |o| self.add(&mut s, o));
                self.add(&mut s, v);
                s
            }
            StmtKind::Mutate { place, args, result, .. } => {
                let mut s = out.clone();
                for o in outs(args).chain(*result) {
                    s.remove(o);
                }
                self.add(&mut s, &Operand::Local(place.local));
                crate::ir::place_operands(place, &mut |o| self.add(&mut s, o));
                self.add_args(&mut s, args);
                s
            }
            StmtKind::If(_, then, otherwise) => {
                let mut s = self.live_block(then, out, brk, cont);
                s.union(&self.live_block(otherwise, out, brk, cont));
                s
            }
            StmtKind::Loop(body) => self.loop_head(body, out, None, None),
            StmtKind::ForRange { var, body, .. } => self.loop_head(body, out, Some(*var), None),
            StmtKind::ForStr { var, over, body, .. } => self.loop_head(body, out, Some(*var), Some(over)),
            StmtKind::Break => brk.clone(),
            StmtKind::Continue => cont.clone(),
            StmtKind::Return(value) => {
                let mut s = Set::new(self.locals.len());
                if let Some(v) = value {
                    self.add(&mut s, v);
                }
                s
            }
            StmtKind::Panic(_) => Set::new(self.locals.len()),
            StmtKind::Inc(_) | StmtKind::Dec(_) | StmtKind::DropReuse { .. } => out.clone(),
        }
    }

    /// What is live at the head of a loop: for a counted loop, also what is live after it, since
    /// it may end there; for a string, the string walked.
    fn loop_head(&self, body: &[Stmt], out: &Set, var: Option<Local>, over: Option<&Operand>) -> Set {
        let counted_loop = var.is_some();
        let mut head = if counted_loop { out.clone() } else { Set::new(self.locals.len()) };
        loop {
            let mut next = self.live_block(body, &head, out, &head);
            if let Some(v) = var {
                next.remove(v);
            }
            if counted_loop {
                next.union(out);
            }
            if let Some(o) = over {
                self.add(&mut next, o);
            }
            if next == head {
                return head;
            }
            head = next;
        }
    }

    // Counting ----------------------------------------------------------------------------

    fn block(&mut self, block: Block, out: &Set, brk: &Set, cont: &Set) -> Block {
        let mut after = vec![out.clone(); block.len()];
        let mut live = out.clone();
        for (i, stmt) in block.iter().enumerate().rev() {
            after[i] = live.clone();
            live = self.live_stmt(stmt, &live, brk, cont);
        }
        let mut result = Vec::new();
        for (stmt, live_out) in block.into_iter().zip(after) {
            self.stmt(stmt, &live_out, brk, cont, &mut result);
        }
        result
    }

    fn uses_of(&self, e: &Expr) -> Uses {
        let mut uses = Uses::default();
        if let Expr::ReadPlace(place) = e {
            self.place_uses(place, &mut uses);
            return uses;
        }
        let stores = matches!(
            e,
            Expr::Use(_)
                | Expr::Call(..)
                | Expr::TupleNew { .. }
                | Expr::ListNew { .. }
                | Expr::Construct { .. }
                | Expr::OptNew { .. }
                | Expr::OptIf { .. }
                | Expr::ResultNew { .. }
                | Expr::DictNew { .. }
                | Expr::SetNew { .. }
                | Expr::Closure { .. }
                | Expr::CallClosure { .. }
                | Expr::ToDyn { .. }
                | Expr::CallDyn { .. }
        );
        if let Expr::CallSlots(_, args) = e {
            for a in args {
                match a {
                    Arg::Slot(place) => self.place_uses(place, &mut uses),
                    Arg::Value(o) => self.local_uses(&[o], true, &mut uses),
                    _ => {}
                }
            }
            return uses;
        }
        e.operands(&mut |o| {
            if let Operand::Local(l) = o {
                if self.counted(*l) {
                    if stores { uses.stored.push(*l) } else { uses.read.push(*l) }
                } else if stores && self.by_ref(*l) {
                    uses.by_ref_stored.push(*l);
                }
            }
        });
        uses
    }

    /// How a place uses its locals: the local it starts from and its indices are read, the key
    /// and default of a `setdefault` taken over.
    fn place_uses(&self, place: &Place, uses: &mut Uses) {
        self.local_uses(&[&Operand::Local(place.local)], false, uses);
        for proj in &place.proj {
            match proj {
                Proj::Index(i) | Proj::OwnedIndex(i) | Proj::Key(i) => self.local_uses(&[i], false, uses),
                Proj::SetDefault(k, d) => self.local_uses(&[k, d], true, uses),
                Proj::Field(_) => {}
            }
        }
    }

    fn local_uses(&self, operands: &[&Operand], stored: bool, uses: &mut Uses) {
        for o in operands {
            if let Operand::Local(l) = o {
                if self.counted(*l) {
                    if stored { uses.stored.push(*l) } else { uses.read.push(*l) }
                } else if stored && self.by_ref(*l) {
                    uses.by_ref_stored.push(*l);
                }
            }
        }
    }

    fn stmt(&mut self, stmt: Stmt, live_out: &Set, brk: &Set, cont: &Set, out: &mut Vec<Stmt>) {
        let span = stmt.span;
        let at = |kind| Stmt { span, kind };
        match stmt.kind {
            StmtKind::Let(x, e) => {
                let set = e.outs();
                let uses = self.uses_of(&e);
                let borrowed = matches!(
                    e,
                    Expr::ListGet { .. }
                        | Expr::ReadPlace(_)
                        | Expr::TupleGet { .. }
                        | Expr::RtValue { .. }
                        | Expr::ToStr(_, lotml_check::ty::Ty::Str)
                        | Expr::Field { .. }
                        | Expr::OptValue(_)
                        | Expr::ResultValue(_)
                        | Expr::ResultError(_)
                        | Expr::Capture { .. }
                );
                self.simple(span, uses, live_out, Some((x, borrowed)), StmtKind::Let(x, e), out);
                self.drop_unused(span, &set, live_out, out);
            }
            StmtKind::Do(e) => {
                let set = e.outs();
                let uses = self.uses_of(&e);
                self.simple(span, uses, live_out, None, StmtKind::Do(e), out);
                self.drop_unused(span, &set, live_out, out);
            }
            StmtKind::Store(place, v) => {
                let mut uses = Uses::default();
                self.local_uses(&[&v], true, &mut uses);
                self.place_uses(&place, &mut uses);
                self.simple(span, uses, live_out, None, StmtKind::Store(place, v), out);
            }
            StmtKind::Mutate { op, place, args, at: here, result } => {
                let stores = matches!(
                    op,
                    Builtin::ListPush | Builtin::ListInsert | Builtin::DictSet | Builtin::SetAdd | Builtin::HeapPush
                );
                let mut uses = Uses::default();
                for a in &args {
                    match a {
                        Arg::Address(o, _) => self.local_uses(&[o], stores, &mut uses),
                        Arg::Value(o) => self.local_uses(&[o], false, &mut uses),
                        Arg::Slot(p) => self.place_uses(p, &mut uses),
                        Arg::Out(..) | Arg::Desc(_) | Arg::Offset(_) => {}
                    }
                }
                self.place_uses(&place, &mut uses);
                let set: Vec<Local> = outs(&args).collect();
                let kind = StmtKind::Mutate { op, place, args, at: here, result };
                self.simple(span, uses, live_out, None, kind, out);
                self.drop_unused(span, &set, live_out, out);
            }
            StmtKind::Return(Some(v)) => {
                let mut uses = Uses::default();
                self.local_uses(&[&v], true, &mut uses);
                self.simple(span, uses, live_out, None, StmtKind::Return(Some(v)), out);
            }
            StmtKind::If(test, then, otherwise) => {
                let live_then = self.live_block(&then, live_out, brk, cont);
                let live_else = self.live_block(&otherwise, live_out, brk, cont);
                let mut live_in = live_then.clone();
                live_in.union(&live_else);
                let mut new_then = self.decs(span, live_in.minus(&live_then));
                new_then.extend(self.block(then, live_out, brk, cont));
                let mut new_else = self.decs(span, live_in.minus(&live_else));
                new_else.extend(self.block(otherwise, live_out, brk, cont));
                out.push(at(StmtKind::If(test, new_then, new_else)));
            }
            StmtKind::Loop(body) => {
                let head = self.loop_head(&body, live_out, None, None);
                let body = self.block(body, &head, live_out, &head);
                out.push(at(StmtKind::Loop(body)));
            }
            StmtKind::ForRange { var, start, stop, step, body, .. } => {
                let head = self.loop_head(&body, live_out, Some(var), None);
                let (body, exit) = self.loop_body(span, body, &head, live_out, var);
                out.push(at(StmtKind::ForRange { var, start, stop, step, body, exit }));
            }
            StmtKind::ForStr { var, over, body, .. } => {
                let mut head = self.loop_head(&body, live_out, Some(var), Some(&over));
                if let Operand::Local(o) = over
                    && self.counted(o)
                {
                    head.insert(o);
                }
                let (body, exit) = self.loop_body(span, body, &head, live_out, var);
                out.push(at(StmtKind::ForStr { var, over, body, exit }));
            }
            other => out.push(at(other)),
        }
    }

    /// The body of a counted loop, with what it does not use dropped on entry, and the block run
    /// when the loop is spent, dropping what is not used after it.
    fn loop_body(&mut self, span: Span, body: Block, head: &Set, live_out: &Set, var: Local) -> (Block, Block) {
        let live_body = self.live_block(&body, head, live_out, head);
        let mut entry = head.clone();
        if self.counted(var) {
            entry.insert(var);
        }
        let mut new_body = self.decs(span, entry.minus(&live_body));
        new_body.extend(self.block(body, head, live_out, head));
        let exit = self.decs(span, head.minus(live_out));
        (new_body, exit)
    }

    /// Drop the counted locals a statement set through its outputs that nothing uses after it.
    fn drop_unused(&self, span: Span, set: &[Local], live_out: &Set, out: &mut Vec<Stmt>) {
        for &l in set {
            if self.counted(l) && !live_out.contains(l) {
                out.push(Stmt { span, kind: StmtKind::Dec(l) });
            }
        }
    }

    fn decs(&self, span: Span, locals: Vec<Local>) -> Vec<Stmt> {
        locals.into_iter().map(|l| Stmt { span, kind: StmtKind::Dec(l) }).collect()
    }

    /// A statement with no blocks: increments before it for what it stores and still needs,
    /// decrements after it for what it read last; `defined` is the local it sets, and whether the
    /// value it sets is borrowed and needs its own count.
    fn simple(
        &mut self,
        span: Span,
        uses: Uses,
        live_out: &Set,
        defined: Option<(Local, bool)>,
        kind: StmtKind,
        out: &mut Vec<Stmt>,
    ) {
        let at = |kind| Stmt { span, kind };
        let target = defined.map(|(x, _)| x);
        for &l in &uses.by_ref_stored {
            out.push(at(StmtKind::Inc(l)));
        }
        let mut seen: Vec<Local> = uses.stored.iter().chain(&uses.read).copied().collect();
        seen.sort_unstable();
        seen.dedup();
        let mut dying = Vec::new();
        for l in seen {
            let stored = uses.stored.iter().filter(|&&s| s == l).count();
            let read = uses.read.iter().filter(|&&r| r == l).count();
            let alive = live_out.contains(l) && target != Some(l);
            // A local the statement both stores and reads, `f(xs, &xs)`, cannot give its own count
            // away: the callee may drop the stored one while it still reads the cell through the
            // other. It keeps it, and drops it after the statement.
            let incs = if alive || read > 0 { stored } else { stored.saturating_sub(1) };
            for _ in 0..incs {
                out.push(at(StmtKind::Inc(l)));
            }
            if !alive && read > 0 {
                dying.push(l);
            }
        }
        match defined {
            Some((x, borrowed)) if self.by_ref(x) => {
                // An `inout` parameter assigned: the new value replaces the caller's, which is
                // dropped once the new one is computed.
                let StmtKind::Let(_, e) = kind else { unreachable!("a definition is a Let") };
                let ty = self.locals[x].ty.clone();
                self.locals.push(LocalInfo { ty, name: None, by_ref: false });
                let t = self.locals.len() - 1;
                out.push(at(StmtKind::Let(t, e)));
                if borrowed {
                    out.push(at(StmtKind::Inc(t)));
                }
                for l in dying {
                    out.push(at(StmtKind::Dec(l)));
                }
                out.push(at(StmtKind::Dec(x)));
                out.push(at(StmtKind::Let(x, Expr::Use(Operand::Local(t)))));
            }
            Some((x, borrowed)) if self.counted(x) && dying.contains(&x) => {
                // `x = f(x)` where f only reads x: the old value is dropped once the new one is
                // computed, and the new one moved into x.
                let StmtKind::Let(_, e) = kind else { unreachable!("a definition is a Let") };
                let ty = self.locals[x].ty.clone();
                self.locals.push(LocalInfo { ty, name: None, by_ref: false });
                let t = self.locals.len() - 1;
                out.push(at(StmtKind::Let(t, e)));
                if borrowed {
                    out.push(at(StmtKind::Inc(t)));
                }
                for l in dying {
                    out.push(at(StmtKind::Dec(l)));
                }
                out.push(at(StmtKind::Let(x, Expr::Use(Operand::Local(t)))));
                if !live_out.contains(x) {
                    out.push(at(StmtKind::Dec(x)));
                }
            }
            _ => {
                out.push(at(kind));
                if let Some((x, borrowed)) = defined
                    && borrowed
                    && self.counted(x)
                {
                    out.push(at(StmtKind::Inc(x)));
                }
                for l in dying {
                    out.push(at(StmtKind::Dec(l)));
                }
                if let Some((x, _)) = defined
                    && self.counted(x)
                    && !live_out.contains(x)
                {
                    out.push(at(StmtKind::Dec(x)));
                }
            }
        }
    }
}
