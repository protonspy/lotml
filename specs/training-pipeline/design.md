# Training pipeline — design

Where training runs changes, and what it runs grows by one stage. adr:0019-guide-training-on-runpod-with-artifacts-in-a-private-hugging-face-repository
records the move; the base model, the runtime and the records' format stay as adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server
set them.

## The run, end to end

```
local                                   RunPod pod                         Hugging Face (private)
─────                                   ──────────                         ──────────────────────
build records ── upload ──────────────────────────────────────────────▶  records/<digest>/
price run, check cap
create pod ─────────────────────────▶   bootstrap at <commit>
                                        sft ──── checkpoints ─────────▶  runs/<run>/sft/
                                        rl  ──── checkpoints ─────────▶  runs/<run>/rl/
                                        export + calibrate + validate ▶  runs/<run>/{gguf,report.json}
                                        status.json at every step ────▶  runs/<run>/status.json
watch status and pod ◀─────────────
terminate pod, confirm gone
ledger + report ── commit
download GGUF
```

`python -m lotml_harness.guide.pipeline run --stages sft,rl,export --gpu "NVIDIA GeForce RTX 4090"
--hours 3` is the whole interface. `--gpu` takes GPU types, comma separated, tried in order until RunPod has
a card — it often has none of one type — and the cap is checked at the dearest of them. `--from
<run>` names the run whose outputs the first stage reads (R3.5); `--dry-run` prints the estimate
and the pod's request without creating it.

## Pods (R1)

`guide/runpod.py` speaks RunPod's REST API v2 (`https://api.runpod.io/v2`) through `urllib`, as the
harness already speaks OpenRouter, following no redirect, which would carry the key with it: `price(gpu, cloud)` reads `GET /catalog/gpus/{id}`,
`create(request)` posts `/pods`, `status(id)` gets `/pods/{id}`, `terminate(id)` posts
`/pods/{id}/action` with `terminate` and then polls until `GET` answers 404. Errors arrive in RFC
9457 form; the `detail` is raised, never the request, since the request carries the token.

**The cap** (R1.2, R1.3) is held by a ledger in the user's home, `~/.lotml/runpod.jsonl`, one
row per pod, changed under a lock file: outside every checkout, so a branch, a worktree or a reset
cannot lower what was spent. Its committed copy, `harness/results/runpod.jsonl`, and
`harness/results/runpod.md`, built from it, are the record; the spend counts the rows of both. Spent is the sum of the rows' costs; a row's cost is
the price times the time from creation to the confirmed termination, rounded up to the minute. The
estimate uses the deadline, not a guess at the duration, so a run that hangs still fits. The cap
is 25.00 USD, the amount the user approved on 2026-10-07, and no flag raises it; the estimate
covers the deadline, the watcher's ten minutes of grace and the five a termination may wait, and
a deadline is more than zero and at most twelve hours. As soon as RunPod
returns the pod's id a row is written with `ended: null`, and the watcher completes it; a row left
open by a crash is counted at its full deadline until `pipeline reconcile` closes it — a pod still
there terminated and closed now, a pod gone closed at its deadline or now, whichever is sooner.

**The deadline** (R1.4, R1.5) is enforced twice. The watcher terminates the pod when the run's
`status.json` says `done` or `failed`, or when the deadline passes. Inside the pod, the bootstrap
runs the stages under `timeout`, and on any exit calls `runpodctl remove pod "$RUNPOD_POD_ID"`,
which the pod's own environment authorizes; so a laptop that sleeps does not leave a pod billing.
Before it does, it uploads the run's output, the token replaced, to `runs/<run>/pod.log`: a
failure before the stages leaves no status, and a failed stage also leaves
`runs/<run>/diagnostics.json`, the GPU's setup.

**The pod** is `runpod/pytorch` at a pinned tag with CUDA 12.8, community cloud first, 40 GB of
container disk, no volume (R2 holds what must outlive it), no ports. Its environment carries
`HF_TOKEN`, the run's id, the commit, the stages and the deadline; the RunPod key is not passed in.
The clone of the public repository at the commit (R1.6) runs under the deadline with the rest. The
bootstrap holds the token aside while it installs uv (a pinned version) and rustup over https only
and builds the release `lotml`, then syncs the environment on uv's own CPython 3.13.14 — the image's
3.13 failed torch's import — `uv sync --locked --group train`, and fetches llama.cpp b11450's Linux
release and `convert_hf_to_gguf.py` from its source archive.

## Artifacts (R2)

`guide/hub.py` wraps `huggingface_hub`: `ensure_private(repo)` creates the repository private or
refuses one that is public (R2.2); `put(local, remote)` and `get(remote, local)`. Records are
uploaded once per digest to `records/<sha256>/`; a run writes under `runs/<run>/`, and its
`run.json` names the records' digest and, per stage, the run it read from (R2.4). Checkpoints are
uploaded by a `TrainerCallback` on each save, and a stage that finds `runs/<run>/<stage>/checkpoint-*`
downloads the latest and passes it to `trainer.train(resume_from_checkpoint=…)` (R2.3). Before any
stage trains, `train.load` checks every record against the split, as it does today (R2.5).

## Stages (R3)

- **records** (local): `guide.seeded` and `guide.records` as today. Each record now also keeps
  the `state` it was rendered from — the path, the failing text, the diagnostics or the failing
  block — because the reward judges the raw file, not its rendering (a delta to
  specs/guide-records/).
- **sft**: `guide.train.train`, unchanged in its settings, with the checkpoint callback.
- **rl**: `guide.grpo`, TRL's `GRPOTrainer` on the SFT adapter merged into the base model, a fresh
  LoRA adapter of the same shape, `num_generations` 8, `max_completion_length` 1024 (the tool's
  answer budget), `beta` 0.04, learning rate 1e-6, one epoch over the train split. Its dataset is
  the records' prompt — the system and user messages — with the `state` and the target's symbols as
  columns the reward reads. Generation uses transformers, not vLLM: one 24 GB card holds the 0.5B
  model eight answers wide, and vLLM would be a second runtime to pin.
- **export**: `train.merge` and `train.export`, then `llama-server` on the pod's CPU serves the GGUF
  file and the validation split is asked as the `guide` tool asks — the schema in its trained key
  order and `top_logprobs: 1` (plans/guide-request.md). The first location's confidence and
  whether it is right go to `calibrate.threshold` for a target precision of 0.9; the threshold, the
  precision it reached and the metrics of R5.1 go to `report.json`. The same validation run is made
  on the SFT model first, so the report has before and after.

## The reward (R4)

`lotml dev judge <file> --answer <answer.json> --path <path> [--failing <name>]` is a hidden
command beside `dev diff`. It reads the answer with the `guide` tool's own reader — the same
closed fields and limits — makes the edit with `gate::applied`, checks the result in memory, and
for a failing block runs it in a scratch directory as `candidate_passes` does. It prints
`{"valid": bool, "symbols": [...], "edit": "none" | "fails-check" | "fails-test" | "passes"}`.

`guide/reward.py` lays each answer's file in its own scratch directory (R4.4) and calls the judge
with a 20 s deadline. The score (R4.1–R4.3):

| the answer | locations | edit | reward |
|---|---|---|---|
| outside the schema, or the judge failed | — | — | 0 |
| names a declaration the file does not have, or none | 0 | | |
| otherwise: F-score, beta 3, of the declarations named against the fix's | 0–1 | | |
| edit passes | | 1 | |
| no edit, or one that fails check or the test | | 0 | |

and the reward is the mean of the two parts. A copy of the failing body, the pilot's habit, fails
check and earns no edit credit, while a location that is right still earns half. The F-score favours
recall, as SoRFT's does; naming more declarations than the fix changed lowers precision, so it
never scores more than naming the right one — the first reward, which paid half for a right
declaration listed after a wrong one, was the shape SoRFT saw gamed
(adr:0020-the-guide-trains-by-fine-tuning-then-rejection-sampling-and-rl-on-what-it-sometimes-solves).
`dev judge` says which named symbols the file does not declare. Judging fans out
over a thread pool as wide as the pod's cores, since each answer is a separate `lotml` process.

## Reports (R5)

`harness/results/training/<run>.md` is written from `report.json`, `run.json` and the ledger row:
the stages and their settings, the records' digest, the GPU, the wall time and the cost; on the
validation split, before and after reinforcement learning, answers within the schema, top-1 and
top-3, edits that pass, the threshold and its precision; and the count of answers the judge could
not score. Every text written there and to the ledger passes `secrets.scrub` and
`secrets.anonymised` (R5.2).

## Alternatives considered

- **RunPod's Python SDK** (`runpod`) instead of `urllib`: one more dependency for four calls, and
  the harness already keeps its HTTP clients to the standard library.
- **A network volume** for checkpoints: billed by the month and bound to one data center, so a run
  could not move to whichever has a card free; the user chose a private Hugging Face repository.
- **PPO** instead of group-relative policy optimization: GRPO keeps PPO's clipped objective and
  its KL penalty to the reference model, and replaces the learned value model with the mean reward
  of the group sampled for the same failure. The compiler scores a whole answer once, at its end,
  so a per-token value estimate adds little, and a value model the size of the policy is a second
  network to train, tune and pay GPU hours for.
- **vLLM for generation in GRPO**: faster, but a second runtime with its own CUDA pins.
- **Similarity of the edit to the reference**, SLMFix's second term: the compiler's verdict already
  says whether the edit fixes the file, and a textual similarity would reward copying the broken
  body, the habit this stage is meant to remove.
