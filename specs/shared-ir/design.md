# Shared IR — design

## What changes

Serves R1.1, R2.1, R3.1, R4.1.

A new crate, `compiler/crates/lotml-ir`, takes what the C backend's intermediate form already is
and makes it the one every backend reads. The C backend's lowering and passes move, they are not
rewritten (adr:0020); `lotml-c` keeps only the C emitter and its driver.

```
lotml_syntax::Module + lotml_check::Checked
  └─ lotml-ir::lower     monomorphic IR: R1.1–R1.4, R2.1, R3.3's refusal
      └─ own, reuse, hoist   counted IR, R3.2               ──► C backend · LLVM backend
```

| From `lotml-c/src/` | To `lotml-ir/src/` | What moves with it |
| --- | --- | --- |
| `mir.rs` | `ir.rs` | the data types, with the changes under Data |
| `lower.rs` | `lower.rs` | instantiation still inside it; `specs/python-on-ir/` splits it out |
| `own.rs`, `reuse.rs`, `hoist.rs` | the same names | unchanged, run by `lotml_ir::native(module)` |

`types.rs` stays in `lotml-c`: it is how C spells a type. The runtime the native targets link is
the crate plan task 1.1 creates, and it holds the table from each built-in operation to the
runtime function that performs it, which both native emitters read.

The API is two functions: `lotml_ir::lower(module, checked, source, tests) -> Result<Module,
Vec<Diagnostic>>` for the monomorphic IR, and `lotml_ir::native(Module) -> Module` for the
counted one. `lotml check` calls neither (R2.2).

## Data

Serves R1.2, R1.3, R1.4.

- `Stmt.line: u32` becomes `Stmt.span: Span` (byte offsets into the source). The C emitter takes
  the line from it for `#line`; the Python backend needs the column, to underline the expression
  a traceback names. Every intermediate value is a `Let` of its own, and each statement keeps
  beside its span the span of the expression it computes (`Stmt.at`, added by
  `specs/python-on-ir/`), so an operand inside a larger statement is underlined on its own.
- `Expr::Rt { name: &'static str, .. }` and `Mutate { name, .. }` name a C runtime function
  today. They take a `Builtin` enum instead (`ListAppend`, `StrRepr`, `DictGet`, …), one variant
  per operation the language defines.
- `Function.name` is the function's symbol, made by `lotml_ir::symbol` — `lf_` for a module
  function, `lm<n>_` for a method of an owner `n` characters long, `li<k>_` for an instance,
  `ll` for a lambda, `lt_test` for a `test` block — so the C and LLVM targets, and the wrappers
  of the C ABI export (`specs/c-abi-export/`), name a function the same way; `source_name` is
  its LotML name, the one a panic reports. The symbols before the IR let a function named
  `lambda0` collide with the first lambda and `a_b.c` with `a.b_c`; the prefixes keep them apart.
- A call into a Python module through its interface is an expression of the IR
  (`CallPython { module, function, args, ret }`), beside `CallC`. The Python backend compiles it;
  the native targets refuse it at the import until `specs/python-bridge/`.
- The IR prints as text, one statement per line with its span (R1.5): what `lower` and the
  native passes produce is compared in tests as text, so a change to a pass shows in review
  without going through a backend. A verifier (R1.6) runs after each pass in test builds and
  checks every local is set before it is read and every operand has the type its use needs; the
  three emitters then rely on those facts instead of each re-checking them.
- The argument forms `Arg::Address`, `Out`, `Desc`, `Offset` and `Slot` say how the native
  runtime takes a value. They stay: the LLVM backend calls the same runtime. The Python backend
  reads the operand out of each and ignores descriptors and offsets.

## Alternatives considered

Splitting instantiation out of lowering here, as this spec first planned: adr:0020 has the Python
backend read generics intact, but neither native backend needs the split, and it is the largest
change to `lower.rs`; it moves to `specs/python-on-ir/`, the first spec that needs it, so the
LLVM target arrives on the IR first (plan reorder). Kept the forms of `Arg` rather than giving each backend its own calling convention, since two of the
three backends call one runtime.

## Risks

- `lower.rs` (3,852 lines) moves crates and trades C runtime names for built-ins and C function
  names for LotML paths. R4.2 and R4.3 are the guard, run before every commit that moves code. R4.3 compares
  against a baseline measured on the same machine just before the move, never against the
  recorded run of another session: the shortest benchmarks take 19 ms, where noise alone
  exceeds 5%, so the gate is the geometric mean, with a looser bound per program.
- A rule lowered in a form only C could read — a temporary the Python backend cannot name, a
  place projection with no Python meaning — shows up in `specs/python-on-ir/`, not here. That
  spec changes the IR as a delta to this one.
