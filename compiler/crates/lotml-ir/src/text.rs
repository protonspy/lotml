//! The IR as text (specs/shared-ir R1.5): one statement per line, indented by the block it is in,
//! each ending with the place it was lowered from as `@line:column`. It is the form the IR's tests
//! compare, so a change to lowering or to a pass shows in review without going through a backend.

use std::fmt::Write;

use crate::ir::{Block, Function, Operand, Place, Proj, StmtKind};
use crate::lower::Lowered;

impl Lowered {
    /// Every function of the program as text, in order.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for f in &self.functions {
            out.push_str(&self.function_text(f));
        }
        out
    }

    /// The function `f` as text.
    pub fn function_text(&self, f: &Function) -> String {
        let params: Vec<String> = f.params.iter().map(|&p| declared(f, p)).collect();
        let generic = if f.type_params.is_empty() { String::new() } else { format!("[{}]", f.type_params.join(", ")) };
        let mut out = format!("fn {}{generic}({}) -> {} {}\n", f.name, params.join(", "), f.ret, self.at(f.span.start));
        self.block(f, &f.body, 1, &mut out);
        out
    }

    /// `@line:column` of a byte offset of the source, both from 1.
    fn at(&self, offset: u32) -> String {
        let line = self.line_starts.partition_point(|&start| start <= offset);
        let column = offset - self.line_starts.get(line.wrapping_sub(1)).copied().unwrap_or(0) + 1;
        format!("@{line}:{column}")
    }

    fn block(&self, f: &Function, block: &Block, depth: usize, out: &mut String) {
        for stmt in block {
            let pad = "  ".repeat(depth);
            let at = self.at(stmt.span.start);
            let line = |text: String, out: &mut String| {
                let _ = writeln!(out, "{pad}{text} {at}");
            };
            match &stmt.kind {
                StmtKind::Let(l, e) => line(format!("let {} = {e:?}", declared(f, *l)), out),
                StmtKind::Store(place, v) => line(format!("{} = {}", place_text(place), operand(v)), out),
                StmtKind::Mutate { op, place, args, at: here, result } => {
                    let result = result.map(|r| format!("%{r} = ")).unwrap_or_default();
                    let here = if *here { " at" } else { "" };
                    line(format!("{result}{op:?}({}, {args:?}){here}", place_text(place)), out);
                }
                StmtKind::Do(e) => line(format!("do {e:?}"), out),
                StmtKind::If(test, then, otherwise) => {
                    line(format!("if {}", operand(test)), out);
                    self.block(f, then, depth + 1, out);
                    if !otherwise.is_empty() {
                        let _ = writeln!(out, "{pad}else");
                        self.block(f, otherwise, depth + 1, out);
                    }
                }
                StmtKind::Loop(body) => {
                    line("loop".to_string(), out);
                    self.block(f, body, depth + 1, out);
                }
                StmtKind::Break => line("break".to_string(), out),
                StmtKind::Continue => line("continue".to_string(), out),
                StmtKind::Return(v) => {
                    line(v.as_ref().map_or("return".to_string(), |v| format!("return {}", operand(v))), out)
                }
                StmtKind::ForRange { var, start, stop, step, body, exit } => {
                    let range = [start, stop, step].map(operand).join(", ");
                    line(format!("for {} in range({range})", declared(f, *var)), out);
                    self.block(f, body, depth + 1, out);
                    self.exit(f, exit, depth, out);
                }
                StmtKind::ForStr { var, over, body, exit } => {
                    line(format!("for {} in {}", declared(f, *var), operand(over)), out);
                    self.block(f, body, depth + 1, out);
                    self.exit(f, exit, depth, out);
                }
                StmtKind::Panic(p) => line(format!("panic {p:?}"), out),
                StmtKind::Inc(l) => line(format!("inc %{l}"), out),
                StmtKind::Dec(l) => line(format!("dec %{l}"), out),
                StmtKind::DropReuse { local, token } => line(format!("drop %{local} keeping token {token}"), out),
                StmtKind::Enter => line("enter".to_string(), out),
                StmtKind::Leave => line("leave".to_string(), out),
            }
        }
    }

    fn exit(&self, f: &Function, exit: &Block, depth: usize, out: &mut String) {
        if !exit.is_empty() {
            let _ = writeln!(out, "{}exit", "  ".repeat(depth));
            self.block(f, exit, depth + 1, out);
        }
    }
}

/// A local where it is declared: `%3 total: int`, `&` before an `inout` parameter.
fn declared(f: &Function, local: usize) -> String {
    let info = &f.locals[local];
    let by_ref = if info.by_ref { "&" } else { "" };
    match &info.name {
        Some(name) => format!("{by_ref}%{local} {name}: {}", info.ty),
        None => format!("{by_ref}%{local}: {}", info.ty),
    }
}

fn operand(o: &Operand) -> String {
    format!("{o:?}")
}

fn place_text(place: &Place) -> String {
    let mut text = format!("%{}", place.local);
    for proj in &place.proj {
        match proj {
            Proj::Index(i) => text.push_str(&format!("[{}]", operand(i))),
            Proj::OwnedIndex(i) => text.push_str(&format!("[own {}]", operand(i))),
            Proj::Field(k) => text.push_str(&format!(".{k}")),
            Proj::Key(k) => text.push_str(&format!("[key {}]", operand(k))),
            Proj::SetDefault(k, d) => text.push_str(&format!("[key {} or {}]", operand(k), operand(d))),
        }
    }
    text
}
