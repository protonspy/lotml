"""What a training pod runs: its environment, and a wrapper that removes the pod on any exit
(specs/training-pipeline/ R1.5, R1.6)."""

import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from lotml_harness.guide import pod

COMMIT = "0123456789abcdef0123456789abcdef01234567"
POSIX = pytest.mark.skipif(
    sys.platform == "win32" or not shutil.which("timeout"), reason="POSIX bash"
)


def test_the_environment_carries_the_run_and_the_token_and_not_the_runpod_key(
    monkeypatch: pytest.MonkeyPatch,
):
    monkeypatch.setenv("HF_TOKEN", "hf_x")
    monkeypatch.setenv("RUNPOD_API_KEY", "rp_x")
    env = pod.environment("r1", COMMIT, ["sft", "rl"], 3600, "me/guide", "abc", {"rl": "r0"})
    assert env["HF_TOKEN"] == "hf_x" and "rp_x" not in env.values()  # noqa: S105 - fake
    assert (env["LOTML_STAGES"], env["LOTML_INPUTS"], env["LOTML_SECONDS"]) == (
        "sft,rl",
        "rl=r0",
        "3600",
    )
    assert env["LOTML_LLAMA"] == "b11450"
    with pytest.raises(ValueError, match="not a full commit"):
        pod.environment("r1", "main; rm -rf /", ["sft"], 60, "me/guide", "abc", {})


def test_the_command_interpolates_nothing():
    expanded = ("${RUNPOD_API_KEY:-}", "${LOTML_ROOT:-/lotml}", "${LOTML_LOG:-/tmp/lotml-pod.log}",
                "${HF_TOKEN:-no-token}", '{"action":"terminate"}', "gone() {")  # fmt: skip
    text = pod.WRAPPER
    for known in expanded:
        text = text.replace(known, "")
    assert "{" not in text


def stubs(tmp_path: Path, git_fails: bool) -> tuple[Path, dict[str, str]]:
    """A PATH whose `git` and `runpodctl` record their calls, and whose clone holds a run.sh."""
    bin_dir, calls = tmp_path / "bin", tmp_path / "calls"
    bin_dir.mkdir()
    git = f'echo "git $*" >> {calls}\n' + ("exit 1\n" if git_fails else "")
    git += (
        'if [ "$1" = clone ]; then d="${@: -1}"; mkdir -p "$d/harness/pod"; '
        'printf "sleep 30\\n" > "$d/harness/pod/run.sh"; fi\n'
    )
    for name, body in (("git", git), ("runpodctl", f'echo "runpodctl $*" >> {calls}\n')):
        (bin_dir / name).write_text(f"#!/usr/bin/env bash\n{body}", encoding="utf-8", newline="")
        (bin_dir / name).chmod(0o755)
    env = {
        "PATH": f"{bin_dir}:/usr/bin:/bin",
        "RUNPOD_POD_ID": "pod_9",
        "LOTML_ROOT": str(tmp_path / "lotml"),
        "LOTML_REPOSITORY": "https://example.invalid/lotml",
        "LOTML_COMMIT": COMMIT,
        "LOTML_SECONDS": "1",
        "LOTML_LOG": str(tmp_path / "pod.log"),
    }
    return calls, env


@POSIX
@pytest.mark.parametrize("git_fails", [True, False])
def test_the_pod_removes_itself_when_the_clone_fails_or_the_deadline_passes(
    tmp_path: Path, git_fails: bool
):
    calls, env = stubs(tmp_path, git_fails)
    subprocess.run(["bash", "-c", pod.WRAPPER], env=env, timeout=30, check=False)  # noqa: S603, S607
    lines = calls.read_text(encoding="utf-8").splitlines()
    assert lines[-1] == "runpodctl remove pod pod_9"
    assert lines[0].startswith("git clone --quiet https://example.invalid/lotml")
    if not git_fails:
        assert f"git -C {tmp_path / 'lotml'} checkout --quiet {COMMIT}" in lines


def test_the_run_script_checks_llama_cpp_by_digest_and_runs_the_stages():
    script = (Path(pod.__file__).parents[2] / "pod" / "run.sh").read_text(encoding="utf-8")
    assert "sha256sum -c --quiet -" in script
    assert "--locked --release -p lotml" in script
    assert "python -m lotml_harness.guide.stages" in script
