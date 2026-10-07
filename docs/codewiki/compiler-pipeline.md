# The compiler pipeline

How a `.lot` file becomes a running program: one lowering into one IR, read by two targets —
Python, which `lotml run` and `lotml test` use and which reaches Python's libraries, and LLVM,
which `lotml build` uses to make an executable or a library C calls
(adr:0020-one-ir-between-the-checker-and-every-backend,
adr:0025-two-targets-python-for-run-llvm-for-build). What follows is where each step lives and
what holds it together that no one file shows.

## Where a command picks its target

[compiler/crates/lotml/src/exec.rs:197-261]()

`build`, `run` and `test` each take `--target`, and each has its own default: `build` is native
(adr:0022), `run` and `test` are Python, because the development loop a model iterates in needs
Python's ecosystem and a quick start, and a native program never loads CPython. The LLVM target
runs at `-O0` with line tables for `run` and `test`, at `-O2` for `build`; `--shared` is only
native, since a library C calls cannot carry an interpreter.

## One lowering, read twice

[compiler/crates/lotml-py/src/lib.rs:60-72]()

[compiler/crates/lotml-llvm/src/lib.rs:48-83]()

Both targets call the same `lotml_ir::lower` on the same checked program, so every rule past the
checker — what a `for` over a dict walks, how `?` returns, what `sum` of an empty list gives — is
decided once. The Python target reads the IR as lowering leaves it, generic; the LLVM target first
makes it monomorphic and inserts the counts, then writes LLVM IR. A Python module import is
refused only on the native side, at the import, because CPython is what a native program does
without; the lowering itself compiles a Python call, which the Python target needs.

## Generics stay generic until a native target asks

[compiler/crates/lotml-ir/src/lower.rs:132-142]()

[compiler/crates/lotml-ir/src/lower.rs:3800-3825]()

Lowering writes a generic function once, its types naming its parameters, and a call of it — or a
method of a generic type, or of a type parameter through its trait bound — as `CallGeneric`
carrying the type arguments. That is what lets the Python target write one Python function per
generic function, as Python callers through the boundary expect, and it is why nothing in
lowering may decide on the concrete type of a type parameter: whatever depends on it is a node
that already carries the type.

## What `mono` does that lowering used to

[compiler/crates/lotml-ir/src/mono.rs:158-220]()

[compiler/crates/lotml-ir/src/mono.rs:281-313]()

`mono` walks the program from its non-generic functions, giving each set of type arguments a call
asks for an instance, and resolving each generic node in it once its types are known. A lambda
written inside a generic function is generic too, so it is made once per instance of that
function, its index in the closure renumbered. The `dyn` tables are made here as well, since a
table's slots are method instances. The limits on instances and on the size of a type argument
stop a recursion that grows its type argument — `f([x])` inside `f[T]` — which would otherwise
ask for instances without end.

## Counting is for the native target only

[compiler/crates/lotml-ir/src/lib.rs:16-27]()

The native program manages memory by counts the passes insert: a count added where a value is
stored, taken where its last use ends, a cell reused by the constructor after a `match` arm that
took it apart, and the uniqueness check of a list a loop changes moved before the loop. The
verifier runs after each pass in a debug build, so a pass that breaks the IR's invariants fails
where it broke them rather than as a wrong program.

## Value semantics without counts

[compiler/crates/lotml-py/src/from_ir.rs:600-698]()

[compiler/crates/lotml-py/src/from_ir.rs:911-945]()

The native target copies a value when it changes while shared; Python shares, so the Python
target copies instead, where a `var` or an `inout` is involved. The analysis marks the locals that
change in place, the locals that may hold what one of those holds, and the temporaries that hold a
value built where they are; `let_stmt` and `stored` copy a value entering a changed local, a
shared value stored anywhere it outlives the read, and the snapshot a loop walks. The copy itself
is per part (`lotml_rt.copy`), since a value holding one list twice holds two lists.

## The runtime's ABI is read from its header

[compiler/crates/lotml-runtime/src/abi.rs:84-100]()

[compiler/crates/lotml-llvm/src/layout.rs:155-170]()

The LLVM IR calls the C runtime by name, so each declaration it writes must say what `lotml.h`
says. Rather than a second list of signatures, `abi` parses the header the runtime ships, and
every runtime function takes the place it may stop at by pointer, because LLVM IR leaves a struct
passed by value to the frontend's C ABI. The structs the emitter lays out itself are checked
against `sizeof` and `offsetof` from the same `clang` that builds the program.

## Lines a debugger and a traceback read

[compiler/crates/lotml-llvm/src/emit.rs:451-465]()

[compiler/crates/lotml-llvm/src/module.rs:44-90]()

Each IR statement carries the span of the source statement and of the expression it computes. The
Python target places every node at the expression's span, so a traceback underlines it; the LLVM
target turns the same spans into a `DISubprogram` per function and a `!dbg` per instruction at
`-O0`, as DWARF, and as CodeView beside it on Windows.

## A library C calls

[compiler/crates/lotml-llvm/src/export.rs:52-95]()

[compiler/crates/lotml-llvm/src/emit.rs:172-230]()

[compiler/crates/lotml-runtime/c/lotml.c:95-100]()

The functions a library exports are chosen from the checker's signatures before lowering, so the
warning for one left out points at its declaration
(adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax). Each wrapper is the only external
symbol: it marks the runtime as called from a host, which locks the output buffer from then on,
copies each `str` argument after checking it is UTF-8 — the module's function takes it over, as
any callee does — and writes out what the function printed before returning.
