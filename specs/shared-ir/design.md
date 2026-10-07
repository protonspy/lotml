# Shared IR — design

## What changes

Serves R1.1, R2.1, R3.1, R4.1.

A new crate, `compiler/crates/lotml-ir`, takes what the C backend's intermediate form already is
and makes it the one every backend reads. The C backend's lowering and passes move, they are not
rewritten (adr:0020); `lotml-c` keeps only the C emitter and its driver.

```
lotml_syntax::Module + lotml_check::Checked
  └─ lotml-ir::lower     generic IR: R1.1–R1.4, R2.1        ──► Python backend (specs/python-on-ir)
      └─ lotml-ir::mono  monomorphic IR, R3.3's refusal
          └─ own, reuse, hoist   counted IR, R3.2           ──► C backend · LLVM backend
```

| From `lotml-c/src/` | To `lotml-ir/src/` | What moves with it |
| --- | --- | --- |
| `mir.rs` | `ir.rs` | the data types, with the changes under Data |
| `lower.rs` | `lower.rs` and `mono.rs` | instantiation split out of lowering into its own pass |
| `own.rs`, `reuse.rs`, `hoist.rs` | the same names | unchanged, run by `lotml_ir::native(module)` |

`types.rs` stays in `lotml-c`: it is how C spells a type. The runtime the native targets link is
the crate plan task 1.1 creates, and it holds the table from each built-in operation to the
runtime function that performs it, which both native emitters read.

The API is two functions: `lotml_ir::lower(module, checked, source, tests) -> Result<Module,
Vec<Diagnostic>>` for the generic IR, and `lotml_ir::native(Module) -> Result<Module,
Vec<Diagnostic>>` for the monomorphic, counted one. `lotml check` calls neither (R2.2).

## Data

Serves R1.2, R1.3, R1.4.

- `Stmt.line: u32` becomes `Stmt.span: Span` (byte offsets into the source). The C emitter takes
  the line from it for `#line`; the Python backend needs the column, to underline the expression
  a traceback names. Every intermediate value is a `Let` of its own, so a span per statement is a
  span per expression.
- `Expr::Rt { name: &'static str, .. }` and `Mutate { name, .. }` name a C runtime function
  today. They take a `Builtin` enum instead (`ListAppend`, `StrRepr`, `DictGet`, …), one variant
  per operation the language defines.
- `Function.name` is a C name today. It becomes the LotML path plus the instance's type
  arguments; each native emitter makes its symbol from those with `lotml_ir::symbol`, so the C
  and LLVM targets, and the C ABI export (`specs/c-abi-export/`), name a function the same way.
- Calls to a function before `mono` carry their type arguments (`Call { callee, type_args,
  args }`); `mono` replaces them with calls to one instance per set of arguments.
- A call into a Python module through its interface is an expression of the IR
  (`CallPython { module, function, args, ret }`), beside `CallC`. The Python backend compiles it;
  the native targets refuse it at the import until `specs/python-bridge/`.
- The argument forms `Arg::Address`, `Out`, `Desc`, `Offset` and `Slot` say how the native
  runtime takes a value. They stay: the LLVM backend calls the same runtime. The Python backend
  reads the operand out of each and ignores descriptors and offsets.

## Alternatives considered

Instantiation stays inside lowering, as now: fewer moving parts, but the Python backend would
read every generic once per instance, and adr:0020 has it read them with generics intact. Kept
the forms of `Arg` rather than giving each backend its own calling convention, since two of the
three backends call one runtime.

## Risks

- `lower.rs` (3,852 lines) instantiates as it lowers; splitting instantiation out is the largest
  change. R4.2 and R4.3 are the guard, run before every commit that moves code.
- A rule lowered in a form only C could read — a temporary the Python backend cannot name, a
  place projection with no Python meaning — shows up in `specs/python-on-ir/`, not here. That
  spec changes the IR as a delta to this one.
