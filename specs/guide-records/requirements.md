---
autonomy: auto
ci: wait
branch: feat/guide-tool
delivery: merged
pr: 20
---

# Guide records — requirements

## Purpose

The harness guide learns from pairs of a failing state and the change that got out of it
(plans/harness-guide.md). The trace dataset's repairs hold the agent's real ones — refused by
`check`, or failing a test in code that checks (specs/trace-dataset/) — and the seeded failures hold
mutants with their known fix (specs/seeded-failures/). This turns both into guidance records: the
state the guide's tool will show the model, and the location, kind of change and edit it should
answer with, in the shape a trainer reads. The study behind it found the line a compiler reports is
often not the one to fix (DrRepair, docs/wiki/pages/compiler-embedded-model.md), so the target is
taken from the fix, never from the diagnostic.

## R1 · Where a change falls

- **R1.1** The diff command shall report, for two versions of a `.lotml` file, every declaration of the first version whose lines changed, innermost first, and for a declaration only the second version has, the declaration of the first it follows, or the file's start when none does, each with its symbol as the MCP tools name it, its kind and its line span in the first version.
- **R1.2** The diff command shall report the one symbol-addressed edit that turns the first version into the second, trying in order a `match` arm, a body and a definition of the innermost changed declaration, an added declaration and a removed one, and last an `edit` of whole lines, each written as the arguments of the MCP tool that applies it, the file's path among them.
- **R1.3** The diff command shall report an edit only when applying it to the first version through the compiler's own edit functions gives the second, byte for byte.
- **R1.4** If no single edit reproduces the change, then the diff command shall report the changed declarations and no edit.

## R2 · The records

- **R2.1** The guide records builder shall write one guidance record per repair the trace dataset exported and per seeded failure, with the task's prompt, the failing file, and the diagnostics or the failing test block with the values each side of its comparison had, as the state, and the changed declarations, the kind of change and the edit the diff command reported as the target.
- **R2.2** The guide records builder shall name the kind of change after the edit — `arm`, `body`, `definition`, `add`, `remove` or `lines` — and `several` when no single edit reproduces it.
- **R2.3** The guide records builder shall write each record as a chat example: the guide's system message and the state as the guide's tool renders them (specs/guide-tool/), then the target as the assistant's answer in the JSON the tool's schema accepts.
- **R2.4** Where a record's task is known, the guide records builder shall write it both with the task and without it, in the same split.
- **R2.5** The guide records builder shall mark every record with its problem, split, origin — `real` or `seeded` — source, model, compiler version and renderer version, write train and validation to separate files, and refuse a held-out record.
- **R2.6** If a record's rendered messages and the answer budget together exceed the guide's context, counted with the base model's tokenizer, then the guide records builder shall leave it out and count it.
- **R2.7** The guide records builder shall write the records to the git-ignored cache and commit `harness/results/guide-records.md` with the records by origin, split, kind and source, and every record left out by reason.
- **R2.8** The guide records builder shall keep with each record the state it was rendered from, so an answer can be judged against the raw file (specs/training-pipeline/ R4.2).

## Out of scope

- Training on the records (plans/harness-guide.md, 2.2) and choosing the base model (2.1).
- Records from held-out problems: the guide's evaluation reads those from the traces itself
  (specs/guide-evaluation/).
