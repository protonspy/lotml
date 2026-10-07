"""The guide asked about the validation split as an agent's tool asks it — `lotml guide ask`
against a llama-server serving the GGUF file, the gate running every proposed edit — and the
threshold calibrated on its answers (specs/training-pipeline/ R3.4, R5.1).

Every record is asked with the threshold at zero, so each answer's confidence is seen; the
threshold is then the lowest one whose answers reach the target precision on the first location.
"""

import contextlib
import json
import os
import subprocess
import tempfile
import time
import urllib.request
from collections import Counter
from collections.abc import Iterator
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from lotml_harness.agent import safe
from lotml_harness.guide import calibrate, train

PRECISION = 0.9
"""The share of shown answers whose first location is right that the threshold is held to."""
DEADLINE = 120.0
PORT = 8091


@contextlib.contextmanager
def served(gguf: Path, llama_cpp: Path, port: int = PORT, slots: int = 4) -> Iterator[str]:
    """`llama-server` from `llama_cpp` serving `gguf` on the GPU, every layer offloaded, until the
    block ends; its URL once it answers its health check."""
    server = train._tool(llama_cpp, ("llama-server",))
    libraries = sorted({str(p.parent) for p in llama_cpp.rglob("*.so*")})
    env = os.environ | {
        "LD_LIBRARY_PATH": ":".join([*libraries, os.environ.get("LD_LIBRARY_PATH", "")])
    }
    command = [str(server), "-m", str(gguf), "--host", "127.0.0.1", "--port", str(port),
               "-c", "8192", "-ngl", "99", "-np", str(slots)]  # fmt: skip
    quiet = {"stdout": subprocess.DEVNULL, "stderr": subprocess.DEVNULL}
    with subprocess.Popen(command, env=env, **quiet) as process:  # noqa: S603
        url = f"http://127.0.0.1:{port}"
        try:
            for _ in range(300):
                if process.poll() is not None:
                    raise RuntimeError(f"llama-server exited with {process.returncode}")
                if healthy(url):
                    break
                time.sleep(1)
            else:
                raise RuntimeError("llama-server did not become healthy in 300 s")
            yield url
        finally:
            process.terminate()
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=30)


def healthy(url: str) -> bool:
    try:
        with urllib.request.urlopen(f"{url}/health", timeout=2) as response:  # noqa: S310
            return response.status == 200
    except OSError:
        return False


def configuration(url: str, records: Path, path: Path) -> Path:
    """The guide tool's configuration for asking every record: threshold zero, the tool's answer
    budget, candidates run."""
    path.write_text(
        f'url = "{url}"\nmodel = "guide"\nthreshold = 0.0\nanswer = 1024\n'
        f'run_candidates = true\nrenderer = 1\nrecords = "{records.as_posix()}"\n',
        encoding="utf-8",
    )
    return path


def truth(record: dict) -> list[str | None]:
    """The symbols of the declarations the record's real fix changed."""
    return [loc["symbol"] for loc in json.loads(record["messages"][-1]["content"])["locations"]]


def asked(record: dict, config: Path, deadline: float = DEADLINE) -> dict:
    """One record asked through `lotml guide ask`, its file laid alone in a scratch project; a row
    of the answer's verdicts."""
    state = record["state"]
    args = ["guide", "ask", "--root", "."] + (
        ["--task", state["task"]] if state.get("task") else []
    )
    with tempfile.TemporaryDirectory(prefix="lotml-validate-", ignore_cleanup_errors=True) as d:
        root = Path(d)
        safe.lay(root, {state["path"]: state["text"]})
        env = {"LOTML_HARNESS_GUIDE": str(config)}
        done = safe.lotml(args, [state["path"]], root, deadline, env=env)
    answer: dict = {"guidance": None, "reason": "deadline"}
    if done is not None:
        try:
            answer = json.loads(done.stdout.strip().splitlines()[-1])
        except (json.JSONDecodeError, IndexError):
            answer = {"guidance": None, "reason": "unread"}
    expected = truth(record)
    symbols = [loc.get("symbol") for loc in answer.get("locations") or []]
    silent = "guidance" in answer and answer["guidance"] is None
    return {
        "silent": silent,
        "reason": answer.get("reason") if silent else None,
        "top1": bool(symbols) and symbols[0] in expected,
        "top3": any(s in expected for s in symbols[:3]),
        "edit": answer.get("edit") is not None,
        "confidence": answer.get("confidence"),
    }


def metrics(rows: list[dict]) -> dict:
    """The split's verdicts: answers within the schema, top-1 and top-3 location, edits shown —
    which the gate shows only when they check clean and pass the failing block — and the
    silences by reason, each share over every record asked."""
    count = len(rows) or 1
    return {
        "records": len(rows),
        "within_schema": sum(1 for r in rows if r["reason"] != "invalid-answer") / count,
        "top1": sum(r["top1"] for r in rows) / count,
        "top3": sum(r["top3"] for r in rows) / count,
        "edits_pass": sum(r["edit"] for r in rows) / count,
        "silent": dict(Counter(r["reason"] for r in rows if r["silent"])),
    }


def calibrated(rows: list[dict], target: float = PRECISION) -> tuple[float, float, float]:
    """The threshold for `target` on the rows that carry a confidence, the precision the answers
    at or above it reached, and the share of records they are."""
    pairs = [(r["confidence"], r["top1"]) for r in rows if r["confidence"] is not None]
    chosen = calibrate.threshold(pairs, target)
    shown = [ok for confidence, ok in pairs if confidence >= chosen]
    precision = sum(shown) / len(shown) if shown else 0.0
    return chosen, precision, len(shown) / (len(rows) or 1)


def validate(
    gguf: Path, llama_cpp: Path, records: Path, workers: int = 4
) -> tuple[list[dict], dict]:
    """Ask every validation record of `records` about `gguf`, served on the GPU; the rows and their
    metrics."""
    found = train.load(records, "validation")
    with served(gguf, llama_cpp) as url, tempfile.TemporaryDirectory() as directory:
        config = configuration(url, records, Path(directory) / "guide.toml")
        with ThreadPoolExecutor(max_workers=workers) as pool:
            rows = list(pool.map(lambda r: asked(r, config), found))
    return rows, metrics(rows)
