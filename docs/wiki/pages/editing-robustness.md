# Editing robustness

Agents change code by editing it — search-and-replace blocks, diffs, line edits — not by writing
it whole. adr:0009-significant-indentation-with-symbol-addressed-edits keeps indentation-based blocks on one condition: that
an editing test does not find the indented form failing more often than a braces form. This page
gathers what was measured about that question: what a misplaced line does to a program in each
block style, how three models fared editing both styles, and what the literature on edit formats
says. The grammar side — whether a constrainer can enforce indentation — is in
[[constrained-decoding]].

## What a slip does

`research/experiments/indentation/` takes the 40 variant B programs that parse — the paired corpus
and the pilot's B runs — and applies every single slip, one at a time: a line moved one level in
or out, a whole compound statement moved one level, and, in the braces form, a `}` moved past its
neighbouring line, dropped or doubled. Each result is parsed, checked for a function that can now
fall off its end without returning a value (a type error in lotml), and run against the program's
own tests.

| block style | slip | mutants | rejected | no change | silent, tests catch | silent, tests pass |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| indentation | one line, one level | 1,547 | 88% | 9% | 2.3% | 0.6% |
| indentation | whole statement, one level | 488 | 88% | 5% | 6.1% | 0.2% |
| braces | one line's whitespace | 1,569 | 0% | 100%, by construction | 0% | 0% |
| braces | `}` moved one line | 478 | 93% | 0.2% | 5.2% | 1.9% |
| braces | `}` dropped or doubled | 636 | 100% | 0% | 0% | 0% |
| braces + indentation check | any of the above | 2,683 | 99%+, by construction | the rest | 0% | 0% |

- **The measured rows are the indentation slips and the brace moves.** The parser rejects 88% of
  indentation slips, because a header needs a block and a dedent must land on an enclosing level;
  2.9% of them change the program silently. A `}` moved past its neighbour is silent 7.1% of the
  time. These are different slips, so the rates do not rank the designs on their own.
- **Two rows are true by construction, not measured.** The braces form is rebuilt from its braces
  alone, so a whitespace slip cannot register; and a check that requires braces and indentation to
  agree rejects any slip that moves only one of them. Neither says how often the slips occur.
- **"No change" is real, not an artifact:** indenting the only line of a block just widens the
  block, which Python also accepts.
- **The two styles fail on different slips.** Whitespace slips are harmless with braces; brace
  slips do not exist with indentation. Which design is safer therefore depends on which slip
  agents actually make — the question the editing pilot asks.
- **Redundancy removes silent slips.** Requiring braces *and* checking that indentation agrees with
  them, as GCC's `-Wmisleading-indentation` and Go's formatter effectively do, rejects every
  single slip that changes the structure. An indentation-only language has no second signal; its
  safety net is the parser's strictness, the type checker and the tests.

## The editing pilot

`research/experiments/editing/` gives twelve tasks — one per paired-corpus program, each changing
block structure: a new branch, a guard, a nested condition, a new method — to three Claude models,
once with the programs and spec in the indented form and once in a braces form (`} else {` style,
indentation not significant). Each model answers with SEARCH/REPLACE blocks, written once without
running anything; the scorer applies them, parses the result and runs the program's tests plus a
hidden test that fails on the unedited program and passes on a reference edit.

| model | indented: pass | braces: pass | median share of the program quoted in SEARCH |
| --- | ---: | ---: | --- |
| Haiku | 12 of 12 | 11 of 12 | 42% indented, 31% braces |
| Sonnet | 12 of 12 | 12 of 12 | 8% indented, 6% braces |
| Opus | 12 of 12 | 12 of 12 | 9% indented, 6% braces |

- **No edit failed to apply, in either style, strict or tolerant**: all 104 SEARCH blocks in the 72
  answers matched the file byte for byte, indentation included.
- **The one failure was a prior from braces languages**, not whitespace: in the braces form Haiku
  wrote `} else if xs[mid] < target {` — C, Java and JavaScript's spelling — where lotml has
  `elif`. A braces lotml invites the C family's idioms the way an indented lotml invites Python's.
- **Weaker models rewrite.** Haiku quoted a third to two fifths of each program, rewriting whole
  functions in eight answers; Sonnet and Opus changed only what the task needed.
- **What this does not show:** twelve short programs, one Claude family, single-shot edits with the
  file in context. Long files, multi-turn agents that edit what they no longer see, and open
  models are where whitespace errors were reported; the sample is far below the 168 paired tasks a
  decision needs ([[evaluation-harness]]).

## What agent tooling reports

- **SWE-agent's guard is a general lint gate, not an indentation guard**
  ([arXiv 2405.15793](https://arxiv.org/abs/2405.15793)). It reverts an edit that introduces any of
  eight flake8 codes — syntax errors, undefined names, duplicate arguments, unreadable files and
  three indentation codes — and the failed edits are never broken down by code, so the share due
  to indentation is unknown. Its prompts do single out indentation as the formatting hazard.
- **Edit mechanics sink about a quarter of failures**: looping on failing edits accounts for 23.4%
  of SWE-agent's unresolved instances, and an edit attempt's chance of eventually succeeding falls
  from 90.5% to 57.2% after one failed edit.
- **The guard has costs a compiler must avoid.** It rejects legitimate intermediate states (a
  header changed before its uses) and assumes the file was clean before the edit; a check-on-edit
  for lotml must diff diagnostics against the pre-edit state and report only what the edit
  introduced.
- **The lint ablation is within noise**: six runs of the default configuration ranged from 17.33%
  to 18.67%, and the 3-point ablation is a single run.
- **aider** applies hunks with relative leading whitespace and flexible patching, and reports a 9×
  increase in editing errors on its Exercism benchmark when flexible patching is disabled
  ([aider's documentation](https://aider.chat/docs/unified-diffs.html); not a paper, not machine-checked). The pilot's tolerant mode
  implements the same relative matching and was never needed.

## What the edit-format literature says

- **The interface decides more than the block style.** On SWE-bench Verified — all Python —
  [CODESTRUCT](https://arxiv.org/abs/2604.05407) replaced string-replacement edits with edits
  addressed to named syntax entities: the tool takes the indentation from the target node,
  re-indents the replacement itself and rejects any edit that leaves a syntax error. Edit errors
  per task fell by 76–88% for GPT-5, GPT-5-mini and Qwen3-Coder-480B, and GPT-5-nano gained 20.8
  points as runs with no valid patch fell from 46.6% to 7.2%. With text edits, the authors report
  failures dominated by exact-match and whitespace mismatches (their own log analysis is not fully
  consistent with that). Weaker models made more errors with the structured tools, which also need
  a file that parses.
- **Line numbers fail; whole units work.** Diffs that locate edits by line number score far below
  rewriting the code, and content-anchored line fragments still trail; diffs that rewrite whole
  syntactic units — a block, a function — were the most accurate on every base model tested
  ([*To Diff or Not to Diff?*](https://arxiv.org/abs/2604.27296)). Models shown a new edit format
  with one example fell back to unified diffs: an edit format outside the training prior is not
  learned from the prompt.
- **Search-and-replace is the best text format for large models**
  ([Diff-XYZ](https://arxiv.org/abs/2510.12487)): each replacement stands alone, with no line
  numbers or hunk counts to get wrong. Generating a well-formed diff takes far more capacity than
  applying one.
- **Whitespace errors are rare when programs are written whole.** Across nine models on Python
  benchmarks, incorrect indentation caused 0.0% of failures in almost every cell, and 0–2.9% of
  real-world tasks ([arXiv 2407.06153](https://arxiv.org/abs/2407.06153)) — consistent with the
  pilot's 60 programs. An indentation slip that still parses would be counted as a functional bug
  there, so these are lower bounds.
- **Braces buy one thing indentation cannot: strippable layout.** With explicit delimiters, the
  layout that does not change the program can be removed from code given to the model as context —
  13.2–14.7% of tokens in Java, C# and C++, against 4.0% in Python — with no significant loss in
  fill-in-the-middle tests; models keep writing formatted code, so the saving is on input only
  ([arXiv 2508.13666](https://arxiv.org/abs/2508.13666)). Asking a model to emit minified Python broke
  it (GPT-4o's pass@1 from 71.5% to 59.7%).

## What follows for lotml

1. **Keep significant indentation** (adr:0009-significant-indentation-with-symbol-addressed-edits): nothing measured argues for
   switching. The pilot did not trigger its condition, though twelve tasks per cell cannot separate
   the designs; a misplaced brace is silent more often than a misplaced line; and which slips agents
   make is unmeasured. The decision stays conditional on the editing test at scale.
2. **The compiler offers edits addressed to symbols**: replace the body of a function, a branch of
   a `match`, a method of an `impl`, with the indentation taken from the target and any edit that
   breaks the syntax rejected — the interface that cut edit errors by three quarters in Python.
   Text edits stay available for everything else, matching whole lines and re-indenting
   relatively; the parser is tolerant, so structured edits work on a file that does not parse;
   and the formatter is canonical, so whitespace never carries information a tool could lose.
3. **Check on every edit, against the pre-edit state**, with the error, the attempted edit and the
   original text in the response, and an explicit "no errors" when there are none.
4. **Diagnose the neighbours' idioms**: `else if` and `{`-blocks from the C family as well as
   Python's habits, each with the fix ([[semantic-compiler]]).
