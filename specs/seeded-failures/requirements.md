---
autonomy: auto
ci: wait
branch: feat/guide-training
delivery: merged
pr: 18
---

# Seeded failures — requirements

## Purpose

The agent's real repairs will be few: a capable model writes code that checks the first time, and
the trace dataset's own risks say so (specs/trace-dataset/). The study behind the harness guide
found where more come from — programs that compile, mutated the ways programs break: DrRepair
pre-trained on mutants of 310K compiling programs, with corruptions weighted by compiler-error
statistics repairing 62.5% of DeepFix against 49.4% for random dropout, and HDLdebugger built its
triples the same way (docs/wiki/pages/compiler-embedded-model.md). It also found what they cannot
do: a model that learned only mutants learned the mutator, and seeded bugs train a detector, not a
reviewer. So this makes failures to train on — lotml programs that check and pass, mutated, each
with the program as its known fix — and the guide is judged on real failures only
(plans/harness-guide.md).

## R1 · The programs

- **R1.1** The seeded failure generator shall take as programs only files that check and pass their tests, from the train and validation problems of the agent benchmark's solutions with their hidden blocks, the original HumanEval's canonical solutions translated to lotml, and the files at the end of the trajectories the trace dataset exported.
- **R1.2** The seeded failure generator shall translate a HumanEval canonical solution with the corpus pipeline's rules alone, its signature typed from its recorded cases as specs/agent-humaneval/ types its hidden blocks, and keep it only when it checks and passes those blocks.
- **R1.3** If the rules cannot translate a canonical solution, then the seeded failure generator shall leave the problem out and count it by reason.

## R2 · The mutants

- **R2.1** The mutation command shall list, for a `.lotml` file, every mutant each operator makes, with its operator, the declaration it falls in and the mutated text.
- **R2.2** The mutation command shall hold operators that break what agents break — a name misspelt, a type annotation changed, an argument dropped or added, a `?` dropped from a fallible call, an immutable binding reassigned or mutated, a field or method misnamed — and operators that keep a program checking but change its meaning — a comparison or arithmetic operator swapped, a bound moved by one, two arguments swapped, a condition negated, a constant changed, a statement dropped.
- **R2.3** The seeded failure generator shall keep a mutant only when `check` refuses it or one of its program's tests then fails.
- **R2.4** If `check --fix` makes a mutant check clean, then the seeded failure generator shall drop it and count it.
- **R2.5** The seeded failure generator shall give each program a fixed budget of mutants, shared among the families in proportion to how often agents' failures fall in each family, from the counts the harness's committed reports give, with a family's unused share passed to the others, and name those counts in its report.
- **R2.6** The seeded failure generator shall draw within a family uniformly over its operators' mutants, and keep each identical mutant once per problem.
- **R2.8** The seeded failure generator shall also make mutants that join two drawn mutants of one program in different declarations, judged as one mutant, so the fix spans more than one declaration (specs/training-pipeline/, adr:0020-the-guide-trains-by-fine-tuning-then-rejection-sampling-and-rl-on-what-it-sometimes-solves).
- **R2.7** The seeded failure generator shall judge each mutant in a scratch copy written through the agent harness's safe layer, running every `lotml` call in the harness's clean environment, with its files after `--`, a memory cap and a deadline that ends its process tree.

## R3 · The output

- **R3.1** The seeded failure generator shall write each kept mutant as a repair record in the trace dataset's shape — the mutant as the failing file, its diagnostics or its failing block with the values each side had, the program as the fixed file — marked `origin: seeded` with its operator, problem and split.
- **R3.2** If a program's problem is held out, then the seeded failure generator shall refuse it.
- **R3.3** The seeded failure generator shall write the records to the git-ignored cache with the notices their sources require, and commit `harness/results/seeded.md` with the programs taken by source, the mutants made, kept and dropped by operator and reason, and the counts that weighted the draw.

## Out of scope

- Errors `check --fix` repairs: a rule states them, and the plan keeps them out of guidance.
- A learned breaker, as Break-It-Fix-It trains: hand-written operators first; a breaker is a
  later spec if the evaluation shows the guide learned the mutator.
- Programs from the Python→lotml corpus: it is translated from MultiPL-E's typed originals, which
  may not be trained on (harness/results/NOTICE.md).
