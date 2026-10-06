"""One run: a task's workspace in a scratch directory, the arm's context, a deepagents agent with
the compiler's tools on a filesystem confined to the workspace and no shell, the step and time
limits, the grade, and the row of metrics (specs/agent-harness/ R2, R4.1, R4.4).

Metrics come from LangChain callbacks rather than the final messages, so the calls of deepagents'
subagents, which never reach the main agent's state, are counted too.
"""

import difflib
import json
import os
import tempfile
import threading
import time
from collections import Counter
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, ClassVar

from langchain_core.callbacks import BaseCallbackHandler
from langchain_core.language_models import BaseChatModel
from langchain_core.messages import BaseMessage, HumanMessage, messages_to_dict
from langchain_core.outputs import LLMResult

from lotml_harness import ROOT
from lotml_harness.agent.bench import AgentTask
from lotml_harness.agent.grade import grade
from lotml_harness.agent.mcp import LotmlMcp, langchain_tools
from lotml_harness.experiments import variants
from lotml_harness.experiments.phase1 import Lotml

MODEL = "z-ai/glm-5.3-flash"
ARMS = ("agents", "reference")
STEPS = 60
"""Model calls a run may make, its subagents' included."""
SECONDS = 600
REQUEST_TIMEOUT = 180
"""Seconds one model call may take: the time limit is checked between steps, so this bounds by
how much a run can overshoot it."""
TRACES = ROOT / "harness" / "cache" / "agent"

SYSTEM = (
    "You are a coding agent working on a lotml project. The project's files are at `/`. Do the "
    "task you are given by editing the files, then answer with a one-paragraph summary of what you "
    "changed. You have no shell: use the file tools and the compiler's tools."
)


class Stop(Exception):
    """A limit was reached; `reason` is `steps` or `time`."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def require_key() -> str:
    key = os.environ.get("OPENROUTER_API_KEY")
    if not key:
        raise SystemExit("OPENROUTER_API_KEY is not set: export it to run the agent harness")
    return key


def openrouter(model: str = MODEL) -> BaseChatModel:
    from langchain_openrouter import ChatOpenRouter

    require_key()
    return ChatOpenRouter(
        model=model, temperature=0, request_timeout=REQUEST_TIMEOUT, max_retries=2
    )


@dataclass
class Meter(BaseCallbackHandler):
    """Model calls, tokens, cost and tool calls, from every model and tool the run touches. It
    raises `Stop` before a model call past a limit, so the handler must not have its errors
    swallowed."""

    raise_error: ClassVar[bool] = True
    steps: int = STEPS
    deadline: float = float("inf")
    model_calls: int = 0
    tokens: Counter = field(default_factory=Counter)
    cost: float = 0.0
    providers: Counter = field(default_factory=Counter)
    tools: Counter = field(default_factory=Counter)
    tool_errors: int = 0
    check_errors: int = 0
    running: dict = field(default_factory=dict)
    lock: threading.Lock = field(default_factory=threading.Lock)
    live: Callable[[str], None] | None = None
    """Where to say each model call and tool call as it happens, for watching a run."""

    def say(self, text: str) -> None:
        if self.live:
            self.live(text)

    def on_chat_model_start(self, *_: Any, **__: Any) -> None:
        with self.lock:
            if self.model_calls >= self.steps:
                raise Stop("steps")
            if time.monotonic() > self.deadline:
                raise Stop("time")
            self.model_calls += 1

    def on_llm_end(self, response: LLMResult, **_: Any) -> None:
        for generations in response.generations:
            for generation in generations:
                message = getattr(generation, "message", None)
                if message is None:
                    continue
                usage = getattr(message, "usage_metadata", None) or {}
                metadata = getattr(message, "response_metadata", None) or {}
                with self.lock:
                    self.tokens["input"] += usage.get("input_tokens", 0)
                    self.tokens["output"] += usage.get("output_tokens", 0)
                    details = usage.get("output_token_details") or {}
                    self.tokens["reasoning"] += details.get("reasoning", 0)
                    self.cost += float(metadata.get("cost") or 0.0)
                    if metadata.get("provider"):
                        self.providers[metadata["provider"]] += 1
                    calls, cost = self.model_calls, self.cost
                self.say(
                    f"model #{calls}: +{usage.get('input_tokens', 0)} in"
                    f" +{usage.get('output_tokens', 0)} out, ${cost:.4f} so far"
                )

    def on_tool_start(
        self, serialized: dict, input_str: str = "", *_: Any, run_id: Any = None, **__: Any
    ) -> None:
        name = (serialized or {}).get("name", "?")
        with self.lock:
            self.tools[name] += 1
            self.running[run_id] = name
        self.say(f"  {name} {' '.join(str(input_str).split())[:100]}")

    def on_tool_end(self, output: Any, *, run_id: Any = None, **_: Any) -> None:
        with self.lock:
            name = self.running.pop(run_id, None)
        status = getattr(output, "status", "success")
        content = getattr(output, "content", output)
        failed_check = name == "check" and reports_errors(str(content))
        with self.lock:
            if status == "error":
                self.tool_errors += 1
            if failed_check:
                self.check_errors += 1
        if status == "error":
            self.say(f"    {name} failed: {' '.join(str(content).split())[:100]}")
        elif failed_check:
            self.say("    check: errors")

    def on_tool_error(self, error: BaseException, *, run_id: Any = None, **_: Any) -> None:
        with self.lock:
            name = self.running.pop(run_id, None)
            self.tool_errors += 1
        self.say(f"    {name} failed: {str(error)[:100]}")


def reports_errors(check_output: str) -> bool:
    """Whether a `check` result names an error: its JSON summary counts one."""
    try:
        report = json.loads(check_output)
    except json.JSONDecodeError:
        return "error[" in check_output
    return (report.get("summary") or {}).get("errors", 0) > 0


def snapshot(workspace: Path) -> dict[str, str]:
    return {
        p.relative_to(workspace).as_posix(): p.read_text(encoding="utf-8", errors="replace")
        for p in sorted(workspace.rglob("*.lotml"))
    }


def lines_changed(before: dict[str, str], after: dict[str, str]) -> int:
    changed = 0
    for name in before.keys() | after.keys():
        diff = difflib.unified_diff(
            before.get(name, "").splitlines(), after.get(name, "").splitlines(), lineterm="", n=0
        )
        changed += sum(
            1 for line in diff if line[:1] in "+-" and not line.startswith(("+++", "---"))
        )
    return changed


def prepare(task: AgentTask, workspace: Path, arm: str, lotml: Lotml) -> tuple[str, list[str]]:
    """Lay the workspace and set the arm's context: the system prompt and the memory files."""
    task.lay(workspace)
    if arm == "agents":
        done = lotml.compiler(["init", "--harness", "none", "."], str(workspace))
        if done is None or done.returncode != 0:
            raise RuntimeError(f"lotml init failed: {done.stdout if done else 'timed out'}")
        return SYSTEM, ["/AGENTS.md"]
    if arm == "reference":
        return SYSTEM + "\n\n# The lotml language reference\n\n" + variants.reference_text("b"), []
    raise ValueError(f"arm {arm!r} is not one of {ARMS}")


def run(
    task: AgentTask,
    arm: str,
    attempt: int,
    model: BaseChatModel,
    model_name: str,
    lotml: Lotml | None = None,
    traces: Path = TRACES,
    live: Callable[[str], None] | None = None,
) -> dict:
    """Run the task once and return its row; the trace goes to `traces`."""
    from deepagents import create_deep_agent
    from deepagents.backends import FilesystemBackend

    lotml = lotml or Lotml()
    steps = task.steps if task.steps is not None else STEPS
    seconds = task.seconds if task.seconds is not None else SECONDS
    start = time.monotonic()
    meter = Meter(steps=steps, deadline=start + seconds, live=live)
    stopped, error = "done", None
    messages: list[BaseMessage] = []
    with tempfile.TemporaryDirectory(prefix="lotml-agent-") as scratch:
        workspace = Path(scratch) / "workspace"
        system, memory = prepare(task, workspace, arm, lotml)
        before = snapshot(workspace)
        with LotmlMcp(workspace, binary=lotml.binary) as server:
            agent = create_deep_agent(
                model=model,
                tools=langchain_tools(server),
                system_prompt=system,
                backend=FilesystemBackend(root_dir=workspace, virtual_mode=True),
                memory=memory or None,
            )
            config = {"callbacks": [meter], "recursion_limit": steps * 4 + 20}
            try:
                for state in agent.stream(
                    {"messages": [HumanMessage(task.prompt)]}, config, stream_mode="values"
                ):
                    messages = state.get("messages", messages)
            except Stop as limit:
                stopped = limit.reason
            except Exception as failure:  # noqa: BLE001 - a model or network failure ends this run only (R2.7)
                if type(failure).__name__ == "GraphRecursionError":
                    stopped = "steps"
                else:
                    stopped, error = "error", f"{type(failure).__name__}: {str(failure)[:500]}"
        seconds_taken = time.monotonic() - start
        after = snapshot(workspace)
        graded = grade(task, workspace, lotml)
    row = {
        "task": task.id,
        "kind": task.kind,
        "model": model_name,
        "arm": arm,
        "attempt": attempt,
        "outcome": "error" if error else graded.outcome,
        "hidden": [graded.passed, graded.total],
        "checks": graded.checks,
        "stopped": stopped,
        "model_calls": meter.model_calls,
        "tools": dict(sorted(meter.tools.items())),
        "tool_errors": meter.tool_errors,
        "check_errors": meter.check_errors,
        "tokens": {k: meter.tokens[k] for k in ("input", "output", "reasoning")},
        "cost": round(meter.cost, 6),
        "providers": dict(meter.providers),
        "seconds": round(seconds_taken, 1),
        "lines_changed": lines_changed(before, after),
        "failures": graded.failures[:5],
        "error": error,
    }
    trace = traces / model_name.replace("/", "__") / arm / f"{task.id}-{attempt}.json"
    trace.parent.mkdir(parents=True, exist_ok=True)
    trace.write_text(
        json.dumps({"row": row, "messages": messages_to_dict(messages), "final": after}, indent=1),
        encoding="utf-8",
    )
    return row


def error_row(task: AgentTask, arm: str, attempt: int, model_name: str, error: str) -> dict:
    """The row of a run that could not be set up or graded: an error, run again next time."""
    return {
        "task": task.id,
        "kind": task.kind,
        "model": model_name,
        "arm": arm,
        "attempt": attempt,
        "outcome": "error",
        "hidden": [0, 0],
        "checks": False,
        "stopped": "error",
        "model_calls": 0,
        "tools": {},
        "tool_errors": 0,
        "check_errors": 0,
        "tokens": {"input": 0, "output": 0, "reasoning": 0},
        "cost": 0.0,
        "providers": {},
        "seconds": 0.0,
        "lines_changed": 0,
        "failures": [],
        "error": error[:500],
    }
