"""Secrets kept out of what is committed (specs/trace-dataset/ R2.3, R3.7)."""

import json

import pytest

from lotml_harness.agent import secrets


@pytest.fixture
def keyed(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("OPENROUTER_API_KEY", "or-value-abcdef")
    monkeypatch.setenv("HF_TOKEN", "hftokenvalue1234")
    monkeypatch.setenv("SHORT_SECRET", "abc")
    monkeypatch.setenv("NOT_A_KEY_NAME", "visible-value-123")


def test_the_value_of_every_key_token_or_secret_variable_is_redacted(keyed):
    text = "failed with or-value-abcdef and hftokenvalue1234, not visible-value-123"
    assert secrets.scrub(text) == "failed with <redacted> and <redacted>, not visible-value-123"


def test_a_short_value_would_match_everything_and_is_left_alone(keyed):
    assert secrets.scrub("abc abc") == "abc abc"


@pytest.mark.parametrize(
    "shaped",
    [
        "sk-" + "a1B2_c3-" * 3,
        "ghp_" + "A" * 36,
        "hf_" + "b" * 34,
        "AKIA" + "ABCDEFGHIJKLMNOP",
        "Bearer " + "tok.en-" * 4,
        "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
    ],
    ids=["openai", "github", "huggingface", "aws", "bearer", "jwt"],
)
def test_key_shapes_are_redacted(shaped):
    assert secrets.scrub(f"x {shaped} y") == "x <redacted> y"
    assert secrets.holds_secret(shaped)


def test_scrubbing_a_row_reaches_every_string_inside_it(keyed):
    row = {"error": "401 or-value-abcdef", "failures": ["hftokenvalue1234"], "n": 3}
    assert secrets.scrub_value(row) == {
        "error": "401 <redacted>",
        "failures": ["<redacted>"],
        "n": 3,
    }
    assert json.loads(secrets.scrub(json.dumps(row)))["error"] == "401 <redacted>"


def test_clean_text_holds_no_secret(keyed):
    assert not secrets.holds_secret("fn f() -> int:\n    return 1\n")
    assert secrets.holds_secret("key or-value-abcdef")


def test_rows_and_the_report_are_scrubbed_before_they_are_written(keyed, tmp_path, monkeypatch):
    from lotml_harness.agent import __main__ as agent_main

    def failing(*_, **__):
        raise RuntimeError("401 for or-value-abcdef")

    monkeypatch.setattr(agent_main, "openrouter", lambda model: None)
    monkeypatch.setattr(agent_main, "run", failing)
    written = tmp_path / "agent.md"
    agent_main.main(["--task", "median-mode", "--arm", "agents"], runs=tmp_path, written=written)
    text = "".join(p.read_text(encoding="utf-8") for p in tmp_path.glob("*.jsonl"))
    assert "or-value-abcdef" not in text
    assert "401 for <redacted>" in text
    assert "or-value-abcdef" not in written.read_text(encoding="utf-8")
