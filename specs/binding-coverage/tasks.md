# Binding coverage — tasks

## 1 · The corpus, the count and the report

- [x] 1.1 (Unit) Name the corpus in `harness/binding-corpus.json` and pin its stub distributions as the harness group `stubs` — R1.1, R1.2
- [x] 1.2 (Unit) Find each module's stub as PEP 561 orders them, and count its public names and those `lotml bind` binds, a module without a stub at 0 — R1.2, R1.3, R2.1, R2.2
  _Depends 1.1_
- [x] 1.3 (Unit) Keep each measurement under a label, write the report, and record the binder before and after the aliases of plans/python-compatibility.md 1.1 — R3.1, R3.2
  _Depends 1.2_
