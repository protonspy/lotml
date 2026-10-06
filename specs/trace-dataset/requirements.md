---
autonomy: auto
ci: wait
---

# Trace dataset — requirements

## Purpose

The compiler-embedded model needs data that comes from the compiler: code it refused, the
diagnostics it gave, and the repairs that made it check (docs/wiki/pages/compiler-embedded-model.md).
Every agent run already leaves that in its trace. This exports it — the trajectories of runs that
passed, and the repairs inside every run — as a training set, taken only from sources whose
licence allows it and from models whose terms allow training on their outputs, each with the
evidence on record (plans/agent-data.md). Training the model is not part of it.

## R1 · What may be exported

- **R1.1** The dataset exporter shall read, for every task source and every model, the licence or terms, whether training is permitted, and the evidence, from one registry file in the repository.
- **R1.2** The dataset exporter shall export a run only when the registry permits training on both its task source and its model, each entry naming its evidence.
- **R1.3** If a run's source or model has no registry entry, or its entry names no evidence, then the dataset exporter shall leave the run out and count it under that reason.
- **R1.4** The dataset exporter shall write a manifest with each source and model exported, its licence, evidence and the notice its licence requires, and each one left out with the reason and the runs it held.

## R2 · Recording what the dataset needs

- **R2.1** When the agent calls `check`, the agent harness shall keep in the run's trace the `.lotml` files as they stood and the report `check` gave.
- **R2.2** The agent harness shall keep in the run's trace the version of the compiler that checked and graded it.

## R3 · The records

- **R3.1** When a run passed every hidden test, the dataset exporter shall write its trajectory: the task's prompt, the agent's messages with their tool calls and tool results, and the files at the end, as one chat record.
- **R3.2** When a `check` reported errors in a file and a later `check` in the same run reported none in it, the dataset exporter shall write a repair record: the file at the failing check, its diagnostics, and the file at the clean check.
- **R3.3** The dataset exporter shall write a repair record whatever the run's outcome, since a repair that checks is data in a run that failed its tests.
- **R3.4** The dataset exporter shall mark every record with its task, source, model, arm, outcome and compiler version.
- **R3.5** The dataset exporter shall write each identical repair once.
- **R3.6** If a record holds a string shaped like an API key, or the text of the task's hidden tests, then the dataset exporter shall leave the record out and count it.
- **R3.7** The dataset exporter shall write the dataset to the git-ignored cache and commit only the manifest.

## Out of scope

- Training, fine-tuning or evaluating the compiler-embedded model, which the roadmap sets aside as
  its own initiative.
- Mutating programs that check to make further repairs: the wiki names it as a source of data; it
  is a later spec if the repairs the agent makes are too few.
