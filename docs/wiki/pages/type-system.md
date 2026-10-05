# Type system

The original study proposes static, strong, nominal typing, mandatory signatures and local
inference; algebraic types with exhaustive `match`; `T?` as the only form of null; `T ! E` for
errors; monomorphized generics and traits; effects and contracts as extensions. The evidence
reinforces almost all of it and adds a requirement the proposal did not have: types must be
checkable while the code is still being written.

## What the evidence says

- **Types are where the model goes wrong.** 94% of compilation errors in LLM-generated
  TypeScript are type errors ([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025).
  Static typing does not create those errors; it brings them to compile time, where the agent
  sees and fixes them — see [[semantic-compiler]].
- **Annotations alone change nothing; `Any` makes things worse.** In
  [MultiPL-E](https://arxiv.org/abs/2208.08227), removing Python's annotations made no difference
  (p=0.23), but replacing TypeScript signature types with `Any` cost 2.5 points (p<0.001). The
  original study's "no `Any` outside FFI" is right.
- **Local inference recovers the token cost.** Languages with inference (Haskell, F#) came close
  to the dynamic ones in [Alderson's](https://martinalderson.com/posts/which-programming-languages-are-most-token-efficient/)
  comparison. "Explicit at the boundary, inferred inside" is the right point, and MoonBit reached
  the same rule with mandatory signatures at module level.
- **Constraining generation by types works** — it cut 52–75% of compilation errors — but it
  requires knowing the expected type at the point where the model is ([[constrained-decoding]]).

## New requirement: types checkable on prefixes

The proposal mentions "simplified Hindley-Milner". Global inference lets a later use decide the
type of an earlier declaration, so the type of a program prefix is not determined. For the
compiler to check on every edit and for type-constrained decoding to work, inference must be
**local and bidirectional**: each declaration's type is fixed at the declaration (by its value or
its annotation), left to right, with the mandatory signatures providing context. The cost is
requiring an annotation in rare cases, such as an empty list with no immediate use
(`var xs: [int] = []`).

## Kept from the original study

- Algebraic types: one-line records and sum types; exhaustive `match`, with the compiler pointing
  at every `match` that has to change when a case is added.
- `T?` is the only type that admits absence; using a `T?` without checking it is a compile error.
- `T ! E` is sugar for `Result[T, E]`, with `?` to propagate and `fail` to fail; panics only on a
  broken invariant.
- Bracketed generics, trait bounds, monomorphization; `dyn Trait` only when asked for.
- No implicit conversions; `if` only accepts `bool` (variant A also took `T?`; variant B, adopted
  in adr:0004-python-syntax-where-semantics-match, does not).

## Adjustments

- **Truthiness banned, but cheap to avoid.** The rule prevents Python's `0`, `""` and `[]` traps,
  and costs 5 to 6 tokens per `len(xs) > 0` ([[token-cost]]). An `is_empty()` in the standard
  library lowers the cost without reopening the trap.
- **Optionals without the semantics of Python's `or`.** In variant A, `x or default` and `if x:`
  use Python's syntax with different semantics; variant B uses `??` and `is not None` — see
  [[lotml-syntax]].
- **Positional fields in variants**, such as `Neg(Expr)`, alongside named ones — the pilot showed
  it is what models write.
- **`int` is an alias for `i64`**, and integer literals are `int`.
- **A single overflow semantics.** The study proposes an error in debug and "wrap or trap,
  configurable" in release. That makes the same program behave differently across builds and
  contradicts the requirement of identical semantics across targets
  ([[transpilation-strategy]]). Recommendation: trap in every build and on every target, with
  explicit `wrapping_add` and friends for those who want modular arithmetic.
- **Equality with an error in tests** written as `== Err(E)`, not `== fail E`.

## Effects

The study recommends marking `uses io` in the signature and starting with `io` only.

- **There is no evidence that effect annotations help LLMs or tooling** — the search found no
  study.
- **Practice weighs against full typed effects.** Effekt describes effect systems as "often
  complicated and potentially hinder a wide-spread adoption" and models them as capabilities;
  Unison infers abilities as the union of those required by the functions called; OCaml 5 has
  untyped effects; Flix uses purity to optimize and parallelize.
- **Zig 0.15 dropped `async`/`await` and passes an `Io` interface as a parameter**, as it already
  did with the allocator. The "color" became an ordinary parameter — an `io` effect expressed as a
  capability, with no new feature in the type system.

Recommendation for v3: start with a capability passed as a parameter (Zig's `Io` style), inferred
inside and annotated only on public signatures, and measure in the harness whether the marking
changes anything for the model before investing in an effect system. The same mechanism serves
[[colorless-concurrency]].

## Contracts

`where` preconditions, checked in debug, stay in v3. No evidence was gathered on them.
