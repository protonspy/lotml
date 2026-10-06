# Seeded failures — design

## What changes

Serves R1.1–R1.3, R2.1–R2.7, R3.1–R3.3.

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
  witness specs/agent-humaneval/ derives to prove a task satisfiable — written as Python
  annotations (`[int]` as `list[int]`, `str?` as `str | None`, `f64` as `float`), and
  `corpus.rules.translate` does the rest. Rules only: the pipeline's model fallback would put a
  model's output in the data, which the registry has not cleared. A translation stands only if it
  checks and passes the problem's hidden blocks; one whose helper functions carry no types is
  refused, since the witness types only the function the tests call. The witness is still never
  shown to an agent;
  here it types a program nobody is asked to write;
- the files at the end of the trajectories the trace dataset exported, read from its `train/` and
  `validation/` output rather than from the raw traces, so the licence registry, the identity
  checks and the scrub of secrets and hidden asserts have already been applied
  (specs/trace-dataset/).

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
which `harness/results/NOTICE.md` forbids — and the report prints them. Each program gets a budget
of 60 mutants; the weights split it among the five families by largest remainder, and a family
with fewer mutants than its share passes the rest to the others in weight order, so the weights
decide the mix even on a small program (R2.5). Within a family a seeded `random.Random` draws
uniformly from all its operators' mutants (R2.6). Each drawn mutant is judged in a scratch copy:
`lotml check --json`; if it checks, `lotml test --json` on the program's blocks. Judging follows the
draw, so dropped mutants leave a program below its budget rather than redrawing. The scratch copy is
written by the agent harness's `lay`, which refuses absolute names, `..`, reserved device names and
any suffix but `.lotml`, and holds no interface; every `lotml` call goes through one helper —
`execute.child_environment()`, `execute.limit_memory` (a job object on Windows, which ends the
call's children with it), a new session killed as a group on POSIX, files after `--` — at a
deadline per call, since a negated loop condition is a mutant that never ends (R2.7). The records
are written with the notices of their sources beside them, as the trace dataset writes its own. A mutant that checks and passes
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
- Seeding the corpus's 509 programs: several times the programs, all from content that may not be
  trained on.

## Risks

- Few programs: about 123 HumanEval problems are in train and validation. The rules translated 59%
  of the corpus's HumanEval tasks from fully typed originals; here only the tested function gets
  types, so a solution with an untyped helper or nested function is refused, and 59% is an upper
  bound. With the benchmark and the permitted runs, low hundreds of programs; sixty mutants each
  keeps thousands of records, and the report says how many.
- The guide may learn the mutator's tells; the offline evaluation is on real failures only and is
  where that shows (specs/guide-evaluation/).
