# lotml syntax

lotml's syntax starts from Python 3.12 and diverges only where the divergence removes a class of
error. This page records the rule that decides each construct, the two variants measured, what
the measurements said and the gaps the original proposal left open.

## The rule

The original study sets two clauses, and the evidence adds a third:

1. **Same syntax ⇒ same semantics.** The model brings Python's behavior along with the word.
2. **Different semantics ⇒ visibly different syntax.** Hence `fn` and not `def`: a lotml
   function requires types and does not throw exceptions.
3. **Same semantics ⇒ Python's syntax.** Copying Python costs no tokens — `def` and `fn`,
   `list[int]` and `[int]`, `from … import` and `use` tie on all eight tokenizers measured
   ([[token-cost]]) — and inherits the [[training-prior]].

## Variant A and variant B

Variant A is the original proposal. Variant B replaces the constructs that break the rule:

| construct | variant A | variant B | why B |
| --- | --- | --- | --- |
| absence of a value | `none` | `None` | the same semantics as `None` under `mypy --strict`; it is what the model writes |
| `match` arm | `Circle(r):` | `case Circle(r):` | Python 3.10's `match` is the same structural matching; `case` costs 1 token |
| optional default | `x or default` | `x ?? default` | Python's `or` falls back to the default on any falsy value (`0`, `""`, `[]`); in A, only on `none`. Same syntax, different semantics. `??` costs the same and has a prior in C#, Swift, Kotlin and JavaScript |
| optional test | `if x:` | `if x is not None:` | same reason: in Python `if x:` is false for `0`; costs 3 more tokens |
| lambda | `u => u.age` | `lambda u: u.age` | same semantics; 1 more token |
| import | `use a.b.{c, d}` | `from a.b import c, d` | same semantics, same cost; Python 3 no longer has implicit relative imports (PEP 328), contrary to what the original study assumes |
| expected error in a test | `== fail Minor(15)` | `== Err(Minor(15))` | `fail` as an expression is odd; `Err` is the constructor `match` already exposes |

Both keep: `fn`, `type` for records and sum types, `T?`, `T ! E`, `?`, `fail`, `var`, `[T]`,
`{K: V}`, `(A, B)`, `impl`, `trait`, `test` blocks and significant indentation.

## What the measurements said

- **Tokens:** variant B costs 0.8 percentage points more than A over the paired corpus
  (0.899–0.915 of typed Python's tokens, against 0.891–0.909 for A) — see [[token-cost]].
- **Pilot:** they tied — 28 of 30 programs parse in each, with no Python syntax leakage in
  either — see [[python-leakage-pilot]].
- **The tie moves the decision to semantics.** The `or` and `if x:` traps in variant A do not
  show up in the parser: a program using `x or 0` with `x = 0` parses, passes the type checker and
  behaves differently from what the model's prior predicts. Variant B removes both traps for a
  few tokens.
- **Execution found the trap in model-written code.** Running the pilot's programs under both
  semantics, two of 18 variant A programs — Sonnet's `s.to_int() or fail NotNumber(s)` and Haiku's
  `if maybe_n:` — return 0 for `"0"` as lotml's A semantics says, and fail under Python's; their
  authors even tested `"0"`. No variant B test changes outcome ([[python-leakage-pilot]]).
- **Familiar syntax with a new meaning is the costliest kind.** Asked to predict what programs do
  under supplied semantics, models lost 40–70 points when familiar operators carried new meanings,
  far more than when the same rules used new symbols, and chain-of-thought did not recover it
  ([arXiv 2510.03415](https://arxiv.org/abs/2510.03415); reading code, not writing it). That is the
  rule's second clause measured: different semantics needs visibly different syntax.

**Decision:** variant B (adr:0004-python-syntax-where-semantics-match); the
[[evaluation-harness]] repeats the comparison with more models and families before v1 freezes.

**Repeated at scale.** Four models of three families — Claude Sonnet 5.5 and Haiku 4.5, Qwen2.5
Coder 7B, Llama 3.1 8B — wrote the same 200 HumanEval and MBPP tasks from each variant's reference
(`harness/results/variants.md`). Variant B passed at least as often for three of them and
significantly more for two: Sonnet 98.0% against 88.0% (21 tasks only B solved, 1 only A, exact
McNemar p < 0.001) and Qwen 49.0% against 42.0% (19 against 5, p = 0.007); Haiku 82.0% against 80.5%
and Llama 21.0% against 22.5% were within noise. Twenty of Sonnet's 21 losses in A were `true` and
`false` in lowercase: a lowercase `none` taught it a convention the rest of the language does not
follow — the cost of a construct that differs from Python's for no semantic reason, which is the
rule's own argument. Variant B's answers cost 0.3–7.5% more tokens ([[token-cost]]).

## Where lotml's semantics already differ

The same finding marks the places lotml reuses Python's syntax with a different meaning, and
where the design already makes the difference visible:

| construct | Python | lotml | the visible marker |
| --- | --- | --- | --- |
| `y = x` of a list or record | alias | copy | observable only through mutation, which needs `var` |
| passing an argument | reference | copy | a function changes its caller's value only through `inout`, with `&` at the call |
| `x = …` again | rebinding | an error for an immutable | `var` at the declaration |
| `if x:` on a non-`bool` | truthiness | an error | — the compiler rejects it |
| `a / b` and `//` on integers | float and floor division | the same | — adr:0007-integer-division-returns-f64 |

Each row that differs is a diagnostic with a fix ([[semantic-compiler]]); integer division kept
Python's semantics, so it needs none.

## Gaps in the original proposal

- **Unit type.** There is no way to declare a function that can fail and returns no value. The
  pilot used `-> none ! E` (A) and `-> None ! E` (B); B has the prior of `-> None`.
- **Positional fields in variants.** Haiku wrote `Negate(Expr)` and `Add(Expr, Expr)`, as in
  Rust and Swift. Accepting positional fields in variants removes the class of error.
- **`var` on a record field.** Haiku wrote `type Queue[T](var items: [T])`, as in a Swift
  `struct`. It stays invalid — mutability belongs to the local, not the field — but it calls for
  a diagnostic with the fix ready.
- **Reassignment versus shadowing.** `line = line.strip()` inside a loop is a Python habit and
  showed up in the pilot. Allowing shadowing would make it legal, but it would turn the
  accumulator `total = total + x` into a silent bug: it would create a new `total` on every
  iteration instead of summing. The right rule is to forbid reassigning immutables and suggest
  `var`.
- **How a function changes a caller's value** — see [[memory-model]].
- **`type` and Python 3.12.** In Python, `type Shape = Circle | Rect` creates an alias for
  existing types; in lotml, `type Shape = Circle(r: f64) | Rect(…)` creates new constructors.
  It is close enough to help and different enough to confuse, but the `type` statement is rare in
  the training corpus: low risk, to be documented.
- **`int` and `i64`.** The study lists `i8`…`i64` and uses `int` in its examples. The pilot fixed
  `int` as an alias for `i64`.
- **Single-line `if`.** The original example writes `if u.age < 18: fail Minor(u.age)`; a single
  canonical form calls for always a block or always a single line, and the formatter decides.

## Indentation or delimiters

| for indentation | for delimiters |
| --- | --- |
| Python's prior, and the transpiler to Python stays direct | layout can be stripped from context: 13–15% of input tokens in brace languages, 4% in Python |
| 88% of indentation slips are rejected by the parser and 2.9% are silent; a misplaced `}` is silent 7.1% | a whitespace slip cannot change a braces program, and braces plus an indentation check reject any single slip — both by construction |
| no indentation error in the pilot's 60 programs; 0–2.9% of failures in published Python studies | aider applies hunks with relative whitespace and reports 9× more editing errors without flexible patching ([docs](https://aider.chat/docs/unified-diffs.html)) |
| 36 of 36 edits passed in the editing pilot, against 35 of 36 in a braces form | a braces form invites the C family's idioms: Haiku wrote `} else if` |
| a line-oriented, depth-bounded grammar constrains it at the same per-token cost as braces | `INDENT`/`DEDENT` cannot be declared in llguidance's Lark ([[constrained-decoding]]) |

SWE-agent's edit guard, once read as an indentation guard, is a general lint gate whose failures
were never broken down by code. The evidence is in [[editing-robustness]]: the editing risk lives
in the edit interface more than in the block style, and edits addressed to syntax entities cut
edit errors by three quarters in Python. **Decision:** keep indentation
(adr:0009-significant-indentation-with-symbol-addressed-edits), with a tolerant parser, a canonical formatter and
symbol-addressed edits in the compiler; its condition was not triggered by the editing pilot,
whose twelve tasks per cell cannot separate the designs, and the harness repeats the test at scale. Switching now is cheap; after v1 it is a migration.

## Kept from the original study

- **Require `return`.** Implicit return saves a token and diverges from Python.
- **No `|>` pipeline in v1.** It would be a second way to call a function.
