"""The private Hugging Face repository the pipeline keeps its artifacts in
(specs/training-pipeline/ R2.1, R2.2, R2.4)."""

import json
from pathlib import Path
from types import SimpleNamespace

import pytest

from lotml_harness.guide import hub
from lotml_harness.guide.evaluate import guard
from lotml_harness.guide.hub import Hub, HubError

TOKEN = "hf_test_0123456789abcdefghijklmnop"  # noqa: S105 - a fake token


class Api:
    """An HfApi that keeps its calls and a repository's files in memory."""

    def __init__(self, private: bool = True, files: tuple[str, ...] = ()):
        self.private, self.listed, self.calls = private, list(files), []

    def create_repo(self, repo, **kwargs):
        self.calls.append(("create_repo", repo, kwargs))

    def repo_info(self, repo, **kwargs):
        return SimpleNamespace(private=self.private)

    def upload_folder(self, **kwargs):
        self.calls.append(("upload_folder", kwargs["path_in_repo"], kwargs))
        self.listed += [
            f"{kwargs['path_in_repo']}/{p.name}" for p in Path(kwargs["folder_path"]).iterdir()
        ]

    def upload_file(self, **kwargs):
        self.calls.append(("upload_file", kwargs["path_in_repo"], kwargs))
        self.listed.append(kwargs["path_in_repo"])
        self.uploaded = Path(kwargs["path_or_fileobj"]).read_text(encoding="utf-8")

    def list_repo_files(self, repo, **kwargs):
        return list(self.listed)


@pytest.fixture(autouse=True)
def hf_token(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("HF_TOKEN", TOKEN)


def records(directory: Path) -> Path:
    directory.mkdir()
    meta = {"problem": "humaneval/0", "split": "train"}
    (directory / "train.jsonl").write_text(json.dumps({"meta": meta}) + "\n", encoding="utf-8")
    (directory / "validation.jsonl").write_text(
        json.dumps({"meta": meta | {"split": "validation"}}) + "\n", encoding="utf-8"
    )
    return directory


def test_a_public_repository_gets_nothing_written(tmp_path: Path):
    api = Api(private=False)
    with pytest.raises(HubError, match="not private"):
        Hub("me/guide", api, lambda **_: None).put(tmp_path, "x", "m")
    assert [c[0] for c in api.calls] == ["create_repo"]
    assert api.calls[0][2]["private"] is True


def test_records_are_uploaded_once_under_their_digest(tmp_path: Path):
    api = Api()
    found = records(tmp_path / "records")
    store = Hub("me/guide", api, lambda **_: None)
    first = store.put_records(found)
    assert first == hub.digest(found) == guard(found)
    assert store.put_records(found) == first
    uploads = [c for c in api.calls if c[0] == "upload_folder"]
    assert [u[1] for u in uploads] == [f"records/{first}"]
    assert uploads[0][2]["token"] == TOKEN
    assert uploads[0][2]["allow_patterns"] == ["*.jsonl"], "only the records the digest covers"


def test_a_run_s_lineage_names_its_records_commit_and_inputs():
    api = Api()
    Hub("me/guide", api, lambda **_: None).put_lineage("r2", "abc", {"rl": "r1"}, "c0ffee")
    assert api.calls[-1][1] == "runs/r2/run.json"
    assert json.loads(api.uploaded) == {
        "run": "r2",
        "records": "abc",
        "commit": "c0ffee",
        "inputs": {"rl": "r1"},
    }


def test_get_downloads_a_directory_by_pattern(tmp_path: Path):
    asked = {}
    store = Hub("me/guide", Api(), lambda **kwargs: asked.update(kwargs))
    assert store.get("runs/r1/sft", tmp_path) == tmp_path / "runs/r1/sft"
    assert asked["allow_patterns"] == ["runs/r1/sft", "runs/r1/sft/**"]
    assert (asked["repo_id"], asked["token"]) == ("me/guide", TOKEN)


def test_no_token_no_write(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    monkeypatch.delenv("HF_TOKEN")
    api = Api()
    with pytest.raises(HubError, match="HF_TOKEN is not set"):
        Hub("me/guide", api, lambda **_: None).put(tmp_path, "x", "m")
    assert api.calls == []
