import json

from lotml_harness.experiments import tokens
from lotml_harness.experiments.models import Completion


class FakeClient:
    """Counts one token per character of the user prompt, plus a fixed overhead."""

    def __init__(self):
        self.prompts = []

    def complete(self, system, user):
        self.prompts.append(user)
        return Completion(text="OK", input_tokens=100 + len(user))


def test_the_cli_counter_subtracts_the_empty_message_once():
    client = FakeClient()
    count = tokens.CliCounter("haiku", client=client)
    assert count.count("abcd") == 4
    assert count.count("xy") == 2
    assert len(client.prompts) == 3
    assert all(p.startswith("Count nothing") for p in client.prompts)


def test_counter_prefers_the_api_when_a_key_is_set(monkeypatch):
    monkeypatch.setenv("ANTHROPIC_API_KEY", "k")
    assert isinstance(tokens.counter("haiku"), tokens.ApiCounter)
    monkeypatch.delenv("ANTHROPIC_API_KEY")
    assert isinstance(tokens.counter("haiku"), tokens.CliCounter)


def test_the_api_counter_posts_the_text_and_reads_input_tokens(monkeypatch):
    seen = {}

    class Response:
        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self):
            return b'{"input_tokens": 17}'

    def urlopen(request, timeout):
        seen["body"] = json.loads(request.data)
        seen["headers"] = dict(request.header_items())
        return Response()

    monkeypatch.setattr(tokens.urllib.request, "urlopen", urlopen)
    assert tokens.ApiCounter("claude-haiku-4-5", "secret").count("hello") == 17
    assert seen["body"]["messages"][0]["content"] == "hello"
    assert seen["headers"]["X-api-key"] == "secret"


def test_the_corpus_has_the_three_forms_of_every_paired_task():
    tasks = tokens.corpus()
    assert len(tasks) == 12
    assert all(set(forms) == set(tokens.FORMS) for forms in tasks.values())


def test_generated_code_is_paired_per_model(tmp_path):
    rows = [
        {"model": "m", "task": "t1", "variant": "a", "code": "A1\n"},
        {"model": "m", "task": "t1", "variant": "b", "code": "B1\n"},
        {"model": "m", "task": "t2", "variant": "a", "code": "A2\n"},
        {"model": "m", "task": "t3", "variant": "b", "error": "quota"},
    ]
    (tmp_path / "m.jsonl").write_text("\n".join(json.dumps(r) for r in rows) + "\n")
    assert tokens.generated(tmp_path) == {("m", "a"): "A1\n", ("m", "b"): "B1\n"}


def test_measure_and_report(monkeypatch):
    class Length(tokens.Counter):
        name = "len"

        def count(self, text):
            return len(text)

    monkeypatch.setattr(tokens, "generated", lambda: {("w", "a"): "aaaa", ("w", "b"): "bb"})
    measurement = tokens.measure(Length())
    assert len(measurement.corpus) == 12 and measurement.generated[("w", "b")] == 2
    text = tokens.markdown({"len": measurement})
    assert "| len | w | 4 | 2 | 0.500 |" in text
    assert "| len | claude -p usage difference |" in text
