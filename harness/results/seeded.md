# Seeded failures

Written by `python -m lotml_harness.guide.seeded` on 2026-10-07 with `lotml 0.1.0`; the
records are in `harness/cache/guide/seeded/2026-10-07/`, git-ignored.

## Programs

| source | programs | left out |
|---|---:|---|
| bench | 8 | — |
| humaneval-original | 77 | literal 4, no-check 30, rules 22 |

| split | records |
|---|---:|
| train | 2285 |
| validation | 402 |

## Weights

From harness/results/phase1.md: each family's count, and its part of a
program's 60 mutants before a short family passes its share on.

| family | count | per program |
|---|---:|---:|
| names | 66 | 20 |
| types | 32 | 10 |
| calls | 4 | 1 |
| mutability | 43 | 13 |
| meaning | 50 | 16 |

Codes no operator aims at: E0001 1, E0003 92, E0101 2, E0110 3, E0111 11, E0206 2, E0216 2.

## Mutants by operator

| family | operator | listed | drawn | check | test | equivalent | fixable | timeout | no report | refused |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| names | misspell-field | 118 | 105 | 105 | 0 | 0 | 0 | 0 | 0 | 0 |
| names | misspell-name | 952 | 846 | 835 | 11 | 0 | 0 | 0 | 0 | 0 |
| types | add-optional | 144 | 119 | 119 | 0 | 0 | 0 | 0 | 0 | 0 |
| types | drop-optional | 6 | 4 | 4 | 0 | 0 | 0 | 0 | 0 | 0 |
| types | int-f64 | 158 | 138 | 134 | 0 | 0 | 4 | 0 | 0 | 0 |
| types | unwrap-list | 85 | 78 | 78 | 0 | 0 | 0 | 0 | 0 | 0 |
| types | wrap-list | 271 | 231 | 231 | 0 | 0 | 0 | 0 | 0 | 0 |
| calls | drop-argument | 289 | 203 | 150 | 47 | 6 | 0 | 0 | 0 | 0 |
| calls | drop-try | 4 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| calls | duplicate-argument | 244 | 161 | 120 | 35 | 6 | 0 | 0 | 0 | 0 |
| mutability | drop-var | 64 | 64 | 13 | 0 | 0 | 51 | 0 | 0 | 0 |
| meaning | change-constant | 270 | 258 | 0 | 231 | 26 | 0 | 1 | 0 | 0 |
| meaning | drop-statement | 196 | 188 | 1 | 82 | 102 | 0 | 3 | 0 | 0 |
| meaning | move-bound | 62 | 56 | 0 | 38 | 18 | 0 | 0 | 0 | 0 |
| meaning | negate-condition | 110 | 97 | 1 | 95 | 0 | 0 | 1 | 0 | 0 |
| meaning | swap-arguments | 29 | 28 | 0 | 22 | 6 | 0 | 0 | 0 | 0 |
| meaning | swap-arithmetic | 83 | 71 | 0 | 66 | 5 | 0 | 0 | 0 | 0 |
| meaning | swap-comparison | 138 | 124 | 0 | 102 | 19 | 0 | 3 | 0 | 0 |
| meaning | swap-logical | 21 | 20 | 0 | 19 | 1 | 0 | 0 | 0 | 0 |

Kept 2687 of 2951 drawn: 1926 refused by `check`, 761 failing a test.
