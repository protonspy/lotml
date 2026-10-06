# Glossary

One canonical term per concept, and the synonyms nobody should use for it. One entry per
line: the term in bold, the definition after an em dash, and an optional `Avoid:` list.
Every avoided synonym is reported wherever it appears as a whole word under `docs/`.

- **lotml** — the LLM-oriented programming language this repository studies; its source files use the `.lotml` extension. The pilot and the corpus under `research/` call it by the provisional name X, with the `.x` extension. Avoid: language X
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
