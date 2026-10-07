# Python on IR — design

## The generic IR

Serves R4.1, R4.2, R4.3.

`specs/shared-ir/` moved lowering into `lotml-ir` still instantiating as it goes. This spec splits
it: `lotml_ir::lower` keeps each generic function, method and lambda generic — a `Function`
carries its `type_params`, its types naming them as `Ty::Param` — and a pass, `lotml_ir::mono`,
makes one instance per set of type arguments before the native passes, refusing a generic that
needs instances without end. Three nodes carry what only the type arguments decide, and `mono`
resolves each once they are known:

- `CallGeneric { callee, args }`: a call of a generic function (`Callee::Function` with its type
  arguments), or of a method of a generic type or of a type parameter bound by a trait
  (`Callee::Method`, whose owner is the type); it becomes a `Call` or a `CallSlots` of the
  instance.
- `FnRefGeneric`: a generic function used as a value; it becomes a `FnRef` of the instance.
- `ToDynOf`: a value made a `dyn` from a type `mono` builds the table of; it becomes a `ToDyn`.
  The tables are made by `mono`, which reads each trait's callable methods from `dyn_methods`.

A trait's default method is lowered once per type implementing the trait without its own, `Self`
standing for that type; a lambda of a generic function is generic over its parameters and made
once per instance of it. Every other decision lowering takes from a type — which comparison, which
runtime function — is a node already carrying the type, which the native emitter reads after the
substitution. The native targets' parity and benchmarks hold through the split (R4.3).

```
lotml-ir::lower    generic IR       ──► Python backend
  └─ lotml-ir::mono   monomorphic IR
      └─ own, reuse, hoist   counted IR   ──► LLVM backend
```

## What changes

Serves R1.1, R1.2, R2.1, R3.1.

`compiler/crates/lotml-py/src/emit.rs` is replaced by `from_ir.rs`, written against
`lotml_ir::lower`'s output. The output keeps its shape: a stub module that hands
`lotml_rt.load_module` the program as a Python syntax tree in JSON, every node at a LotML
position, so `lotml_rt.py`'s protocol, `boundary.rs` and the `.pyi` writer are unchanged; the
runtime gains only the few operations the IR names that Python has no one expression for
(`MISSING`, `rename`, `closure`, `out`, `lookup`, `popped`, `split_pair`, `unpack`,
`sort_by_keys`) and lets a `Heap` be read by index.

- **What the IR carries for it.** Each statement keeps, beside its own span, the span of the
  expression it computes (`Stmt.at`, a delta to `specs/shared-ir/`). Each default of a
  parameter or field is lowered as a function of no parameters (`Lowered.defaults`), which a
  Python caller leaving it out gets: a LotML call fills defaults in where it is made. A
  built-in method the IR has no operation of its own for is `Expr::Method`, the method of the
  value at a place, which the Python target calls and a native target refuses; a `bytes`
  literal is `Const::Bytes`, and `PyError`'s fields are the checker prelude's.
- **Copies.** The native targets copy on write; Python shares, so the emitter copies where a
  `var` or an `inout` is involved, as the syntax tree's emitter did. A local is *mutated* when it
  is the root of an in-place change, an `inout` or a lent slot; it is *rooted* when it may hold
  what a mutated one holds, through a read of it, an element, a field or a built-in result. A
  value entering a mutated local is copied unless it was built where it is; a rooted value is
  copied where it is stored — a container, an argument, a binding, a returned value — and a
  value stored into a mutated container is copied unless it is new. A temporary holding the
  whole value of a mutated local — the snapshot a loop walks — is a copy.

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
