"""The compiler's MCP tools for the agent: a client of `lotml mcp` over its newline-delimited
JSON-RPC, and each tool it lists as a LangChain tool.

One server per run, rooted at the run's workspace, started with `child_environment()`: the server's
`test` tool runs the agent's code, and that code must never see the OpenRouter key.
"""

import itertools
import json
import queue
import subprocess
import threading
from pathlib import Path

from langchain_core.tools import BaseTool, StructuredTool, ToolException

from lotml_harness.execute import child_environment
from lotml_harness.experiments.phase1 import COMPILER

PROTOCOL = "2025-11-25"
TIMEOUT = 90
"""Seconds a call may take: the server stops the `test` tool at 60."""


class McpError(RuntimeError):
    """The server answered with an error, or did not answer."""


class LotmlMcp:
    def __init__(self, root: Path, binary: Path = COMPILER, timeout: float = TIMEOUT):
        self.timeout = timeout
        self.ids = itertools.count(1)
        self.lock = threading.Lock()
        self.lines: queue.Queue[str | None] = queue.Queue()
        self.process = subprocess.Popen(  # noqa: S603
            [str(binary), "mcp", "--root", str(root)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            env=child_environment(),
            cwd=root,
        )
        threading.Thread(target=self._read, daemon=True).start()
        self.instructions = self.request(
            "initialize",
            {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {"name": "lotml-agent-harness", "version": "0.1.0"},
            },
        ).get("instructions", "")
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def _read(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def _send(self, message: dict) -> None:
        assert self.process.stdin is not None
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method: str, params: dict) -> dict:
        with self.lock:
            id_ = next(self.ids)
            try:
                self._send({"jsonrpc": "2.0", "id": id_, "method": method, "params": params})
            except OSError as error:
                raise McpError(f"the server is gone: {error}") from None
            while True:
                try:
                    line = self.lines.get(timeout=self.timeout)
                except queue.Empty:
                    raise McpError(f"`{method}` had no answer in {self.timeout} s") from None
                if line is None:
                    raise McpError("the server closed its output")
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if not isinstance(message, dict) or message.get("id") != id_:
                    continue
                if "error" in message:
                    raise McpError(message["error"].get("message", str(message["error"])))
                return message["result"]

    def tools(self) -> list[dict]:
        return self.request("tools/list", {})["tools"]

    def call(self, name: str, arguments: dict) -> tuple[str, bool]:
        """The tool's text, and whether the tool reported it as an error."""
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        text = "\n".join(c.get("text", "") for c in result.get("content", []))
        return text, bool(result.get("isError"))

    def close(self) -> None:
        if self.process.poll() is None:
            if self.process.stdin:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()

    def __enter__(self) -> "LotmlMcp":
        return self

    def __exit__(self, *_) -> None:
        self.close()


def relative(arguments: dict) -> dict:
    """Paths as the server takes them. The agent's file tools show the workspace as `/`, so it
    names `/stats.lotml`; the server reads an absolute path as one outside the project."""
    out = dict(arguments)
    if isinstance(out.get("path"), str):
        out["path"] = out["path"].lstrip("/") or "."
    if isinstance(out.get("paths"), list):
        out["paths"] = [p.lstrip("/") or "." if isinstance(p, str) else p for p in out["paths"]]
    return out


def langchain_tools(server: LotmlMcp) -> list[BaseTool]:
    """Every tool the server lists, under its own name; an error it reports becomes the tool's
    error, so the agent's trace marks the call as failed."""

    def bind(name: str):
        def run(**arguments) -> str:
            try:
                text, failed = server.call(name, relative(arguments))
            except McpError as error:
                raise ToolException(str(error)) from None
            if failed:
                raise ToolException(text)
            return text

        return run

    return [
        StructuredTool(
            name=spec["name"],
            description=spec.get("description", ""),
            args_schema=spec.get("inputSchema") or {"type": "object", "properties": {}},
            func=bind(spec["name"]),
            handle_tool_error=True,
        )
        for spec in server.tools()
    ]
