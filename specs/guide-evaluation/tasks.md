# Guide evaluation — tasks

## 1 · The failures

- [ ] 1.1 (TDD) Collect held-out real failures from agent traces with the trace dataset's extraction, from any model, and from the phase 1 gate's conversations, each once, refusing a problem that is not held out — R1.1, R1.2
- [ ] 1.2 (TDD) Refuse to run, failing closed, when the guide's training records are missing, unreadable, lack a problem or split, or hold a problem the split derives as held out, and keep their digest for the report — R1.3

## 2 · Asking and scoring

- [ ] 2.1 (Unit) Lay each failure in a scratch copy through the safe layer and ask the guide through `lotml guide ask`, every `lotml` call in a clean environment with files after `--`, a memory cap and a deadline that ends its process tree, recording the answer, its latency and the guide server's peak memory — R2.1, R2.5, R2.8
  _Depends 1.1, 1.2_
- [ ] 2.2 (TDD) Score top-1 and top-3 location against the declarations the fix changed, the compiler's baseline the same way, silence, precision, and the edits shown and withheld by reason over those proposed — R2.2, R2.3, R2.4
  _Depends 2.1_
- [ ] 2.3 (Unit) Write rows of symbols, scores and reason codes through the scrub, and the report with the records' digest, every score by kind of failure and failing model with its Wilson interval, latency percentiles, the CPU, the model file and its quantization, and no verdict below the minimum — R1.4, R2.5, R2.6, R2.7
  _Depends 2.2_

## 3 · Enough failures

- [ ] 3.1 (Unit) Run held-out problems on cheap models through the agent harness until the failures collected reach 97, and commit their rows — R1.4
  _Depends 1.1_
