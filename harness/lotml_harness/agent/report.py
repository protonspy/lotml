"""The agent harness's report: pass@k by Chen et al.'s unbiased estimator, the Wilson interval of
pass@1, and the Markdown tables (specs/agent-harness/ R4.2).

A run whose model call failed (`outcome` `error`) measured the provider, not the agent: it is
counted apart and left out of every rate, and the next invocation runs it again.
"""

import math
from collections import defaultdict
from statistics import mean


def pass_at_k(n: int, c: int, k: int) -> float:
    """The chance that at least one of `k` runs drawn from `n`, `c` of them passing, passes."""
    if not 0 <= c <= n or not 1 <= k <= n:
        raise ValueError(f"pass@{k} needs 1 <= k <= n and 0 <= c <= n, with n={n}, c={c}")
    if n - c < k:
        return 1.0
    return 1.0 - math.comb(n - c, k) / math.comb(n, k)


def wilson(passed: int, runs: int, z: float = 1.96) -> tuple[float, float]:
    """The Wilson score interval of a pass rate; 95% for the default `z`."""
    if runs == 0:
        return 0.0, 1.0
    p = passed / runs
    denominator = 1 + z * z / runs
    center = (p + z * z / (2 * runs)) / denominator
    half = z * math.sqrt(p * (1 - p) / runs + z * z / (4 * runs * runs)) / denominator
    return max(0.0, center - half), min(1.0, center + half)


def latest(rows: list[dict]) -> list[dict]:
    """One row per model, task, arm and attempt: the last one written."""
    kept = {(r["model"], r["task"], r["arm"], r["attempt"]): r for r in rows}
    return list(kept.values())


def summarize(rows: list[dict]) -> dict[tuple[str, str], dict]:
    """Per model and arm: runs, pass@k, the interval, and the means per run."""
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for row in latest(rows):
        groups[(row["model"], row["arm"])].append(row)
    summary = {}
    for key, group in sorted(groups.items()):
        graded = [r for r in group if r["outcome"] != "error"]
        by_task: dict[str, list[dict]] = defaultdict(list)
        for row in graded:
            by_task[row["task"]].append(row)
        passed = sum(r["outcome"] == "pass" for r in graded)
        fewest = min((len(v) for v in by_task.values()), default=0)
        summary[key] = {
            "runs": len(graded),
            "errors": len(group) - len(graded),
            "tasks": len(by_task),
            "passed": passed,
            "wilson": wilson(passed, len(graded)),
            "pass_at": {
                k: mean(
                    pass_at_k(len(v), sum(r["outcome"] == "pass" for r in v), k)
                    for v in by_task.values()
                )
                for k in range(1, fewest + 1)
            },
            "hidden": mean(r["hidden"][0] / r["hidden"][1] for r in graded) if graded else 0.0,
            "checks": sum(r["checks"] for r in graded),
            "stopped": dict(sorted(_count(r["stopped"] for r in graded).items())),
            "per_run": {
                name: mean(value(r) for r in graded) if graded else 0.0
                for name, value in PER_RUN.items()
            },
        }
    return summary


PER_RUN = {
    "model calls": lambda r: r["model_calls"],
    "tool calls": lambda r: sum(r["tools"].values()),
    "tool errors": lambda r: r["tool_errors"],
    "checks with errors": lambda r: r["check_errors"],
    "input tokens": lambda r: r["tokens"]["input"],
    "output tokens": lambda r: r["tokens"]["output"],
    "reasoning tokens": lambda r: r["tokens"]["reasoning"],
    "cost (USD)": lambda r: r["cost"],
    "seconds": lambda r: r["seconds"],
    "lines changed": lambda r: r["lines_changed"],
}


def _count(values) -> dict:
    counts: dict = defaultdict(int)
    for v in values:
        counts[v] += 1
    return counts


def markdown(rows: list[dict]) -> str:
    rows = latest(rows)
    summary = summarize(rows)
    lines = [
        "# Agent harness",
        "",
        "A deepagents agent on the agent benchmark (`harness/agent_bench/`), with the compiler's",
        "MCP tools, graded on hidden tests (specs/agent-harness/). Arm `agents`: `lotml init`",
        "wrote `AGENTS.md`, loaded as the agent's memory, and `lotml.guide.lotml`; arm",
        "`reference`: the language reference in the system prompt. Written by",
        "`python -m lotml_harness.agent`.",
        "",
        "## Results",
        "",
        "| model | arm | tasks | runs | pass@1 | 95% interval | pass@k | hidden tests | checks |"
        " errors | stopped |",
        "| --- | --- | ---: | ---: | ---: | --- | --- | ---: | ---: | ---: | --- |",
    ]
    for (model, arm), s in summary.items():
        low, high = s["wilson"]
        at = ", ".join(f"@{k} {v:.2f}" for k, v in s["pass_at"].items() if k > 1) or "-"
        stopped = ", ".join(f"{k} {v}" for k, v in s["stopped"].items())
        lines.append(
            f"| {model} | {arm} | {s['tasks']} | {s['runs']} | {s['pass_at'].get(1, 0.0):.2f} |"
            f" {low:.2f}-{high:.2f} | {at} | {s['hidden']:.0%} | {s['checks']}/{s['runs']} |"
            f" {s['errors']} | {stopped} |"
        )
    lines += ["", "## Per run, on average", "", "| model | arm | " + " | ".join(PER_RUN) + " |"]
    lines.append("| --- | --- |" + " ---: |" * len(PER_RUN))
    for (model, arm), s in summary.items():
        cells = " | ".join(_number(v) for v in s["per_run"].values())
        lines.append(f"| {model} | {arm} | {cells} |")
    lines += ["", "## Per task", "", "Runs passed out of runs graded.", ""]
    columns = sorted(summary)
    lines.append("| task | kind | " + " | ".join(f"{m} {a}" for m, a in columns) + " |")
    lines.append("| --- | --- |" + " ---: |" * len(columns))
    tasks = sorted({(r["task"], r["kind"]) for r in rows})
    for task, kind in tasks:
        cells = []
        for model, arm in columns:
            mine = [
                r
                for r in rows
                if (r["task"], r["model"], r["arm"]) == (task, model, arm)
                and r["outcome"] != "error"
            ]
            passed = sum(r["outcome"] == "pass" for r in mine)
            cells.append(f"{passed}/{len(mine)}" if mine else "-")
        lines.append(f"| {task} | {kind} | " + " | ".join(cells) + " |")
    lines += [
        "",
        "Eight tasks are far below the 168 paired tasks a 10-point difference needs",
        "(docs/wiki/pages/evaluation-harness.md): read the arms' difference as a direction, and",
        "the per-task table and the traces in `harness/cache/agent/` for where the agent fails.",
        "",
    ]
    return "\n".join(lines)


def _number(value: float) -> str:
    if value >= 100:
        return f"{value:,.0f}"
    if value >= 1 or value == 0:
        return f"{value:.1f}"
    return f"{value:.4f}"
