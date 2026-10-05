import json
import subprocess

import pytest

from lotml_harness.experiments import models


def test_claude_cli_runs_isolated_headless_and_reads_the_json(monkeypatch, tmp_path):
    seen = {}

    def fake_run(command, **options):
        seen["command"] = command
        seen["options"] = options
        payload = {
            "result": "```lotml\nfn f():\n    pass\n```",
            "is_error": False,
            "usage": {
                "input_tokens": 100,
                "cache_creation_input_tokens": 20,
                "cache_read_input_tokens": 3,
                "output_tokens": 7,
            },
        }
        return subprocess.CompletedProcess(command, 0, json.dumps(payload), "")

    monkeypatch.setattr(models.subprocess, "run", fake_run)
    model = models.ClaudeCli("haiku", workdir=tmp_path)
    completion = model.complete("system text", "user text")
    assert completion.text.startswith("```lotml")
    assert (completion.input_tokens, completion.output_tokens) == (123, 7)
    command = seen["command"]
    for flag in ("-p", "--tools", "--system-prompt", "--setting-sources", "--strict-mcp-config"):
        assert flag in command
    assert command[command.index("--model") + 1] == "haiku"
    assert seen["options"]["input"] == "user text"
    assert seen["options"]["cwd"] == tmp_path
    assert model.family == "Claude" and model.name == "claude-haiku"


@pytest.mark.parametrize(
    ("returncode", "stdout"),
    [(1, ""), (0, "not json"), (0, json.dumps({"is_error": True, "result": "quota"}))],
)
def test_claude_cli_failures_raise_model_error(monkeypatch, tmp_path, returncode, stdout):
    def fake_run(command, **options):
        return subprocess.CompletedProcess(command, returncode, stdout, "boom")

    monkeypatch.setattr(models.subprocess, "run", fake_run)
    with pytest.raises(models.ModelError):
        models.ClaudeCli("haiku", workdir=tmp_path).complete("s", "u")


def test_ollama_posts_a_greedy_chat_and_reads_the_counts(monkeypatch):
    seen = {}

    class Response:
        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self):
            return json.dumps(
                {"message": {"content": "answer"}, "prompt_eval_count": 50, "eval_count": 9}
            ).encode()

    def urlopen(request, timeout):
        seen["url"] = request.full_url
        seen["body"] = json.loads(request.data)
        return Response()

    monkeypatch.setattr(models.urllib.request, "urlopen", urlopen)
    model = models.Ollama("qwen2.5-coder:7b", family="Qwen")
    completion = model.complete("sys", "usr")
    assert completion.text == "answer" and completion.input_tokens == 50
    body = seen["body"]
    assert seen["url"].endswith("/api/chat")
    assert body["model"] == "qwen2.5-coder:7b" and body["stream"] is False
    assert body["options"]["temperature"] == 0 and body["options"]["num_ctx"] >= 8192
    assert [m["role"] for m in body["messages"]] == ["system", "user"]
    assert model.name == "qwen2.5-coder-7b"


def test_ollama_network_errors_raise_model_error(monkeypatch):
    def urlopen(request, timeout):
        raise OSError("connection refused")

    monkeypatch.setattr(models.urllib.request, "urlopen", urlopen)
    with pytest.raises(models.ModelError):
        models.Ollama("x", family="X").complete("s", "u")


@pytest.mark.parametrize(
    ("spec", "kind", "family"),
    [
        ("claude:haiku", models.ClaudeCli, "Claude"),
        ("ollama:llama3.1:8b", models.Ollama, "Llama"),
        ("ollama:gemma3:12b", models.Ollama, "Gemma"),
        ("ollama:qwen2.5-coder:7b", models.Ollama, "Qwen"),
    ],
)
def test_models_are_named_by_a_spec(spec, kind, family):
    model = models.from_spec(spec)
    assert isinstance(model, kind) and model.family == family
