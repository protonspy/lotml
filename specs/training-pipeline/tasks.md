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
- [ ] 3.2 (Unit) Upload checkpoints as they are saved and resume a stage from its run's latest — R2.3, R3.2
  _Depends 2.3_
- [ ] 3.3 (Unit) Train by group-relative policy optimization on the fine-tuned model with the reward — R3.3
  _Depends 1.3, 3.2_
- [ ] 3.4 (Unit) Export the GGUF file, ask the validation split through llama-server, calibrate the threshold, and write the run's report — R3.4, R5.1
  _Depends 3.2_
- [ ] 3.5 (Unit) Run the pipeline from the command line: build and upload the records, price and create the pod, watch it, terminate it, record the ledger, and commit the report — R1.4, R2.5, R3.1, R3.5, R5.1, R5.2
  _Depends 2.2, 2.3, 3.1, 3.3, 3.4_
- [ ] 4.1 (Unit) Record the move to RunPod and Hugging Face in an ADR and in the stack — R1.1, R2.1
- [ ] 4.2 (Unit) Run the pipeline on RunPod — SFT, reinforcement learning and export of the 0.5B model — within the cap, and commit its report and ledger — R1.3, R5.1
  _Depends 3.5, 4.1_
