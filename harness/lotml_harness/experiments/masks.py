"""Type masks for open models, by the line (docs/wiki/pages/constrained-decoding.md): does
checking what an open model writes, while it writes, raise how often its first answer passes?

Type-constrained decoding masks each token the type checker rules out; Ollama does not let a
caller mask tokens, so here the mask is applied a line at a time. The model writes its answer
one line per call, through Ollama's raw mode and its own chat template, greedily. Each line is
checked with what came before it by `lotml check --prefix`; a line after which the program can no
longer be completed is drawn again at a higher temperature, up to `RETRIES` times, and kept if
every draw is refused. The free arm is the same model, prompt and template, answering in one
greedy call. Both answers run on the hidden tests of the phase 1 sample, and the arms are
compared per task by the exact McNemar test.

A model named by its OpenRouter id (`owner/model`) is served instead by OpenRouter's raw text
completions, from one provider and no other; the key is read from `OPENROUTER_API_KEY`.

    python -m lotml_harness.experiments.masks --model qwen2.5-coder:7b --model llama3.1:8b
    python -m lotml_harness.experiments.masks --model meta-llama/llama-3.1-8b-instruct --workers 8
"""

import argparse
import json
import os
import sys
import tempfile
import time
import urllib.error
import urllib.request
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from lotml_harness.experiments import variants
from lotml_harness.experiments.models import ModelError
from lotml_harness.experiments.phase1 import RESULTS, Lotml
from lotml_harness.tasks import Task, build

RUNS = RESULTS / "masks"
REPORT = RESULTS / "masks.md"
ARMS = ("free", "masked")
OPEN = "```lotml\n"
RETRIES = 3
"""Draws of a refused line after the greedy one."""
TEMPERATURE = 0.8
MAX_LINES = 80
"""Lines an answer may have before its decoding is cut off."""
HOST = "http://localhost:11434"
OPENROUTER = "https://openrouter.ai/api/v1/completions"
PROVIDERS = {
    "meta-llama/llama-3.1-8b-instruct": "DeepInfra",
    "z-ai/glm-5.3-flash": "Parasail",
}
"""The one provider OpenRouter may send each hosted model's calls to, so every line of a run comes
from the same weights and quantization. Each was checked to take the prompt as written: several
providers wrap a text completion in their own chat template, and the model then answers a turn
the experiment never wrote."""
ATTEMPTS = 5
EVERY = 2
"""Every other task of the phase 1 sample, 100 of its 200: a line costs a call, about 2.4 s for a
7-8B model on the machine the runs were made on, so the whole sample would take six hours."""

Generate = Callable[[str, float, int], str]
"""A raw prompt, a temperature and a seed, to the next line the model writes."""


def label(model: str) -> str:
    """The model's name in the runs: an Ollama tag or an OpenRouter id, without its owner."""
    return model.rsplit("/", 1)[-1].replace(":", "-")


def served(model: str) -> str:
    """Where the model runs: an OpenRouter id has its owner before a slash, an Ollama tag not."""
    if "/" not in model:
        return "ollama"
    if model not in PROVIDERS:
        raise ValueError(f"{model}: no provider checked to take a raw prompt")
    return f"openrouter/{PROVIDERS[model]}"


def raw_prompt(model: str, system: str, user: str, partial: str) -> str:
    """The prompt as the model's chat template lays it out, its answer begun with `partial`."""
    if "glm" in model:
        return (
            f"[gMASK]<sop><|system|>Reasoning Effort: Low<|system|>{system}<|user|>{user}"
            f"<|assistant|><think></think>{partial}"
        )
    if "llama" in model:
        return (
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n"
            f"{system}<|eot_id|><|start_header_id|>user<|end_header_id|>\n\n{user}<|eot_id|>"
            f"<|start_header_id|>assistant<|end_header_id|>\n\n{partial}"
        )
    return (
        f"<|im_start|>system\n{system}<|im_end|>\n<|im_start|>user\n{user}<|im_end|>\n"
        f"<|im_start|>assistant\n{partial}"
    )


@dataclass
class Decoded:
    code: str
    rejected: int = 0
    calls: int = 0


def fence(line: str) -> bool:
    return line.strip().startswith("```")


def masked(
    model: str, system: str, user: str, generate: Generate, check: Callable[[str], str]
) -> Decoded:
    """An answer written a line at a time, each line the compiler refuses drawn again."""
    lines: list[str] = []
    rejected = calls = blanks = 0
    for _ in range(MAX_LINES):
        prompt = raw_prompt(model, system, user, OPEN + "".join(f"{line}\n" for line in lines))
        line = generate(prompt, 0.0, 0)
        calls += 1
        tries = 0
        while (
            not fence(line)
            and tries < RETRIES
            and check("\n".join([*lines, line]) + "\n") == "error"
        ):
            tries += 1
            rejected += 1
            line = generate(prompt, TEMPERATURE, tries)
            calls += 1
        if fence(line):
            break
        blanks = blanks + 1 if not line.strip() else 0
        if blanks == 2:
            break
        lines.append(line)
    return Decoded("\n".join(lines).rstrip("\n") + "\n", rejected, calls)


def free(model: str, system: str, user: str, complete: Callable[[str], str]) -> Decoded:
    """The same answer written in one greedy call, up to its closing fence."""
    text = complete(raw_prompt(model, system, user, OPEN))
    return Decoded(text.split("```", 1)[0].rstrip("\n") + "\n", 0, 1)


def prefix_checker(lotml: Lotml) -> Callable[[str], str]:
    """`lotml check --prefix` on a partial answer: completable, error or unknown."""

    def check(code: str) -> str:
        with tempfile.TemporaryDirectory(prefix="lotml-masks-") as directory:
            Path(directory, "solution.lotml").write_text(code, encoding="utf-8")
            result = lotml.compiler(["check", "--prefix", "--json", "solution.lotml"], directory)
        if result is None:
            return "unknown"
        try:
            return json.loads(result.stdout)["prefix"][0]["verdict"]
        except (json.JSONDecodeError, KeyError, IndexError):
            return "unknown"

    return check


def ollama(
    model: str, prompt: str, temperature: float, seed: int, stop: list[str], limit: int
) -> str:
    body = {
        "model": model,
        "prompt": prompt,
        "raw": True,
        "stream": False,
        "options": {
            "temperature": temperature,
            "seed": seed,
            "stop": stop,
            "num_predict": limit,
            "num_ctx": 16384,
        },
    }
    request = urllib.request.Request(  # noqa: S310
        f"{HOST}/api/generate",
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=900) as response:  # noqa: S310
            return json.loads(response.read()).get("response", "")
    except (OSError, urllib.error.URLError, json.JSONDecodeError) as error:
        raise ModelError(f"ollama did not answer: {error}") from None


def openrouter_body(
    model: str, prompt: str, temperature: float, seed: int, stop: list[str], limit: int
) -> dict:
    """A raw text completion: the prompt goes to the model as written, template and all."""
    return {
        "model": model,
        "prompt": prompt,
        "temperature": temperature,
        "seed": seed,
        "stop": stop,
        "max_tokens": limit,
        "provider": {"order": [PROVIDERS[model]], "allow_fallbacks": False},
    }


class NoRedirect(urllib.request.HTTPRedirectHandler):
    """A redirect is refused rather than followed: urllib would carry the key's header along."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = urllib.request.build_opener(NoRedirect())


def openrouter(
    model: str, prompt: str, temperature: float, seed: int, stop: list[str], limit: int
) -> str:
    key = os.environ.get("OPENROUTER_API_KEY")
    if not key:
        raise ModelError("OPENROUTER_API_KEY is not set")
    request = urllib.request.Request(  # noqa: S310
        OPENROUTER,
        data=json.dumps(openrouter_body(model, prompt, temperature, seed, stop, limit)).encode(),
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"},
    )
    for attempt in range(ATTEMPTS):
        try:
            with OPENER.open(request, timeout=120) as response:
                choice = json.loads(response.read())["choices"][0]
            if choice.get("reasoning"):
                raise ModelError(f"{PROVIDERS[model]} wrapped the raw prompt in a chat turn")
            return choice.get("text") or ""
        except urllib.error.HTTPError as error:
            if (error.code != 429 and error.code < 500) or attempt == ATTEMPTS - 1:
                raise ModelError(f"openrouter refused the call: {error}") from None
            time.sleep(2**attempt)
        except (
            OSError,
            urllib.error.URLError,
            json.JSONDecodeError,
            KeyError,
            IndexError,
        ) as error:
            if attempt == ATTEMPTS - 1:
                raise ModelError(f"openrouter did not answer: {error}") from None
            time.sleep(2**attempt)
    return ""


def complete(
    model: str, prompt: str, temperature: float, seed: int, stop: list[str], limit: int
) -> str:
    call = openrouter if served(model) != "ollama" else ollama
    return call(model, prompt, temperature, seed, stop, limit)


def answer(model: str, task: Task, arm: str, lotml: Lotml) -> dict:
    record = {"model": label(model), "served": served(model), "task": task.id, "arm": arm}
    system, user = variants.prompt(task, "b")
    try:
        if arm == "free":
            decoded = free(model, system, user, lambda p: complete(model, p, 0.0, 0, ["```"], 2048))
        else:
            decoded = masked(
                model,
                system,
                user,
                lambda p, t, s: complete(model, p, t, s, ["\n"], 200),
                prefix_checker(lotml),
            )
    except ModelError as error:
        return record | {"error": str(error)}
    verdict = lotml.judge(task, decoded.code)
    return record | {
        "error": None,
        "code": decoded.code,
        "passed": verdict.passed,
        "outcome": verdict.outcome,
        "rejected": decoded.rejected,
        "calls": decoded.calls,
    }


def run(model: str, tasks: list[Task], path: Path, workers: int = 1) -> list[dict]:
    lotml = Lotml()
    done = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["arm"])] = record
    todo = [(t, arm) for t in tasks for arm in ARMS if (t.id, arm) not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(lambda job: answer(model, job[0], job[1], lotml), todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            if record.get("error") is None:
                done[(record["task"], record["arm"])] = record
    return list(done.values())


def summarize(rows: list[dict]) -> dict[str, dict]:
    by_model: dict[str, dict[tuple[str, str], dict]] = {}
    for r in rows:
        if r.get("error") is None:
            by_model.setdefault(r["model"], {})[(r["task"], r["arm"])] = r
    summary = {}
    for model, records in sorted(by_model.items()):
        paired = sorted({t for t, _ in records if all((t, arm) in records for arm in ARMS)})
        passed = {arm: {t for t in paired if records[(t, arm)]["passed"]} for arm in ARMS}
        only_free = len(passed["free"] - passed["masked"])
        only_masked = len(passed["masked"] - passed["free"])
        summary[model] = {
            "served": next((r["served"] for r in records.values() if r.get("served")), "ollama"),
            "pairs": len(paired),
            "passed": {arm: len(passed[arm]) for arm in ARMS},
            "refused": {
                arm: sum(records[(t, arm)]["outcome"] == "does not check" for t in paired)
                for arm in ARMS
            },
            "only_free": only_free,
            "only_masked": only_masked,
            "p": variants.mcnemar(only_free, only_masked),
            "rejected": sum(records[(t, "masked")]["rejected"] for t in paired),
        }
    return summary


def markdown(summary: dict[str, dict], tasks: int) -> str:
    lines = [
        "# Type masks for open models, by the line",
        "",
        "Generated by `python -m lotml_harness.experiments.masks`. Each open model wrote",
        f"{tasks} tasks of the phase 1 sample, every other one, twice from a raw prompt in its",
        "own chat template, served by Ollama's raw mode or by OpenRouter's text completions",
        "from the one provider named: free, in one greedy call; and masked, one line per call,",
        "each line checked with what came before it by `lotml check --prefix`, and a line that",
        f"left the program impossible to complete drawn again at temperature {TEMPERATURE}, up",
        f"to {RETRIES} times. Neither lets a caller mask tokens, so this is a coarse mask: a",
        "line, not a token, is the unit refused.",
        "",
        "| model | served by | tasks | pass@1, free | pass@1, masked | refused, free |"
        " refused, masked | only free | only masked | McNemar p | lines redrawn |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for model, s in summary.items():
        n = max(1, s["pairs"])
        lines.append(
            f"| {model} | {s['served']} | {s['pairs']} | {s['passed']['free'] / n:.1%} |"
            f" {s['passed']['masked'] / n:.1%} |"
            f" {s['refused']['free']} | {s['refused']['masked']} | {s['only_free']} |"
            f" {s['only_masked']} | {s['p']:.3f} | {s['rejected']} |"
        )
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", action="append", default=[], help="qwen2.5-coder:7b")
    parser.add_argument("--workers", type=int, default=1)
    parser.add_argument(
        "--every", type=int, default=EVERY, help="every Nth task of the phase 1 sample"
    )
    args = parser.parse_args()
    tasks = variants.sample(build.load(), variants.SAMPLE)[:: args.every]
    for model in args.model:
        run(model, tasks, RUNS / f"{label(model)}.jsonl", workers=args.workers)
    wanted = {t.id for t in tasks}
    rows = [
        record
        for path in sorted(RUNS.glob("*.jsonl"))
        for line in path.read_text(encoding="utf-8").splitlines()
        if (record := json.loads(line))["task"] in wanted
    ]
    report = markdown(summarize(rows), len(tasks))
    REPORT.write_text(report, encoding="utf-8")
    sys.stdout.buffer.write(report.encode("utf-8"))


if __name__ == "__main__":
    main()
