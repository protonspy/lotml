//! The IR's verifier (specs/shared-ir R1.6), run after lowering and after each native pass in a
//! build with debug assertions: every local is set before it is read, and an operand has the type
//! its use requires where the statement says what that is. The emitters rely on both instead of
//! each checking them again.

use lotml_check::ty::{FloatKind, Ty};

use crate::ir::{Arg, BinOp, Block, Const, Expr, Function, Local, Operand, Stmt, StmtKind, place_operands};

/// What is wrong with `f` after the pass `pass`, one line per finding; empty when nothing is.
pub fn verify(f: &Function, pass: &str) -> Vec<String> {
    walk(f, pass).1
}

/// Whether control can run past the last statement of `f`'s body: a function returning `()`
/// that ends without a `return`.
pub fn reaches_end(f: &Function) -> bool {
    walk(f, "").0
}

fn walk(f: &Function, pass: &str) -> (bool, Vec<String>) {
    let mut check = Check { f, pass, breaks: Vec::new(), findings: Vec::new() };
    let mut set = vec![false; f.locals.len()];
    for &p in &f.params {
        set[p] = true;
    }
    let end = check.block(&f.body, set);
    (end.is_some(), check.findings)
}

/// Panics with every finding of `verify`, in a build with debug assertions.
pub fn assert_valid(f: &Function, pass: &str) {
    if cfg!(debug_assertions) {
        let findings = verify(f, pass);
        assert!(findings.is_empty(), "the IR is not valid:\n{}", findings.join("\n"));
    }
}

/// Which locals are set at a point: `None` where control never reaches.
type Set = Option<Vec<bool>>;

struct Check<'f> {
    f: &'f Function,
    pass: &'f str,
    /// For each loop being walked, what is set at each of its `break`s.
    breaks: Vec<Vec<Vec<bool>>>,
    findings: Vec<String>,
}

fn join(a: Set, b: Set) -> Set {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.iter().zip(&b).map(|(x, y)| *x && *y).collect()),
        (one, None) | (None, one) => one,
    }
}

impl Check<'_> {
    fn report(&mut self, stmt: &Stmt, what: String) {
        self.findings.push(format!(
            "after {}: in {} at bytes {}..{}: {what}",
            self.pass, self.f.name, stmt.span.start, stmt.span.end
        ));
    }

    fn read(&mut self, stmt: &Stmt, set: &[bool], o: &Operand) {
        if let Operand::Local(l) = o {
            self.read_local(stmt, set, *l);
        }
    }

    fn read_local(&mut self, stmt: &Stmt, set: &[bool], l: Local) {
        if !set.get(l).copied().unwrap_or(false) {
            self.report(stmt, format!("%{l} is read before it is set"));
        }
    }

    fn ty(&self, o: &Operand) -> Option<Ty> {
        match o {
            Operand::Local(l) => self.f.locals.get(*l).map(|i| i.ty.clone()),
            Operand::Const(Const::Int(_, k)) => Some(Ty::Int(*k)),
            Operand::Const(Const::Float(_)) => Some(Ty::Float(FloatKind::F64)),
            Operand::Const(Const::Bool(_)) => Some(Ty::Bool),
            Operand::Const(Const::Unit) => Some(Ty::Unit),
            Operand::Const(Const::Str(_)) => Some(Ty::Str),
            Operand::Const(Const::Bytes(_)) => Some(Ty::Bytes),
            Operand::Const(Const::Null | Const::Char(_)) => None,
        }
    }

    /// Whether `o` may be used where a `want` is required.
    fn expect(&mut self, stmt: &Stmt, o: &Operand, want: &Ty, role: &str) {
        let Some(found) = self.ty(o) else { return };
        if !fits(&found, want) {
            self.report(stmt, format!("{role} is {found} where {want} is required"));
        }
    }

    fn block(&mut self, block: &Block, mut set: Vec<bool>) -> Set {
        for stmt in block {
            set = self.stmt(stmt, set)?;
        }
        Some(set)
    }

    fn stmt(&mut self, stmt: &Stmt, mut set: Vec<bool>) -> Set {
        match &stmt.kind {
            StmtKind::Let(l, e) => {
                self.expr(stmt, &set, e);
                self.typed_let(stmt, *l, e);
                for out in e.outs() {
                    set[out] = true;
                }
                set[*l] = true;
            }
            StmtKind::Store(place, v) => {
                self.read_local(stmt, &set, place.local);
                place_operands(place, &mut |o| self.read(stmt, &set, o));
                self.read(stmt, &set, v);
            }
            StmtKind::Mutate { place, args, result, .. } => {
                self.read_local(stmt, &set, place.local);
                place_operands(place, &mut |o| self.read(stmt, &set, o));
                self.args(stmt, &set, args);
                for out in crate::ir::outs(args) {
                    set[out] = true;
                }
                if let Some(r) = result {
                    set[*r] = true;
                }
            }
            StmtKind::Do(e) => {
                self.expr(stmt, &set, e);
                for out in e.outs() {
                    set[out] = true;
                }
            }
            StmtKind::If(test, then, otherwise) => {
                self.read(stmt, &set, test);
                self.expect(stmt, test, &Ty::Bool, "the condition");
                let a = self.block(then, set.clone());
                let b = self.block(otherwise, set);
                return join(a, b);
            }
            StmtKind::Loop(body) => {
                self.breaks.push(Vec::new());
                self.block(body, set);
                let exits = self.breaks.pop().unwrap_or_default();
                return exits.into_iter().map(Some).reduce(join).flatten();
            }
            StmtKind::ForRange { var, start, stop, step, body, exit } => {
                for o in [start, stop, step] {
                    self.read(stmt, &set, o);
                }
                return self.walk(*var, body, exit, set);
            }
            StmtKind::ForStr { var, over, body, exit } => {
                self.read(stmt, &set, over);
                return self.walk(*var, body, exit, set);
            }
            StmtKind::Break => {
                if let Some(exits) = self.breaks.last_mut() {
                    exits.push(set);
                }
                return None;
            }
            StmtKind::Continue | StmtKind::Panic(_) => return None,
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    self.read(stmt, &set, v);
                    let ret = self.f.ret.clone();
                    self.expect(stmt, v, &ret, "the value returned");
                }
                return None;
            }
            StmtKind::Inc(l) | StmtKind::Dec(l) | StmtKind::DropReuse { local: l, .. } => {
                self.read_local(stmt, &set, *l)
            }
            StmtKind::Enter | StmtKind::Leave => {}
        }
        Some(set)
    }

    /// A loop over values of `var`: its body sees `var` set, its `exit` runs once they are spent,
    /// and what follows is what both the exit and every `break` leave set.
    fn walk(&mut self, var: Local, body: &Block, exit: &Block, set: Vec<bool>) -> Set {
        let mut inside = set.clone();
        inside[var] = true;
        self.breaks.push(Vec::new());
        self.block(body, inside);
        let exits = self.breaks.pop().unwrap_or_default();
        let after = self.block(exit, set);
        exits.into_iter().map(Some).fold(after, join)
    }

    fn args(&mut self, stmt: &Stmt, set: &[bool], args: &[Arg]) {
        for a in args {
            match a {
                Arg::Slot(place) => {
                    self.read_local(stmt, set, place.local);
                    place_operands(place, &mut |o| self.read(stmt, set, o));
                }
                other => {
                    if let Some(o) = other.operand() {
                        self.read(stmt, set, o);
                    }
                }
            }
        }
    }

    fn expr(&mut self, stmt: &Stmt, set: &[bool], e: &Expr) {
        match e {
            Expr::Rt { args, .. }
            | Expr::RtValue { args, .. }
            | Expr::CallSlots(_, args)
            | Expr::CallGeneric { args, .. } => self.args(stmt, set, args),
            other => other.operands(&mut |o| self.read(stmt, set, o)),
        }
        match e {
            Expr::Binary(op, a, b, ty) => {
                self.expect(stmt, a, ty, "the left operand");
                if !matches!(op, BinOp::Shl | BinOp::Shr | BinOp::Pow) {
                    self.expect(stmt, b, ty, "the right operand");
                }
            }
            Expr::Compare(_, a, b, ty) => {
                self.expect(stmt, a, ty, "the left operand");
                self.expect(stmt, b, ty, "the right operand");
            }
            Expr::Unary(_, a, ty) => self.expect(stmt, a, ty, "the operand"),
            _ => {}
        }
    }

    /// The local a `let` sets has the type of the value it is given, where the expression says.
    fn typed_let(&mut self, stmt: &Stmt, l: Local, e: &Expr) {
        let local = self.f.locals[l].ty.clone();
        let given = match e {
            Expr::Use(o) => self.ty(o),
            Expr::Compare(..) | Expr::Contains { .. } | Expr::OptIsSome(_) | Expr::ResultIsOk(_) => Some(Ty::Bool),
            Expr::Binary(BinOp::TrueDiv, _, _, Ty::Int(_)) => Some(Ty::Float(FloatKind::F64)),
            Expr::Binary(op, _, _, ty) if !matches!(op, BinOp::Pow) => Some(ty.clone()),
            Expr::Convert(_, _, to) => Some(to.clone()),
            _ => None,
        };
        if let Some(given) = given
            && !fits(&given, &local)
        {
            self.report(stmt, format!("%{l}, of type {local}, is given a value of type {given}"));
        }
    }
}

/// Whether a value of `found` fits where `want` is required: the same type, floats of either
/// width (a float constant is written once for both), or alike but where a mistake, `todo()` or
/// an inference the checker left open stands, at any depth, or a heap where the list it is kept in
/// is.
fn fits(found: &Ty, want: &Ty) -> bool {
    matches!((found, want), (Ty::Float(_), Ty::Float(_))) || alike(found, want)
}

fn alike(a: &Ty, b: &Ty) -> bool {
    let open = |t: &Ty| matches!(t, Ty::Error | Ty::Never | Ty::Var(_) | Ty::Param(_));
    let all = |xs: &[Ty], ys: &[Ty]| xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| alike(x, y));
    if open(a) || open(b) {
        return true;
    }
    match (a, b) {
        (Ty::List(x) | Ty::Heap(x), Ty::List(y) | Ty::Heap(y))
        | (Ty::Set(x), Ty::Set(y))
        | (Ty::Optional(x), Ty::Optional(y)) => alike(x, y),
        (Ty::Dict(k, v), Ty::Dict(l, w)) | (Ty::Result(k, v), Ty::Result(l, w)) => alike(k, l) && alike(v, w),
        (Ty::Tuple(xs), Ty::Tuple(ys)) => all(xs, ys),
        (Ty::Adt(n, xs), Ty::Adt(m, ys)) => n == m && all(xs, ys),
        (Ty::Func(ps, r), Ty::Func(qs, s)) => all(ps, qs) && alike(r, s),
        _ => a == b,
    }
}
