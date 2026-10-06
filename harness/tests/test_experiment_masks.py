"""Type masks for open models, by the line: each line an open model writes is checked as a prefix
by the compiler, and one the compiler says can no longer complete is drawn again."""

import io
import json
import urllib.error

import pytest

from lotml_harness.experiments import masks
from lotml_harness.experiments.phase1 import Lotml


def test_the_raw_prompt_follows_each_family_s_chat_template():
    qwen = masks.raw_prompt("qwen2.5-coder:7b", "SYS", "USER", "```lotml\n")
    assert qwen == (
        "<|im_start|>system\nSYS<|im_end|>\n<|im_start|>user\nUSER<|im_end|>\n"
        "<|im_start|>assistant\n```lotml\n"
    )
    llama = masks.raw_prompt("llama3.1:8b", "SYS", "USER", "```lotml\n")
    assert llama.startswith(
        "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nSYS<|eot_id|>"
    )
    assert llama.endswith("<|start_header_id|>assistant<|end_header_id|>\n\n```lotml\n")
    hosted = masks.raw_prompt("meta-llama/llama-3.1-8b-instruct", "SYS", "USER", "```lotml\n")
    assert hosted == llama, "a model named by its OpenRouter id keeps its family's template"
    glm = masks.raw_prompt("z-ai/glm-5.3-flash", "SYS", "USER", "```lotml\n")
    assert glm == (
        "[gMASK]<sop><|system|>Reasoning Effort: Low<|system|>SYS<|user|>USER"
        "<|assistant|><think></think>```lotml\n"
    ), "GLM answers with its thinking already closed, as in a past turn without reasoning"


def test_a_model_is_named_and_served_by_its_id():
    assert masks.label("qwen2.5-coder:7b") == "qwen2.5-coder-7b"
    assert masks.label("meta-llama/llama-3.1-8b-instruct") == "llama-3.1-8b-instruct"
    assert masks.served("qwen2.5-coder:7b") == "ollama"
    assert masks.served("meta-llama/llama-3.1-8b-instruct") == "openrouter/DeepInfra"
    assert masks.served("z-ai/glm-5.3-flash") == "openrouter/Parasail"
    with pytest.raises(ValueError, match="no provider"):
        masks.served("someone/unchecked-model")


def test_openrouter_is_asked_for_a_raw_completion_from_one_provider():
    body = masks.openrouter_body("z-ai/glm-5.3-flash", "P", 0.8, 2, ["\n"], 200)
    assert body == {
        "model": "z-ai/glm-5.3-flash",
        "prompt": "P",
        "temperature": 0.8,
        "seed": 2,
        "stop": ["\n"],
        "max_tokens": 200,
        "provider": {"order": ["Parasail"], "allow_fallbacks": False},
    }


class Replies:
    """OpenRouter answering from a script: a body to read, or an HTTP status to fail with."""

    def __init__(self, *replies):
        self.replies = list(replies)
        self.calls = 0

    def open(self, request, timeout):
        self.calls += 1
        reply = self.replies.pop(0)
        if isinstance(reply, int):
            raise urllib.error.HTTPError(request.full_url, reply, "status", {}, None)
        return io.BytesIO(json.dumps({"choices": [reply]}).encode())


@pytest.fixture
def hosted(monkeypatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "test-key")
    monkeypatch.setattr(masks.time, "sleep", lambda seconds: None)

    def serve(*replies):
        replies = Replies(*replies)
        monkeypatch.setattr(masks, "OPENER", replies)
        return replies

    return serve


def ask():
    return masks.openrouter("z-ai/glm-5.3-flash", "P", 0.0, 0, ["\n"], 10)


def test_openrouter_retries_a_rate_limit_and_a_server_error(hosted):
    replies = hosted(429, 503, {"text": "    return 1\n"})
    assert ask() == "    return 1\n"
    assert replies.calls == 3


def test_openrouter_fails_at_once_on_a_refused_request(hosted):
    replies = hosted(401, {"text": "never read"})
    with pytest.raises(masks.ModelError, match="401"):
        ask()
    assert replies.calls == 1


def test_a_reply_carrying_reasoning_means_the_prompt_was_not_taken_raw(hosted):
    replies = hosted({"text": "", "reasoning": "Simple."}, {"text": "never read"})
    with pytest.raises(masks.ModelError, match="wrapped the raw prompt"):
        ask()
    assert replies.calls == 1


def test_the_key_never_follows_a_redirect():
    handler = masks.NoRedirect()
    assert handler.redirect_request(None, None, 302, "Found", {}, "https://elsewhere/") is None


def test_openrouter_without_a_key_is_a_model_error(monkeypatch):
    monkeypatch.delenv("OPENROUTER_API_KEY", raising=False)
    with pytest.raises(masks.ModelError, match="OPENROUTER_API_KEY"):
        masks.openrouter("meta-llama/llama-3.1-8b-instruct", "P", 0.0, 0, ["\n"], 10)


class Lines:
    """A model answering line by line from a script, keyed by how many lines it has accepted."""

    def __init__(self, script: dict[int, list[str]]):
        self.script = {k: list(v) for k, v in script.items()}
        self.calls = []

    def __call__(self, prompt: str, temperature: float, seed: int) -> str:
        accepted = prompt.count("\n") - PREFIX_LINES
        self.calls.append((accepted, temperature))
        return self.script[accepted].pop(0)


PREFIX_LINES = masks.raw_prompt("qwen2.5-coder:7b", "s", "u", "```lotml\n").count("\n")


def verdicts(errors: set[str]):
    def check(code: str) -> str:
        return "error" if any(e in code for e in errors) else "completable"

    return check


def test_a_line_the_compiler_refuses_is_drawn_again_hotter():
    model = Lines(
        {
            0: ["fn f(x: int) -> int:"],
            1: ["    x = x + 1", "    return x + 1"],
            2: ["```"],
        }
    )
    decoded = masks.masked("qwen2.5-coder:7b", "s", "u", model, verdicts({"x = x"}))
    assert decoded.code == "fn f(x: int) -> int:\n    return x + 1\n"
    assert decoded.rejected == 1
    assert [t for _, t in model.calls] == [0.0, 0.0, masks.TEMPERATURE, 0.0]


def test_a_line_refused_every_time_is_kept_and_the_decoding_goes_on():
    model = Lines({0: ["bad"] * (masks.RETRIES + 1), 1: ["```"]})
    decoded = masks.masked("qwen2.5-coder:7b", "s", "u", model, verdicts({"bad"}))
    assert decoded.code == "bad\n" and decoded.rejected == masks.RETRIES


def test_decoding_stops_at_the_fence_at_two_blank_lines_or_at_the_cap():
    blank = Lines({0: ["fn f():"], 1: ["    pass"], 2: [""], 3: [""]})
    assert (
        masks.masked("qwen2.5-coder:7b", "s", "u", blank, verdicts(set())).code
        == "fn f():\n    pass\n"
    )
    endless = Lines({i: ["x"] for i in range(masks.MAX_LINES + 5)})
    assert (
        masks.masked("qwen2.5-coder:7b", "s", "u", endless, verdicts(set())).code.count("\n")
        == masks.MAX_LINES
    )


def test_the_compiler_says_whether_a_prefix_can_still_complete():
    check = masks.prefix_checker(Lotml())
    assert check('fn f() -> int:\n    return "a"\n') == "error"
    assert check("fn f(x: int) -> int:\n    y = x + 1\n") != "error", "a body not yet returning"
    assert check("fn f(x: int) -> int:\n    y = (x +\n") != "error", "a bracket still open"


def test_the_summary_pairs_the_free_and_the_masked_answer():
    def row(task, arm, passed, outcome, rejected):
        return {
            "model": "m",
            "task": task,
            "arm": arm,
            "passed": passed,
            "outcome": outcome,
            "rejected": rejected,
        }

    rows = [
        row("a", "free", False, "does not check", 0),
        row("a", "masked", True, "pass", 2),
        row("b", "free", True, "pass", 0),
        row("b", "masked", True, "pass", 0),
    ]
    rows[0]["served"] = "openrouter/X"
    s = masks.summarize(rows)["m"]
    assert s["served"] == "openrouter/X"
    assert "| m | openrouter/X | 2 |" in masks.markdown({"m": s}, 2)
    assert masks.summarize(rows[1:2])["m"]["served"] == "ollama", "a run from before the field"
    assert s["pairs"] == 2
    assert s["passed"] == {"free": 1, "masked": 2}
    assert s["refused"] == {"free": 1, "masked": 0}
    assert (s["only_free"], s["only_masked"]) == (0, 1)
    assert s["rejected"] == 2
