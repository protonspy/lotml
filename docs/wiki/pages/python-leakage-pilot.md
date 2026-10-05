# Python leakage pilot

The original study names the lack of a corpus as risk number one: the model "slips" into Python.
This pilot measured that for the first time, with real models writing lotml from the
specification alone. Result: with the spec in the prompt, Python leakage did not show up in the
syntax — it showed up in the semantics, and only in the smallest model.

## Method

- **The specs use the provisional name X**, with the `.x` extension: the name lotml and the
  `.lotml` extension came after the pilot.
- **Two compact specifications**, `research/pilot/spec-a.md` (variant A) and `spec-b.md`
  (variant B), of about 1,830 tokens each on `o200k`. They differ only in the constructs that
  distinguish the variants (see [[lotml-syntax]]).
- **Ten tasks** in `research/pilot/tasks.md`, worded without Python vocabulary ("record",
  "error", "nothing" instead of `class`, exception, `None`): a record, a sum type, a parse with
  three errors, an optional, a generic queue, word counts, a ledger with propagated errors, an
  interface with two implementations, an expression tree and a CSV parse.
- **Three models × two variants**: Claude Haiku, Sonnet and Opus, each in a fresh agent with no
  prior context, told to read only the spec and the tasks, write each solution once and not
  revise it. Outputs are in `research/pilot/runs/<variant>-<model>/`.
- **Three automatic checks**, all with tests in `research/pilot/`:
  - *parse* — a Lark grammar for each variant (`check.py`), validated against the whole paired
    corpus;
  - *syntactic leakage* — Python constructs outside strings and comments (`def`, `class`,
    `raise`, `try`, `except`, decorators, `Optional[`, `isinstance` and, in variant A only,
    `None`, `lambda`, `import`, `case`, `is`);
  - *semantic leakage* — walking the tree (`semantics.py`): reassigning or mutating an immutable
    local, mutating `self` without `var self`, truthiness on a value that is not `bool`, and
    `var` parameters.
- **Reproduction:** `analyze.py` in `research/pilot/` regenerates `results.md` and
  `results.json`; the commands are in `research/README.md`.

## Results

| run | programs | parse | syntactic leakage | semantic violations |
| --- | ---: | ---: | ---: | ---: |
| A · Haiku | 10 | 8 | 0 | 1, plus 3 `var` parameters |
| A · Sonnet | 10 | 10 | 0 | 0 |
| A · Opus | 10 | 10 | 0 | 0 |
| B · Haiku | 10 | 8 | 0 | 3, plus 3 `var` parameters |
| B · Sonnet | 10 | 10 | 0 | 0 |
| B · Opus | 10 | 10 | 0 | 0 |

- **No Python syntax in 60 programs**, across both variants and all three models — not even
  `None` in variant A, where it is invalid.
- **All four parse failures are Haiku's**, two per variant, and none of them is Python:
  - `type Queue[T](var items: [T])` — `var` on a record field, as in a Swift `struct`;
  - `Negate(Expr)` and `Add(Expr, Expr)` — variants with positional fields, as in Rust and
    Swift; the spec only shows named fields.
- **The semantic violations are Python habits that get past the parser**, all from Haiku:
  - `line = line.strip()` reassigning the loop variable (twice, once per variant);
  - a local list created without `var` and then mutated with `append` — `items` and
    `descriptions`, in variant B.
- **Haiku assumed reference semantics for arguments.** In the ledger task, in both variants, it
  wrote `fn withdraw(var ledger: {str: int}, …)`. In variant A its test expects the caller's
  `ledger` to change after a transfer; in variant B its test only checks that nothing changes
  after a failed one. Executing the programs (below) confirmed that only the variant A test
  depends on reference semantics. Sonnet and Opus took the path the spec offers: a `Ledger`
  record with `var self` methods.

## Two failures were the grammar's, not the models'

The first run counted six failures. Two were defects in the pilot's grammar: `**` was lexed as
two `*`, and a negative literal in a pattern (`case Err(Negative(-5))`) was rejected. Both were
fixed, with tests, before recounting. It is the effect the literature measures in
[[constrained-decoding]]: an incomplete grammar rejects valid programs, and used to constrain
generation it degrades quality instead of protecting it.

## Executed

`research/experiments/transpiler/` transpiles the pilot's programs to Python and runs their own
`test` blocks, once with lotml's semantics — value semantics, conditions that only accept `bool`
in variant B, and variant A's `or` and `if x:` that test optionals for absence — and once with
Python's: references, truthiness and Python's `or`. Every node carries its lotml position, so a
failure reports the `.x` line ([[transpilation-strategy]]).

| run | programs that ran | tests | pass, lotml | pass, Python semantics |
| --- | ---: | ---: | ---: | ---: |
| A · Haiku | 8 | 8 | 7 | 7 |
| A · Sonnet | 10 | 30 | 30 | 29 |
| A · Opus | 10 | 33 | 33 | 33 |
| B · Haiku | 8 | 11 | 11 | 11 |
| B · Sonnet | 10 | 35 | 35 | 35 |
| B · Opus | 10 | 30 | 30 | 30 |

- **Every program that parses passes its own tests under lotml's semantics**, except Haiku's
  variant A ledger, whose test expects the caller's dictionary to change: it fails under value
  semantics and passes under references. The tests are the models' own, so this shows each
  program does what its author meant, not that it solves the task.
- **Three tests depend on which semantics runs them, all in variant A.** Besides the ledger, Sonnet
  wrote `s.to_int() or fail NotNumber(s)` and tested `"0"`; Haiku wrote `if maybe_n:` on an
  optional integer and tested `"0"`. Under variant A's semantics both return 0, as their authors
  intended; read with Python's semantics, `0` is falsy and both fail. The trap adr:0004 removes is
  not hypothetical: two of 18 variant A programs that ran exercise it in ordinary code, and a
  reviewer reading them with Python's semantics would predict the wrong result.
- **No variant B test changes outcome between the two semantics.** B's `??` and `is not None`
  mean the same thing to a Python reader and to lotml.

## What the pilot supports

1. **The original study's risk moves.** With the spec in the prompt and the language named,
   Claude models did not mix in Python syntax. The literature points the same way, though less
   cleanly than first reported: [Moumoula et al.](https://arxiv.org/abs/2503.13620) saw adherence
   above 99% for nearly all models (the lowest at 93.07%) on a benchmark that names the language,
   against 74.33–97.60% on one that does not — two datasets, not a controlled ablation.
   What leaks is semantics — "everything is mutable" and "arguments are references" — and the
   parser cannot catch that; only the [[semantic-compiler]] can.
2. **Variant A and variant B tied on parsing** at this sample size (28 of 30 each), but execution
   separates them: two variant A programs depend on a semantics a Python reader gets wrong, and no
   variant B program does. That is the first measured evidence for adr:0004; the
   [[evaluation-harness]] repeats the comparison at scale.
3. **The errors that did appear come from other priors**, Swift and Rust. Accepting positional
   fields in variants costs little and removes a whole class of error.
4. **The specification must say how a function changes a caller's value.** Neither the original
   study nor the pilot spec does, and the model that needed it invented reference semantics —
   see [[memory-model]].

## Limitations

- **One model family.** Claude only; smaller open models should leak more, as
  [[training-prior]] shows.
- **Small sample:** ten short tasks per cell, one sample per task, no temperature control.
  Differences of one or two failures are not signal.
- **Execution covers the models' own tests only.** The research transpiler erases types, so type
  errors, `match` exhaustiveness at compile time and unchecked optionals are still not measured;
  tests written by the same model that wrote the code are a weak oracle.
- **Spec in the prompt.** The result holds for use with the spec in context, which is the
  scenario the original study plans for closed models; it says nothing about generation without
  it.
