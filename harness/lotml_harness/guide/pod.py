"""What a training pod runs: a wrapper that removes the pod on any exit and holds the run to its
deadline, around the repository at one commit and `harness/pod/run.sh` (specs/training-pipeline/
R1.5, R1.6).

Everything the run needs reaches the pod as environment variables, so the command itself is fixed
text that interpolates nothing. The RunPod key is not among them: a pod removes itself with the
pod-scoped credentials RunPod gives it.
"""

import re

from lotml_harness.guide import hub

REPOSITORY = "https://github.com/protonspy/lotml"
IMAGE = "runpod/pytorch:1.0.2-cu1281-torch280-ubuntu2404"
"""CUDA 12.8 for the card; the training environment brings its own torch through uv."""
DISK = 40
"""GB of container disk: the toolchains, the environment, the base model and the run's outputs."""
LLAMA = "b11450"
LLAMA_BINARY_SHA256 = "c93e6b94ae881351d1325c6bf067e6838b4dadbc9fb3fd02cc4342404950e089"
"""`llama-b11450-bin-ubuntu-cuda-12.8-x64.tar.gz`, as the release lists it: the validation asks
the guide on the pod's GPU rather than its few cores."""
LLAMA_CUDART_SHA256 = "30a4c1367f07d387390f6928096be37a9cb9818ad2a4426936f25f70629eafe2"
"""`cudart-llama-b11450-bin-ubuntu-cuda-12.8-x64.tar.gz`: the CUDA libraries that build links."""
LLAMA_SOURCE_SHA256 = "bc717d30da4d3c0546aded0f254349fd272a65fd1c87638ab06b98f781d7a54b"
"""The tag's source archive, for `convert_hf_to_gguf.py` and `gguf-py`."""

WRAPPER = """set -uo pipefail
gone() {
  runpodctl remove pod "$RUNPOD_POD_ID" \\
    || curl -fsS -X POST -H "Authorization: Bearer ${RUNPOD_API_KEY:-}" \\
         -H "Content-Type: application/json" -d '{"action":"terminate"}' \\
         "https://api.runpod.io/v2/pods/$RUNPOD_POD_ID/action"
}
trap gone EXIT
root="${LOTML_ROOT:-/lotml}"
git clone --quiet "$LOTML_REPOSITORY" "$root" \\
  && git -C "$root" checkout --quiet "$LOTML_COMMIT" \\
  && timeout --kill-after=60 "$LOTML_SECONDS" bash "$root/harness/pod/run.sh"
"""
"""The pod's command under `bash -c`: the trap is set before anything can fail, so a clone that
fails, a stage that crashes, a run past its deadline and a run that ends all remove the pod."""

COMMIT = re.compile(r"[0-9a-f]{40}")


def environment(
    run: str,
    commit: str,
    stages: list[str],
    seconds: int,
    repo: str,
    records: str,
    inputs: dict[str, str],
) -> dict[str, str]:
    """The pod's environment: the token, what to run and from where, and llama.cpp's pins."""
    if not COMMIT.fullmatch(commit):
        raise ValueError(f"{commit!r} is not a full commit id")
    return {
        "HF_TOKEN": hub.token(),
        "LOTML_REPOSITORY": REPOSITORY,
        "LOTML_COMMIT": commit,
        "LOTML_RUN": run,
        "LOTML_STAGES": ",".join(stages),
        "LOTML_SECONDS": str(seconds),
        "LOTML_HUB": repo,
        "LOTML_RECORDS": records,
        "LOTML_INPUTS": ",".join(f"{stage}={source}" for stage, source in inputs.items()),
        "LOTML_LLAMA": LLAMA,
        "LOTML_LLAMA_BINARY_SHA256": LLAMA_BINARY_SHA256,
        "LOTML_LLAMA_CUDART_SHA256": LLAMA_CUDART_SHA256,
        "LOTML_LLAMA_SOURCE_SHA256": LLAMA_SOURCE_SHA256,
    }
