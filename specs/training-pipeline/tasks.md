# Training pipeline — tasks

- [x] 1.1 (Unit) Add `lotml dev judge`: read an answer with the guide tool's reader, make its edit with the gate, check it, and run a failing block in a scratch directory — R4.2, R4.4
- [x] 1.2 (Unit) Keep each guidance record's state beside its messages, as a delta to specs/guide-records/ — R4.2
- [x] 1.3 (TDD) Score answers through the judge: zero outside the schema or on a failed judgment, else the mean of the location and edit parts — R4.1, R4.2, R4.3, R4.4
  _Depends 1.1, 1.2_
- [x] 2.1 (Unit) Speak RunPod's REST API: price from the catalog, create, status, terminate and confirm gone, the key from the environment — R1.1, R1.4
- [x] 2.2 (TDD) Hold runs to the cap: estimate from price and deadline, refuse past the cap, record every pod in the ledger and reconcile an open row — R1.2, R1.3
  _Depends 2.1_
- [x] 2.3 (Unit) Keep artifacts in a private Hugging Face repository: refuse a public one, put and get, records by digest, a run's lineage — R2.1, R2.2, R2.4
- [x] 3.1 (Unit) Bootstrap a pod at a commit and terminate it from inside at the deadline or on exit — R1.5, R1.6
- [x] 3.2 (Unit) Upload checkpoints as they are saved and resume a stage from its run's latest — R2.3, R3.2
  _Depends 2.3_
- [x] 3.3 (Unit) Train by group-relative policy optimization on the fine-tuned model with the reward — R3.3
  _Depends 1.3, 3.2_
- [x] 3.4 (Unit) Export the GGUF file, ask the validation split through llama-server, calibrate the threshold, and write the run's report — R3.4, R5.1
  _Depends 3.2_
- [x] 3.5 (Unit) Run the pipeline from the command line: build and upload the records, price and create the pod, watch it, terminate it, record the ledger, and commit the report — R1.4, R2.5, R3.1, R3.5, R5.1, R5.2
  _Depends 2.2, 2.3, 3.1, 3.3, 3.4_
- [x] 4.1 (Unit) Record the move to RunPod and Hugging Face in an ADR and in the stack — R1.1, R2.1
- [ ] 4.2 (Unit) Run the pipeline on RunPod — SFT, reinforcement learning and export of the 0.5B model — within the cap, and commit its report and ledger — R1.3, R5.1
  _Depends 3.5, 4.1_
- [ ] 5.1 (TDD) Score an answer's locations by the F-score with beta 3 of the declarations it names, zero when one is not declared in the file, the judge saying which are not — R4.2
- [ ] 5.2 (Unit) Sample several answers to each train record from the fine-tuned guide and judge each — R3.6
- [ ] 5.3 (Unit) Fine-tune the guide further on the sampled answers that pass, with the record's target where none did — R3.7
  _Depends 5.2_
- [ ] 5.4 (Unit) Build the reinforcement-learning pool from the records sometimes solved, with a share of those always solved — R3.8
  _Depends 5.2_
- [ ] 5.5 (Unit) Train by group-relative policy optimization on the pool with adr:0020's settings, keep the best checkpoint on a validation sample, and offer a random-reward twin — R3.3, R3.9
  _Depends 5.1, 5.4_
- [ ] 5.6 (Unit) Report pass@1, pass@4 and pass@8 sampled on the validation split, the pool's size and the share of groups that scored alike — R5.1
  _Depends 5.2_
- [ ] 5.7 (Unit) Run the recipe on RunPod beside its random-reward twin and commit their reports — R5.1
  _Depends 5.3, 5.5, 5.6_
