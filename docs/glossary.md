# Glossary

One canonical term per concept, and the synonyms nobody should use for it. One entry per
line: the term in bold, the definition after an em dash, and an optional `Avoid:` list.
Every avoided synonym is reported wherever it appears as a whole word under `docs/`.

- **LotML** — the LLM-oriented programming language this repository studies, Lot from Lotus and ML from Machine Learning; its source files use `.lot`, or `.lotml` as written before 2026-10-07, and `lotml` is its command (adr:0018-lot-as-the-preferred-source-extension). The pilot and the corpus under `research/` call it by the provisional name X, with the `.x` extension. Avoid: language X
- **variant A** — lotml's syntax exactly as the original study proposes it: `none`, `match` arms without `case`, `=>`, `use`, `or` for optionals.
- **variant B** — variant A with its constructs replaced by Python's wherever the semantics match: `None`, `case`, `lambda`, `from … import`, `??`, `is None`.
- **training prior** — what a model already knows about a construct from having seen it in pretraining; it is why a construct identical to Python's comes out right without instruction.
- **Python leakage** — a construct that is valid in Python and invalid in the variant in use, showing up in code generated in lotml.
- **paired corpus** — the equivalent programs in typed Python, variant A and variant B under `research/tokens/corpus/`, used to measure tokens.
- **semantic compiler** — lotml's compiler treated as a tool an agent calls in a loop: structured diagnostics, applicable fixes, digest and queries.
- **digest** — the semantic compiler's output listing only a module's public signatures, types and documentation, so it can serve as the project's index in the model's context.
- **constrained decoding** — generation in which the inference engine only accepts tokens that keep the output valid under a grammar or a type checker.
- **value semantics** — the model in which assigning or passing a value amounts to copying it; the compiler turns the copy into a move or a borrow when it proves that safe.
- **colorless concurrency** — concurrency in which no function is marked `async` or requires `await`; tasks run on green threads.
- **evaluation harness** — the set of tasks, models and metrics that settles syntax questions by measurement; the roadmap's first deliverable.
- **gate** — a criterion measured by the evaluation harness that must pass before the next roadmap phase starts.
- **slip** — an editing error that puts lines at the wrong block depth: wrong whitespace in the indented form, a misplaced `}` in the braces form; silent when the program still parses. Avoid: indentation bug
- **editing pilot** — models applying search-and-replace edits to the indented and the braces form of the paired corpus's programs, scored by applying, parsing and running hidden tests.
- **source verification** — checking every number the study quotes against the downloaded paper, by a verbatim quote that `research/literature/verify.py` finds in the converted text.
- **compiler-embedded model** — a small language model specialised to one language and shipped inside its compiler or toolchain to repair errors, explain diagnostics or suggest improvements; lotml has none.
- **prefix check** — the semantic compiler judging a partial file while a model writes it: completable, an error here, or not yet decidable; it never rejects a prefix that can still be completed.
- **symbol-addressed edit** — an edit that names a syntax entity (a function body, a `match` arm, a method) and gives only its new content; the tool re-indents it and rejects it if it breaks the syntax. Avoid: AST edit
- **agent guide** — what `lotml init` writes for coding agents: the lotml block in `AGENTS.md` and `lotml.guide.lot`, the language taught as code that checks and passes its tests.
- **agent harness** — `harness/lotml_harness/agent/`: a coding agent run on the agent benchmark's development tasks with the compiler's MCP tools, graded on hidden tests.
- **harness guide** — the compiler-embedded model planned to guide any coding agent over MCP: when `check` refuses code or a test fails, it says where to change it and what kind of change, proposes at most one patch that checks, and stays silent below a calibrated confidence; the agent edits.
- **trace dataset** — the training data exported from the agent harness's traces: trajectories of runs that passed and repairs that made `check` clean or a failing test pass, taken only from sources and models the licence registry permits.
- **problem split** — the assignment of each benchmark problem to train, validation or held-out, shared by every source derived from it, so the harness guide is never scored on a problem it was trained on. Avoid: data split
- **interface** — the bodyless signatures of a foreign module, imported by origin as `py.<module>` (each returning `T ! PyError`) or `c.<library>`. The compiler generates it from the stub or header (adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated); a `.lotmli` file checked into `bindings/` overrides it. Python's is generated on import, running no Python (specs/bind-on-import), and `lotml bind` writes it out for review; C's are written by hand until plans/c-header-import.md ships. Avoid: binding file
- **package** — LotML source distributed as a wheel on PyPI, declared and locked like a Python dependency and imported bare, `import <module>`; the compiler finds it in the project's environment without running Python (adr:0038-a-facade-is-a-lotml-package-published-on-pypi-as-a-wheel)
- **facade** — a package that gives one foreign library a LotML-shaped API: LotML over `py.` imports, the library's exceptions mapped to error values, and a Python module of its own, stubbed, where LotML cannot express a part; distributed as `lotml-<library>`, its module named after the library (adr:0038-a-facade-is-a-lotml-package-published-on-pypi-as-a-wheel)
- **overload** — one of the signatures a Python stub declares for one function, constructor or method with `@overload`; an interface writes each, in order, and a call is typed by the first its arguments fit (adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call). LotML's own functions have none
- **re-export** — a name a Python stub imports from another module and makes its own, by `__all__`, by `n as n` or by `*`; the binder binds it from the stub of the module it comes from (specs/python-reexports)
- **type variable** — a Python stub's `TypeVar`; an interface writes a plain or bounded one as a type parameter, `fn first[T](xs: [T]) -> T`, and a constrained one as an overload per type (adr:0036-a-python-type-variable-crosses-as-a-type-parameter-checked-where-a-value-crosses)
- **GRPO** — Group Relative Policy Optimization: reinforcement learning that samples a group of answers per prompt and scores each against the group's mean reward, with no learned critic.
- **verifiable reward** — a reward computed by a program, such as the compiler or a test run, rather than by a learned model.
- **reward hacking** — a policy raising its reward by exploiting the grader, such as exiting before the tests run or editing them, instead of solving the task.
- **on-policy distillation** — distillation in which the student samples its own answers and a teacher scores every token of them; it needs the teacher's probabilities and a shared tokenizer.
- **IR** — the typed, structured intermediate form between the checker and every backend, in `compiler/crates/lotml-ir`: nested blocks, every intermediate value a named local, every statement the span of its source statement and of the expression it computes; the Python backend reads it with generics intact, the native backend after `mono` has made it monomorphic and the passes have counted it (adr:0020-one-ir-between-the-checker-and-every-backend). Avoid: MIR
- **target** — what `--target` compiles a program to: `python`, which `lotml run` and `lotml test` use and which reaches Python's libraries, or `llvm`, the native target `lotml build` uses (adr:0025-two-targets-python-for-run-llvm-for-build); a backend is the crate that compiles to one.
- **export** — a top-level function a shared library built by `lotml build --shared` lets C call, under the symbol `<module>_<function>`, chosen because C can be given its parameters and result (adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax).
- **parity suite** — the programs run on both targets with their reports compared, the Python target the reference: the corpus through `harness/lotml_harness/experiments/parity.py`, and the hand-written programs of `compiler/crates/lotml-llvm/tests/` (`docs/wiki/pages/target-parity.md`)
- **parity floor** — the committed list of programs the parity suite must keep passing, with their count; CI fails when a program leaves it, never because a program has not joined it yet
