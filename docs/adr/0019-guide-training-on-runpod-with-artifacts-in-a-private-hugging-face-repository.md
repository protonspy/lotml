---
status: accepted
---

# 0019 · The guide trains on RunPod, its artifacts in a private Hugging Face repository

## Context

adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server chose the guide's base
model, its runtime and where it trains: the local RTX 3060, with an on-demand RunPod GPU as the
fallback "with its own budget approved before a run". This record changes only the last of the
three. The model and the runtime stand.

What held on 2026-10-07, when the user asked for the guide's whole training pipeline on RunPod and
approved US$ 25 for it (specs/training-pipeline/):

- **The pipeline grew a stage the local card fits badly.** Reinforcement learning by group-relative
  policy optimization samples eight answers to each record and judges each with the compiler.
  At 1024 records that is some 512 optimizer steps. On a 24 GB RTX 4090 it was estimated at about
  1.5 hours. The 3060 has 12 GB, and on Windows its driver spills past the card into system
  memory, which already slowed supervised training from 17 to 30 seconds a step and got a run
  killed (adr:0017).
- **The price.** RunPod's catalog listed the RTX 4090 at about US$ 0.34 an hour on its community
  cloud. A supervised epoch and a reinforcement-learning run fit in a few dollars.
- **A pod is billed until it is gone.** A pod that outlives its run, because the machine watching
  it slept or crashed, bills in silence.
- **What must outlive a pod.** Checkpoints, adapters and models must survive a pod. The user chose
  a private Hugging Face repository over a RunPod network volume, which is billed monthly and
  bound to one data center.

## Decision

The guide trains in RunPod pods, one per run:
- Each pod is created through RunPod's REST API v2 with the standard library, an RTX 4090 on the
  community cloud by default.
- It runs the repository at one pushed commit, and its artifacts go to a private Hugging Face
  repository.
- A ledger of every pod holds runs to a cap: US$ 25 to start with.
- A run is priced at its deadline before its pod is created.
- A pod removes itself on any exit or at its deadline, and the machine that started it terminates
  it too and waits until RunPod no longer reports it.
- Validation asks the guide through `lotml guide ask` against llama.cpp's CUDA build on the pod's
  GPU.

The other options were these:

- **The local GPU alone.** It stays possible, since `guide.train` runs anywhere, but the
  reinforcement-learning stage would run for hours on a card that spills.
- **RunPod's Python SDK.** It is one more dependency for four calls, and the harness keeps its HTTP
  clients to the standard library.
- **A RunPod network volume.** The user chose a Hugging Face repository instead, for the reasons in
  the context.
- **Other GPU clouds** were not weighed. adr:0017 had named RunPod, and the user asked for it.

## Consequences

- **Keys.** Training needs `RUNPOD_API_KEY` and an `HF_TOKEN` that can write the repository. Both
  are read from the user's environment and written nowhere. The pod receives the token but not
  the RunPod key, and removes itself with its own pod-scoped credentials.
- **Committed records of every run.** `harness/results/runpod.md` and its ledger hold every pod and
  its cost. `harness/results/training/<run>.md` holds every run's report. Spend past the cap needs
  the user's approval of a new cap.
- **What a pod depends on.** The pod clones the public GitHub repository, so a run needs its
  commit pushed. It also downloads llama.cpp b11450's CUDA build and source, checked by SHA-256,
  and the base model from Hugging Face.
- **The guidance records leave the machine.** They are bench and HumanEval-derived content under
  MIT, and go to the private repository under their digest. The pipeline refuses a repository that
  is not private.
- **The threshold is calibrated on the GPU build.** The guide's users run the CPU build. Their
  confidences can differ in the last digits, so the threshold is checked again where the guide is
  served before it is relied on.
- **Self-removal is an assumption until the first run.** That a pod can remove itself with
  `runpodctl` is RunPod's documented behaviour, and the first run is where it is confirmed. Until
  then the watcher's termination and `pipeline reconcile` are what guarantee it.
