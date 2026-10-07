# Python on IR — design

## What changes

Serves R1.1, R1.2, R2.1, R3.1.

`compiler/crates/lotml-py/src/emit.rs` is rewritten against `lotml_ir::lower`'s output; nothing
else in the crate moves. The output keeps its shape: a stub module that hands
`lotml_rt.load_module` the program as a Python syntax tree in JSON, every node at a LotML
position, so `lotml_rt.py`, `boundary.rs` and the `.pyi` writer are untouched.

- **Statements.** Each IR statement becomes Python statements at its span. A `Let` of an
  intermediate value becomes an assignment to a Python local named for the IR local (`_t12`),
  carrying the span of the expression it computed — which is what gives a traceback its
  underline (R2.1).
- **Control flow.** `If` and `Loop` with `Break`/`Continue` map one to one; `ForRange` and
  `ForStr` become `for` over `range` and over the string, their `exit` block the loop's `else`.
  A `match` arrives as a test of the tag and the field reads that take the value apart, and is
  written as that.
- **Built-ins.** One table maps each `lotml_ir::Builtin` to the `lotml_rt` function or Python
  operation that performs it. The argument forms that only the native runtime needs (`Desc`,
  `Offset`) are dropped, and the operand is read out of the others.
- **Generics.** The IR before `mono` keeps them generic (R1.2); Python needs no instances, so
  type arguments are read only where the checker's types decide a behaviour — which arithmetic
  traps outside its integer type, which values are copied.
- **Foreign calls.** `CallPython` becomes the call to `lotml_rt.foreign` it is today, and
  `CallC` the `ctypes` call (R3.1, R3.2).

## Risks

- The tree written today places some positions at sub-expressions the IR has no statement for,
  such as an operand inside an f-string. Where a traceback test fails for that reason, the fix is
  a span on the IR's operand, made as a delta to `specs/shared-ir/`.
- Building and running gain the lowering the Python target did not have (adr:0020). If the
  agent harness's per-iteration time moves, it is measured before the old emitter is removed.
