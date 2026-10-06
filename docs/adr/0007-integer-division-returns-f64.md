---
status: accepted
---

# 0007 · Integer division returns `f64`

## Context

The pilot spec lists `/` and `//` without saying what `/` does on two `int`s. Python 3 returns a
float; C, Java, Go and Rust truncate. R34 rules out the one answer that is silently wrong — a
truncating `/` — and leaves two: return `f64` as Python does, or reject `/` on integers with a
diagnostic pointing at `//`. The general rule (adr:0004-python-syntax-where-semantics-match) is
that semantics equal to Python's use Python's syntax and different semantics use visibly different
syntax; models lost 40–70 points when familiar operators carried new meanings ([[lotml-syntax]]).
Every program, test and corpus written after v1 depends on what `/` means, so changing it later is
a migration of all existing code. See [[type-system]].

## Decision

`a / b` on two integers returns `f64`, exactly as in Python 3: both operands are converted to
`f64` and divided, so `7 / 2 == 3.5` and `1 / 0` traps like every other division by zero. `a // b`
on integers is floor division returning `int`, and `a % b` takes the sign of the divisor, both as in
Python. `/` is the only operator that converts an integer to a float; every other mixing of `int`
and `f64` is a type error with a fix. Rejected: an error pointing at `//`, which turns valid Python
— the code the training prior writes — into a compile round for no semantic gain; and truncation,
which R34 forbids.

## Consequences

- The model's prior is right without instruction: `/` means what it means in Python, and the
  transpiler to Python passes it through.
- A static language with one implicit `int` → `f64` conversion: the type checker special-cases `/`,
  and the C target emits a conversion before dividing.
- Integers above 2^53 lose precision under `/`, as in Python; code that needs an exact quotient
  uses `//`.
- Floor division and a divisor-signed `%` cost a branch on the C target, where the hardware
  truncates; the parity suite (R18) checks negative operands on both targets.
