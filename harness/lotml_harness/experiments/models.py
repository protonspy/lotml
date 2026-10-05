"""The models the experiments ask: Claude through the `claude` CLI, open models through Ollama.

`ClaudeCli` runs `claude -p` headless in a scratch directory with no tools, no settings, no
MCP servers and our own system prompt, so nothing of this repository reaches the model.
`Ollama` asks a local server, greedily. Both report the tokens the model read and wrote.
"""

import hashlib
import json
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path


class ModelError(RuntimeError):
    """A completion that did not come back: a quota, a crash, a refused connection."""


@dataclass
class Completion:
    text: str
    input_tokens: int = 0
    output_tokens: int = 0
    seconds: float = 0.0


class Model:
    def __init__(self, name: str, family: str):
        self.name = name
        self.family = family

    def complete(self, system: str, user: str) -> Completion:
        raise NotImplementedError

    def chat(self, system: str, messages: list[dict[str, str]]) -> Completion:
        """A conversation's next answer; a model without turns reads it as one transcript."""
        if len(messages) == 1:
            return self.complete(system, messages[0]["content"])
        transcript = "\n\n".join(
            f"<{m['role']}>\n{m['content']}\n</{m['role']}>" for m in messages[:-1]
        )
        return self.complete(
            system, f"Our conversation so far:\n\n{transcript}\n\n{messages[-1]['content']}"
        )


class ClaudeCli(Model):
    """A Claude model through `claude -p`, isolated from this workspace."""

    def __init__(self, model: str, workdir: Path | None = None, timeout: float = 600):
        super().__init__(name=f"claude-{model}", family="Claude")
        self.model = model
        self.workdir = workdir or Path(tempfile.mkdtemp(prefix="lotml-claude-"))
        self.timeout = timeout

    def command(self, system: str) -> list[str]:
        digest = hashlib.sha256(system.encode()).hexdigest()[:16]
        prompt = self.workdir / f"system-{digest}.md"
        if not prompt.exists():
            prompt.write_text(system, encoding="utf-8")
        return [
            "claude",
            "-p",
            "--model",
            self.model,
            "--tools",
            "",
            "--strict-mcp-config",
            "--setting-sources",
            "",
            "--disable-slash-commands",
            "--no-session-persistence",
            "--system-prompt-file",
            str(prompt),
            "--output-format",
            "json",
        ]

    def complete(self, system: str, user: str) -> Completion:
        start = time.perf_counter()
        try:
            child = subprocess.run(  # noqa: S603
                self.command(system),
                input=user,
                capture_output=True,
                text=True,
                encoding="utf-8",
                cwd=self.workdir,
                timeout=self.timeout,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ModelError(f"claude did not answer: {error}") from None
        if child.returncode != 0:
            reason = (child.stderr or child.stdout).strip()[-300:]
            raise ModelError(f"claude exited {child.returncode}: {reason}")
        try:
            payload = json.loads(child.stdout)
        except json.JSONDecodeError:
            raise ModelError(f"claude printed no JSON: {child.stdout[:200]}") from None
        if payload.get("is_error"):
            raise ModelError(f"claude reported an error: {str(payload.get('result'))[:300]}")
        usage = payload.get("usage", {})
        read = sum(
            usage.get(key, 0)
            for key in ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")
        )
        return Completion(
            text=payload.get("result", ""),
            input_tokens=read,
            output_tokens=usage.get("output_tokens", 0),
            seconds=time.perf_counter() - start,
        )


class Ollama(Model):
    """An open model served by a local Ollama, decoding greedily."""

    def __init__(
        self,
        model: str,
        family: str,
        host: str = "http://localhost:11434",
        context: int = 16384,
        timeout: float = 900,
    ):
        super().__init__(name=model.replace(":", "-"), family=family)
        self.model = model
        self.host = host
        self.context = context
        self.timeout = timeout

    def complete(self, system: str, user: str) -> Completion:
        return self.chat(system, [{"role": "user", "content": user}])

    def chat(self, system: str, messages: list[dict[str, str]]) -> Completion:
        body = {
            "model": self.model,
            "stream": False,
            "messages": [{"role": "system", "content": system}, *messages],
            "options": {"temperature": 0, "num_ctx": self.context, "num_predict": 2048},
        }
        request = urllib.request.Request(  # noqa: S310
            f"{self.host}/api/chat",
            data=json.dumps(body).encode(),
            headers={"Content-Type": "application/json"},
        )
        start = time.perf_counter()
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:  # noqa: S310
                payload = json.loads(response.read())
        except (OSError, urllib.error.URLError, json.JSONDecodeError) as error:
            raise ModelError(f"ollama did not answer: {error}") from None
        return Completion(
            text=payload.get("message", {}).get("content", ""),
            input_tokens=payload.get("prompt_eval_count", 0),
            output_tokens=payload.get("eval_count", 0),
            seconds=time.perf_counter() - start,
        )


FAMILIES = {"qwen": "Qwen", "llama": "Llama", "gemma": "Gemma", "deepseek": "DeepSeek"}


def from_spec(spec: str) -> Model:
    """`claude:haiku` or `ollama:qwen2.5-coder:7b`."""
    provider, _, model = spec.partition(":")
    if provider == "claude":
        return ClaudeCli(model)
    if provider == "ollama":
        family = next((f for key, f in FAMILIES.items() if model.startswith(key)), model)
        return Ollama(model, family=family)
    raise ValueError(f"unknown model provider in `{spec}`")
