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

**Decision:** variant B (adr:0004-python-syntax-where-semantics-match); the
[[evaluation-harness]] repeats the comparison with more models and families before v1 freezes.

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
| Python's prior, and the transpiler to Python stays direct | SWE-agent needed a guard against indentation errors (flake8 E111–E113) in edits |
| braces cost 1 token per block — irrelevant ([[token-cost]]) | aider built relative-indentation patching and flexible patching; without it, 9× more editing errors |
| no indentation error in the pilot's 60 programs, written whole | `INDENT`/`DEDENT` do not fit OpenAI's Lark subset ([[constrained-decoding]]) |

The pilot only measured programs written in one go; the risk is in **editing** existing code with
search-and-replace or diffs, which is how agents work. **Decision:** keep indentation, with a
tolerant parser and a canonical formatter, and add to the harness an editing test comparing the
indented variant with a braces variant. Switching now is cheap; after v1 it is a migration — the
decision is in adr:0005-significant-indentation.

## Kept from the original study

- **Require `return`.** Implicit return saves a token and diverges from Python.
- **No `|>` pipeline in v1.** It would be a second way to call a function.
