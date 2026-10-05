# Token cost

An LLM reads and writes tokens, not characters. This page gathers what was measured about the
token cost of [[lotml-syntax]] and what the literature says about compressing languages.
Conclusion: syntax saves about 10% against typed Python — half the original study's target — and
in agentic use that saving weighs less than a single avoided repair round.

## How it was measured

- **Eight tokenizers, six families:** `o200k` and `cl100k` (OpenAI, via tiktoken), Llama 3,
  Qwen3, DeepSeek-V3, Gemma 3, Mistral Nemo and StarCoder2 (Hugging Face). Llama 3 and Gemma 3
  come from the `unsloth/*` mirrors, because the official repositories require accepting a
  license; the exact names and pinned commits are in `research/tokens/counter.py`.
- **Special tokens are excluded.** The Llama, Gemma and Mistral tokenizers add a BOS token by
  default; `research/tokens/test_counter.py` requires a count of zero for empty text.
- **The Claude tokenizer was not measured.** Anthropic does not publish it, and counting
  requires the `count_tokens` API with a key, which was not available. It is a gap in this
  measurement.
- **Paired corpus** in `research/tokens/corpus/`: 12 tasks (records, sum types, errors,
  generics, traits, algorithms), each in modern typed Python (3.12, `X | None`, `type`,
  dataclasses), in variant A and in variant B, always with tests. The Python side passes pytest
  and `mypy --strict`.
- **Reproduction:** `measure.py` in `research/tokens/` regenerates `results.md` and
  `results.json`; the commands, with pinned versions and revisions, are in `research/README.md`.

## Result: about 10% fewer tokens than typed Python

| tokenizer | typed Python | variant A | variant B | A / Python | B / Python |
| --- | ---: | ---: | ---: | ---: | ---: |
| o200k | 2594 | 2348 | 2368 | 0.905 | 0.913 |
| cl100k | 2605 | 2347 | 2367 | 0.901 | 0.909 |
| llama3 | 2605 | 2347 | 2367 | 0.901 | 0.909 |
| qwen3 | 2622 | 2379 | 2399 | 0.907 | 0.915 |
| deepseek-v3 | 2715 | 2420 | 2440 | 0.891 | 0.899 |
| gemma3 | 3025 | 2729 | 2748 | 0.902 | 0.908 |
| mistral-nemo | 2718 | 2445 | 2466 | 0.900 | 0.907 |
| starcoder2 | 2860 | 2599 | 2618 | 0.909 | 0.915 |

- **Lines overstate the saving.** Non-blank lines drop 28% (298 to 215), characters 15% (9340
  to 7911), tokens only 9 to 11%.
- **The saving varies a lot by task:** from 0.76 (a generic stack, where Python needs
  `field(default_factory=list)`) to 0.99 (binary search and bank transfer, which are almost all
  control flow).
- **The original study's own example** drops from 16 to 6 lines (−62%), but from 102 to 87
  tokens on `o200k` — between 12% and 16% depending on the tokenizer. "Less than half the lines"
  is true; in tokens the drop is much smaller.

## Where the saving comes from, and where it is lost

Measured construct by construct, in line context (`research/tokens/results.md`, section
*Isolated constructs*). Median difference across the eight tokenizers:

| construct | reference form | proposed form | Δ tokens |
| --- | --- | --- | ---: |
| two-field record | `@dataclass class User: …` | `type User(name: str, age: int)` | −7 |
| immutable record | `@dataclass(frozen=True) …` | `type P(x: f64, y: f64)` | −7 |
| optional field | `Optional[str]` or `str \| None` | `str?` | −1 |
| lambda | `lambda p: p.age` | `p => p.age` | −1 |
| `match` arm | `case Circle(r):` | `Circle(r):` | −1 |
| function keyword | `def` | `fn` | 0 |
| list, dict, tuple | `list[int]`, `dict[str, int]` | `[int]`, `{str: int}` | 0 |
| import | `from a.b import c, d` | `use a.b.{c, d}` | 0 |
| optional default | `y or 0` | `y ?? 0` | 0 |
| mutable | `x = 1` | `var x = 1` | +1 |
| block | indentation | braces | +1 |
| fallible signature | `-> User` (hidden exception) | `-> User ! LookupErr` | +3 |
| optional test | `if x:` | `if x is not None:` | +3 |
| empty-list test | `if xs:` | `if len(xs) > 0:` | +5.5 |

- **The saving comes from deleting declarations:** dataclasses, exception classes,
  `field(default_factory=…)`, `frozen=True`. Shortening keywords yields nothing.
- **In context, `Optional[str]` costs the same as `str | None`**, and `str?` saves one token.
  The original study's claim of "1 token instead of 3–4" does not hold.
- **The truthiness ban is the most expensive rule per occurrence.** It is still right — see
  [[type-system]] — but it has a price, which the standard library can lower with something like
  `xs.is_empty()`.
- **Making the error visible in the signature costs tokens** (+3), because Python hides the
  exception. It is the price of the information, and it is exactly the information the
  [[semantic-compiler]] uses.

## Abbreviations and symbols

| word | tokens | short form | tokens |
| --- | --- | --- | --- |
| `count`, `index`, `value`, `message` | 1 | `cnt`, `idx`, `val`, `msg` | 1 |
| `number`, `result`, `buffer` | 1 | `num`, `res`, `buf` | 1 |
| `return` | 1 | `rtn` | 1–2 |
| `->`, `lambda` | 1 | `→`, `λ` | 1 |
| `<=`, `!=`, `not` | 1 | `≤`, `≠`, `¬` | 1–2 |
| `reshape`, `reverse` | 1–2 | `⍴`, `⌽` | 2–4 |
| `outer_product` | 2–3 | `∘.×` | 4 |

Minimum–maximum ranges across the eight tokenizers, with a leading space. Abbreviations common
in code cost one token, the same as the full word; only rare glyphs (APL) cost more. The reason
to avoid abbreviations is readability and the training prior, not tokens.

## Indentation

Between 7% and 8% of the corpus's tokens are leading whitespace, in Python and in variant A
alike. StarCoder2 gives a negative value because its tokenizer merges the whitespace into the
following token, so removing indentation does not shorten the count. Replacing indentation with
braces costs one token per block: the choice between them is not a token question, and the
argument that settles it is in [[lotml-syntax]].

## What the literature says

- **SimPy** — Sun et al., [*AI Coders Are Among Us*](https://arxiv.org/abs/2404.16333), ISSTA
  2024. A Python grammar rewritten for machines (no `def`, no `:`, blocks by markers) saves 10.4%
  on the GPT-4 tokenizer, 13.5% on CodeLlama's and 8.6% on StarCoder's. The models had to be
  trained on it: training on SimPy alone dropped TinyLlama from 10.00% to 5.91% pass@1. No test
  on frontier models. Even giving up readability, the saving stays at 9–14%, the same order as
  variant A.
- **Token Sugar** — Sun et al., [arXiv 2512.08266](https://arxiv.org/abs/2512.08266), ASE 2025.
  Mined shorthands cut up to 15.1% of tokens, also requiring training.
- **Alderson** — [*Which programming languages are most token-efficient?*](https://martinalderson.com/posts/which-programming-languages-are-most-token-efficient/),
  2026, with the GPT-4 tokenizer only and "not a scientific study". Dynamic languages come out
  ahead; type inference (Haskell, F#) gets close; APL glyphs cost several tokens each.
- **Tokenmaxxing** — Wu, Anderson and Guha, [arXiv 2607.22807](https://arxiv.org/abs/2607.22807),
  2026. In agentic use, cost relative to Python was 1.28–1.69× for OCaml and 1.07–1.57× for Rust,
  driven by loops on code that does not compile and by prototyping in Python — not by surface
  verbosity.
- **Dan Luu** — [*How does programming language affect token efficiency and correctness?*](https://danluu.com/pl-tokens/).
  Token counts on trivial tasks did not predict agentic cost on realistic ones.
- **Ustynov** — [arXiv 2604.07502](https://arxiv.org/abs/2604.07502), 2026. An abbreviated log
  format cut input tokens by 17.1% and raised total session cost by 67.2% (log data, one session
  per condition): compression can move the cost into reasoning.

## Conclusions

1. **The target "≥20% fewer tokens than the equivalent Python" is not reachable through syntax**
   without giving up the training prior: variant A gives about 10% against typed Python, and
   SimPy, which gives up readability, gives 9–14%.
2. **Variant B costs 0.8 percentage points more than A.** Aligning with Python is nearly free in
   tokens; what it buys is in [[python-leakage-pilot]].
3. **Agentic cost is dominated by repair rounds.** One avoided round is worth more than the whole
   syntax saving, so the primary metric is compiling on the first try and the token target
   becomes "no worse than typed Python" — see [[requirements-and-roadmap]].
4. **Limitations.** A single author wrote the corpus knowing the hypotheses; the assumed standard
   library differs from Python's in a few places (`find`, a `pop` that returns an optional,
   `Heap`); and the choice of tasks moves the result between 0.76 and 0.99. The
   [[evaluation-harness]] must measure model-generated code, not handwritten code.
