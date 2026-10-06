# Seeded failures — design

## What changes

Serves R1.1–R1.3, R2.1–R2.6, R3.1–R3.3.

`lotml dev mutate`, in the compiler, lists mutants; `harness/lotml_harness/guide/seeded.py`, run as
`python -m lotml_harness.guide.seeded`, gathers the programs, draws and judges mutants and writes
them as repair records. Output: `harness/cache/guide/seeded/<date>/train/repairs.jsonl` and
`validation/repairs.jsonl` — the trace dataset's layout, so the guide records builder reads both
alike (specs/guide-records/) — and `harness/results/seeded.md`, committed.

**The programs** (R1.1–R1.3). Three sources, all of them free to train on and none held out:

- the agent benchmark's tasks, solution laid over workspace with the hidden blocks appended, as
  the grader builds them (`bench.lay`, `grade`);
- the original HumanEval's canonical solutions for train and validation problems. Its release
  carries no types, so the signature takes the types the recorded cases' values have — the
  witness `humaneval.py` already derives to prove a task satisfiable — written as Python
  annotations (`[int]` as `list[int]`, `str?` as `str | None`, `f64` as `float`), and
  `corpus.rules.translate` does the rest. Rules only: the pipeline's model fallback would put a
  model's output in the data, which the registry has not cleared. A translation stands only if it
  checks and passes the problem's hidden blocks. The witness is still never shown to an agent;
  here it types a program nobody is asked to write;
- the final files of passing agent runs on train and validation problems whose source, model and
  providers the licence registry permits — the same gate the exporter applies
  (specs/trace-dataset/ R1.2).

The corpus is not a source: it translates MultiPL-E's typed originals.

**Mutants live in the compiler** (R2.1, R2.2). The operators need the syntax tree — names,
annotations, calls and their arguments, `?`, `var` bindings, binary operators, ranges, literals,
`if` conditions — and lotml's only parser is in Rust. `lotml-ide` gains `mutants(text)`, each
operator a function from the tree to a list of `(operator, declaration, span, replacement)`, every
mutant a single span replaced in the source; `lotml dev mutate <file> --json` prints them,
deterministic and in source order. The operators, by the diagnostic they aim at:

| family | operators | aims at |
| --- | --- | --- |
| names | misspell a local, parameter, function or field by one edit | E0201, E0205 |
| types | swap an annotation for a neighbour (`int`/`f64`, `[T]`/`T`, `T`/`T?`) | E0204, E0207 |
| calls | drop or duplicate an argument; drop a `?` after a fallible call | E0203, E0219 |
| mutability | drop a `var`, so a later assignment or mutation is refused | E0301, E0302 |
| meaning | swap `<`/`<=`, `+`/`-`, `and`/`or`; move a bound by one; swap two arguments of one type; negate a condition; change a constant; drop a statement | tests |

The meaning family follows DeepBugs' swapped arguments and wrong operators, and leaves the program
checking: what the guide must locate when no diagnostic points anywhere.

**Drawing and judging** (R2.3–R2.6). The driver weights each family by how often agents fail in it:
the first-error codes in `harness/results/phase1.md` and the diagnostics of the trace dataset's
repairs once it has them, mapped to families by code, with failing tests counted for the meaning
family. Counts are aggregates of codes — no prompt or answer of the phase 1 gate enters the data,
which `harness/results/NOTICE.md` forbids — and the report prints them. A seeded `random.Random`
draws up to 20 mutants per program and family, then each is judged in a scratch copy: `lotml check
--json`; if it checks, `lotml test --json` on the program's blocks. A mutant that checks and passes
is equivalent or untested and dropped; one that `check --fix` makes clean is dropped as a rule's
(R2.4). Mutants are keyed by problem and the SHA-256 of the text.

**The record** (R3.1, R3.2) is the trace dataset's repair: `before` the mutant, `after` the program,
the diagnostics or the failing block with its values, the changed lines, and `meta` with `origin:
seeded`, the operator, the problem and split from `split.py`, and the compiler version. The task's
prompt is the one its source gives: the benchmark's `prompt.md`, HumanEval's fixed prompt with the
docstring in the file. A program whose problem `split.py` holds out stops the run.

## Alternatives considered

- Mutating text with regular expressions in Python: misses the structure the operators need and
  makes mutants that fail to parse for the wrong reason.
- Uniform weights: DrRepair's 62.5 against 49.4 is the measured cost of ignoring what errors are
  frequent.
- Seeding the corpus's 509 programs: four times the programs, all from content that may not be
  trained on.

## Risks

- Few programs: about 123 HumanEval problems are in train and validation, and the rules translated
  59% of the corpus's HumanEval tasks from typed originals, where these get the witness's types;
  with the benchmark and the permitted runs, low hundreds of programs. Twenty mutants per family keeps thousands of records; the report says how many.
- The guide may learn the mutator's tells; the offline evaluation is on real failures only and is
  where that shows (specs/guide-evaluation/).
