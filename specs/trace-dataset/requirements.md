---
autonomy: auto
ci: wait
branch: feat/guide-training
delivery: merged
pr: 18
---

# Trace dataset — requirements

## Purpose

The compiler-embedded model needs data that comes from the compiler: code it refused, the
diagnostics it gave, and the repairs that made it check (docs/wiki/pages/compiler-embedded-model.md).
Every agent run already leaves that in its trace. This exports it — the trajectories of runs that
passed, and the repairs inside every run — as a training set, taken only from sources whose
licence allows it and from models and providers whose terms allow training on their outputs, each
with the evidence on record (plans/agent-data.md). Every record carries its problem's split, fixed
once for every source that derives from the problem, so the harness guide is never scored on a
problem it was trained on (plans/harness-guide.md). Training the model is not part of it.

## R1 · What may be exported

- **R1.1** The dataset exporter shall read, for every task source, every model and every provider serving it, the licence or terms, whether training is permitted, the evidence, the date it was checked and the notice required, from one registry file in the repository.
- **R1.2** The dataset exporter shall export a run only when the registry permits training on its task source, its model and every provider that served it.
- **R1.3** If a run's source, model or a provider has no registry entry, a row names no provider, or a field is missing or empty, then the dataset exporter shall leave the run out and count it under that reason.
- **R1.4** The dataset exporter shall treat an entry as permitting training only when its `training` is exactly `permitted`, its evidence is a repository path that exists or an `https://` URL, and it names the date checked and the notice.
- **R1.5** If the registry cannot be parsed, or holds an unknown table, an unknown field or a duplicate entry, then the dataset exporter shall export nothing and say why.
- **R1.6** The dataset exporter shall take a run's task, source, model, arm and providers from the row inside its trace, derive the source again from the task id, and leave out a run where the two disagree or the trace's path does not match the row.
- **R1.7** The dataset exporter shall write a manifest with each source, model and provider exported, its licence, evidence and notice, and each one left out with the reason and the runs it held.

## R2 · Recording what the dataset needs

- **R2.1** When the agent calls `check`, the agent harness shall keep in the run's trace the `.lotml` files as they stood before and after the call, symbolic links skipped, and the report `check` gave.
- **R2.2** The agent harness shall keep in the run's trace the version of the compiler that checked and graded it.
- **R2.3** When the agent harness writes a row or a report under `harness/results/`, the agent harness shall first replace every key-shaped string and the value, eight characters or longer, of every environment variable whose name ends in `_KEY`, `_TOKEN` or `_SECRET` with a placeholder.
- **R2.4** The agent harness shall keep in the run's trace the system prompt and memory as the main agent's model received them and the schemas of the tools it was offered.
- **R2.5** When the agent calls `test`, the agent harness shall keep in the run's trace the `.lotml` files as they stood before and after the call, symbolic links skipped, its arguments, its status and the report `test` gave, with the user's home directory and name in it replaced by placeholders.

## R3 · The records

- **R3.1** When a run passed every hidden test, the dataset exporter shall write its trajectory: the system prompt and memory, the tools offered, the task's prompt, the main agent's messages with their tool calls and tool results, and the files at the end, as one chat record.
- **R3.2** When a `check` reported errors in a file and a later `check` in the same run judged that file and reported none in it, the dataset exporter shall write a repair record: the task's prompt, the file at the failing check, its diagnostics, the file at the clean check, and the lines the repair changed.
- **R3.3** The dataset exporter shall write a repair record whatever the run's outcome, since a repair that checks is data in a run that failed its tests.
- **R3.4** If a `check` or a `test` ended in a tool error, gave a report that is not JSON, or saw its files change between before and after, then the dataset exporter shall use that call neither to open nor to close a repair.
- **R3.5** The dataset exporter shall mark every record with its task, source, model, arm, outcome and compiler version, and a repair with the origin `real`.
- **R3.6** The dataset exporter shall write each identical repair once per task, so a repair repeated across tasks counts in each.
- **R3.7** If a record holds a key-shaped string, the value of an environment variable as R2.3 names them, or an `assert` line of the task's hidden tests that does not also appear in the task's prompt, then the dataset exporter shall leave the record out and count it by reason in the manifest.
- **R3.8** The dataset exporter shall write the dataset to the git-ignored cache with the notices and a copy of the manifest beside it, and commit only the manifest.
- **R3.9** When a `test` ran the blocks and reported one failing, and a later `test` in the same run reported that block passing, the dataset exporter shall write a repair record: the task's prompt, the file at the failing test, the failing block with the values each side of its comparison had, the file at the passing test, and the lines the repair changed.

## R4 · The split

- **R4.1** The problem split shall name every problem by its original benchmark's id — `humaneval/<n>`, `mbpp/<n>`, `bench/<task>` — mapping to it the ids that derive from it: the task set's own `humaneval/<n>` and `mbpp/<n>`, the agent harness's `humaneval-<n>` and `mbpp-<n>`, and MultiPL-E's `HumanEval_<n>_<name>` and `mbpp_<n>_<name>`.
- **R4.2** The problem split shall assign each HumanEval problem to train, validation or held-out from the SHA-256 of a fixed salt and the problem's id, in the proportions 60, 15 and 25.
- **R4.3** The problem split shall assign every MBPP problem to held-out and every problem of the agent benchmark to train.
- **R4.4** If an id matches none of the forms R4.1 names, then the problem split shall refuse it.
- **R4.5** The dataset exporter shall mark every record with its problem and split, write train and validation records to separate files, and write no held-out record.
- **R4.6** The dataset exporter shall list in the manifest, per source and split, the problems, trajectories and repairs exported.

## Out of scope

- Training, fine-tuning or evaluating the compiler-embedded model, which the roadmap sets aside as
  its own initiative.
- Mutating programs that check to make further repairs: the wiki names it as a source of data; it
  is a later spec if the repairs the agent makes are too few.
