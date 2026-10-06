"""The licence registry: what may be exported, default deny (specs/trace-dataset/ R1.1-R1.6)."""

from pathlib import Path

import pytest

from lotml_harness.agent import registry
from lotml_harness.agent.registry import RegistryError

PERMITTED = """\
[sources.bench]
licence = "MIT"
training = "permitted"
evidence = "LICENSE"
checked = 2026-10-06
notice = "Copyright (c) the lotml authors"

[sources.multipl-e-humaneval]
licence = "BSD-3-Clause with a machine-learning restriction"
training = "forbidden"
evidence = "harness/results/NOTICE.md"
checked = 2026-10-06
notice = ""

[models."m/x"]
terms = "https://example.com/terms"
training = "permitted"
evidence = "https://example.com/terms#outputs"
checked = 2026-10-06
notice = "outputs may be used"

[models."m/x".providers."Prov A"]
training = "permitted"
evidence = "https://example.com/a"
checked = 2026-10-06
notice = "ok"

[models."m/x".providers."Prov B"]
training = "unknown"
evidence = ""
checked = ""
notice = ""
"""


def read(text: str, tmp_path: Path) -> registry.Registry:
    path = tmp_path / "licences.toml"
    path.write_text(text, encoding="utf-8")
    return registry.read(path)


def test_a_run_is_exported_only_when_its_source_model_and_every_provider_permit(tmp_path):
    found = read(PERMITTED, tmp_path)
    assert found.decide("bench", "m/x", ["Prov A"]) == (True, None)
    assert found.decide("bench", "m/x", ["Prov A", "Prov B"]) == (
        False,
        "provider Prov B not permitted",
    )
    assert found.decide("multipl-e-humaneval", "m/x", ["Prov A"]) == (
        False,
        "source multipl-e-humaneval not permitted",
    )


@pytest.mark.parametrize(
    ("source", "model", "providers", "reason"),
    [
        ("nowhere", "m/x", ["Prov A"], "source nowhere has no entry"),
        ("bench", "other/y", ["Prov A"], "model other/y has no entry"),
        ("bench", "m/x", ["Prov C"], "provider Prov C has no entry"),
        ("bench", "m/x", [], "no provider named"),
    ],
)
def test_an_entry_missing_or_no_provider_named_denies_the_run(
    tmp_path, source, model, providers, reason
):
    assert read(PERMITTED, tmp_path).decide(source, model, providers) == (False, reason)


@pytest.mark.parametrize(
    "change",
    [
        (
            'training = "permitted"\nevidence = "LICENSE"',
            'training = "Permitted"\nevidence = "LICENSE"',
        ),
        ('training = "permitted"\nevidence = "LICENSE"', 'training = true\nevidence = "LICENSE"'),
        ('evidence = "LICENSE"', 'evidence = "no/such/file"'),
        ('evidence = "LICENSE"', 'evidence = "http://example.com"'),
        ('evidence = "LICENSE"', 'evidence = ""'),
        ('notice = "Copyright (c) the lotml authors"', 'notice = ""'),
        ('checked = 2026-10-06\nnotice = "Copyright', 'checked = ""\nnotice = "Copyright'),
    ],
    ids=[
        "capitalised",
        "boolean",
        "missing path",
        "plain http",
        "no evidence",
        "no notice",
        "no date",
    ],
)
def test_only_an_exact_permitted_with_evidence_a_date_and_a_notice_permits(tmp_path, change):
    found = read(PERMITTED.replace(*change, 1), tmp_path)
    assert found.decide("bench", "m/x", ["Prov A"]) == (False, "source bench not permitted")


@pytest.mark.parametrize(
    "broken",
    [
        PERMITTED + "\nnot = [toml\n",
        PERMITTED + '\n[datasets.x]\nlicence = "MIT"\n',
        PERMITTED.replace('notice = "ok"', 'notice = "ok"\nreviewer = "me"'),
        PERMITTED + '\n[sources.bench]\nlicence = "MIT"\n',
    ],
    ids=["unparsable", "unknown table", "unknown field", "duplicate entry"],
)
def test_a_malformed_registry_stops_the_export(tmp_path, broken):
    with pytest.raises(RegistryError):
        read(broken, tmp_path)


def test_the_committed_registry_reads_and_denies_the_unverified_model():
    found = registry.read(registry.REGISTRY)
    assert found.decide("bench", "z-ai/glm-5.3-flash", ["Parasail"])[0] is False
    assert found.permits_source("bench") and found.permits_source("humaneval-original")
    assert not found.permits_source("multipl-e-humaneval")


def traced(tmp_path: Path, model: str, arm: str, task: str, attempt: int, row: dict) -> Path:
    path = tmp_path / model.replace("/", "__") / arm / f"{task}-{attempt}.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def row(task: str = "median-mode", source: str | None = "bench", **changes) -> dict:
    found = {
        "task": task,
        "model": "m/x",
        "arm": "agents",
        "attempt": 0,
        "providers": {"Prov A": 2},
    }
    if source is not None:
        found["source"] = source
    return found | changes


def test_a_run_s_identity_comes_from_its_row_and_its_source_from_the_task_id(tmp_path):
    path = traced(tmp_path, "m/x", "agents", "median-mode", 0, row())
    assert registry.identity({"row": row()}, path, tmp_path) == (
        {"task": "median-mode", "source": "bench", "model": "m/x", "arm": "agents",
         "providers": ["Prov A"]},
        None,
    )  # fmt: skip
    humaneval = row("humaneval-3", "humaneval-original")
    path = traced(tmp_path, "m/x", "agents", "humaneval-3", 0, humaneval)
    assert (
        registry.identity({"row": humaneval}, path, tmp_path)[0]["source"] == "humaneval-original"
    )


@pytest.mark.parametrize(
    ("found", "reason"),
    [
        (row(source=None), "no source"),
        (row(source="humaneval-original"), "source disagrees with the task"),
        (row(arm="reference"), "path disagrees with the row"),
        (row(model="other/y"), "path disagrees with the row"),
    ],
)
def test_a_trace_whose_path_or_source_disagrees_is_rejected(tmp_path, found, reason):
    path = traced(tmp_path, "m/x", "agents", "median-mode", 0, found)
    assert registry.identity({"row": found}, path, tmp_path) == (None, reason)
