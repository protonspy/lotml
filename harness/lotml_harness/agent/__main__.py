"""Run the agent benchmark and write its report.

    python -m lotml_harness.agent --attempts 3
    python -m lotml_harness.agent --model z-ai/glm-5.3-flash --arm agents --task stock-take
    python -m lotml_harness.agent --report-only

Rows go to `harness/results/agent/<model>__<arm>.jsonl`, the report to `harness/results/agent.md`.
A task, arm and attempt already recorded is skipped, unless its run ended in a model error.
"""

import argparse
import json
import sys
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from lotml_harness.agent import report, secrets
from lotml_harness.agent.bench import AgentTask, tasks
from lotml_harness.agent.run import ARMS, MODEL, error_row, openrouter, run
from lotml_harness.experiments.phase1 import RESULTS

RUNS = RESULTS / "agent"
REPORT = RESULTS / "agent.md"


def rows_file(runs: Path, model: str, arm: str) -> Path:
    return runs / f"{model.replace('/', '__')}__{arm}.jsonl"


def read_rows(path: Path) -> list[dict]:
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def pending(found: list[AgentTask], attempts: int, done: list[dict]) -> list[tuple[AgentTask, int]]:
    """The runs still to do: every task and attempt not recorded, or recorded as a model error."""
    recorded = {(r["task"], r["attempt"]) for r in report.latest(done) if r["outcome"] != "error"}
    return [(t, a) for t in found for a in range(attempts) if (t.id, a) not in recorded]


def main(argv: list[str] | None = None, runs: Path = RUNS, written: Path = REPORT) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", default=MODEL, help="an OpenRouter model id")
    parser.add_argument("--arm", action="append", choices=ARMS, help="both when absent")
    parser.add_argument("--attempts", type=int, default=1, help="runs per task and arm")
    parser.add_argument("--task", action="append", help="only these tasks")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--report-only", action="store_true")
    parser.add_argument("--live", action="store_true", help="say each model and tool call")
    args = parser.parse_args(argv)
    if not args.report_only:
        found = [t for t in tasks() if not args.task or t.id in args.task]
        model = openrouter(args.model)
        lock = threading.Lock()
        runs.mkdir(parents=True, exist_ok=True)
        for arm in args.arm or ARMS:
            path = rows_file(runs, args.model, arm)
            todo = pending(found, args.attempts, read_rows(path))

            def one(job: tuple[AgentTask, int], arm: str = arm, path: Path = path) -> None:
                task, attempt = job
                live = None
                if args.live:

                    def live(text: str, tag: str = f"[{arm} {task.id} #{attempt}]") -> None:
                        print(f"{tag} {text}", flush=True)

                try:
                    row = run(task, arm, attempt, model, args.model, live=live)
                except Exception as failure:  # noqa: BLE001 - one broken run must not stop the rest (R2.7)
                    row = error_row(
                        task, arm, attempt, args.model, f"{type(failure).__name__}: {failure}"
                    )
                row = secrets.scrub_value(row)
                with lock, path.open("a", encoding="utf-8") as out:
                    out.write(json.dumps(row) + "\n")
                print(
                    f"{arm} {task.id} #{attempt}: {row['outcome']} {row['hidden'][0]}/"
                    f"{row['hidden'][1]}, {row['model_calls']} calls, ${row['cost']:.4f},"
                    f" {row['seconds']:.0f} s{' - ' + row['error'] if row['error'] else ''}",
                    flush=True,
                )

            with ThreadPoolExecutor(max_workers=args.workers) as pool:
                list(pool.map(one, todo))
    rows = [row for path in sorted(runs.glob("*.jsonl")) for row in read_rows(path)]
    text = secrets.scrub(report.markdown(rows))
    written.write_text(text, encoding="utf-8")
    sys.stdout.buffer.write(text.encode("utf-8"))


if __name__ == "__main__":
    main()
