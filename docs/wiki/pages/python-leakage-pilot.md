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
  wrote `fn withdraw(var ledger: {str: int}, …)` and a test that expects to see the caller's
  `ledger` changed. With value semantics ([[memory-model]]) the parameter is a copy and the test
  would fail. Sonnet and Opus took the path the spec offers: a `Ledger` record with `var self`
  methods.

## Two failures were the grammar's, not the models'

The first run counted six failures. Two were defects in the pilot's grammar: `**` was lexed as
two `*`, and a negative literal in a pattern (`case Err(Negative(-5))`) was rejected. Both were
fixed, with tests, before recounting. It is the effect the literature measures in
[[constrained-decoding]]: an incomplete grammar rejects valid programs, and used to constrain
generation it degrades quality instead of protecting it.

## What the pilot supports

1. **The original study's risk moves.** With the spec in the prompt and the language named,
   Claude models did not mix in Python syntax. The literature points the same way: naming the
   language explicitly raised adherence above 99% in
   [Moumoula et al.](https://arxiv.org/abs/2503.13620). What leaks is semantics — "everything is
   mutable" and "arguments are references" — and the parser cannot catch that; only the
   [[semantic-compiler]] can.
2. **Variant A and variant B tied** at this sample size (28 of 30 each). The pilot gives no
   evidence for B on syntactic leakage; the choice rests on the semantic arguments in
   [[lotml-syntax]], and the [[evaluation-harness]] repeats the comparison.
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
- **Nothing was executed.** Without a compiler or transpiler, the pilot measures parsing and
  static rules, not functional correctness. Types, `match` exhaustiveness and unchecked
  optionals were left out.
- **Spec in the prompt.** The result holds for use with the spec in context, which is the
  scenario the original study plans for closed models; it says nothing about generation without
  it.
