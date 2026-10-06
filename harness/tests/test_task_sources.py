import json
from hashlib import sha256

import pytest

from lotml_harness.tasks import Case, Task, build, sources
from lotml_harness.tasks.types import Prim


@pytest.mark.parametrize(
    ("source", "filename", "expected"),
    [
        ("humaneval", "HumanEval_0_has_close_elements.py", "humaneval/0"),
        ("mbpp", "mbpp_100_next_smallest_palindrome.py", "mbpp/100"),
        ("mbpp", "README.md", None),
    ],
)
def test_task_ids_come_from_multipl_e_filenames(source, filename, expected):
    assert sources.task_id(source, filename) == expected


def test_download_saves_once_and_reuses_the_file(tmp_path, monkeypatch):
    calls = []

    class Response:
        """A body of b"data" that reads in pieces of at most `size` bytes, then ends."""

        def __init__(self):
            self.left = b"data"

        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

        def read(self, size=-1):
            piece = self.left if size < 0 else self.left[:size]
            self.left = self.left[len(piece) :]
            return piece

    def urlopen(url, timeout):
        calls.append(url)
        return Response()

    monkeypatch.setattr(sources.urllib.request, "urlopen", urlopen)
    target = tmp_path / "a" / "b.txt"
    assert sources.download("https://x/b.txt", target).read_bytes() == b"data"
    sources.download("https://x/b.txt", target)
    assert calls == ["https://x/b.txt"]
    with pytest.raises(ValueError, match="larger than 3 bytes"):
        sources.download("https://x/c.txt", tmp_path / "c.txt", limit=3)
    assert not (tmp_path / "c.txt").exists()


class Body:
    """A response whose body reads in pieces of at most `size` bytes, then ends."""

    def __init__(self, body: bytes):
        self.left = body

    def __enter__(self):
        return self

    def __exit__(self, *_):
        return False

    def read(self, size=-1):
        piece = self.left if size < 0 else self.left[:size]
        self.left = self.left[len(piece) :]
        return piece


def serve(monkeypatch, body: bytes) -> list[str]:
    calls = []

    def urlopen(url, timeout):
        calls.append(url)
        return Body(body)

    monkeypatch.setattr(sources.urllib.request, "urlopen", urlopen)
    return calls


def test_download_checks_a_pinned_digest_before_the_file_is_moved_into_place(tmp_path, monkeypatch):
    serve(monkeypatch, b"data")
    target = tmp_path / "pinned.bin"
    with pytest.raises(sources.DigestMismatch, match=r"pinned\.bin"):
        sources.download("https://x/pinned.bin", target, digest=sha256(b"other").hexdigest())
    assert list(tmp_path.iterdir()) == []
    good = sha256(b"data").hexdigest()
    assert sources.download("https://x/pinned.bin", target, digest=good).read_bytes() == b"data"


def test_pinned_checks_the_cached_file_on_every_read_and_removes_one_that_differs(
    tmp_path, monkeypatch
):
    calls = serve(monkeypatch, b"data")
    target = tmp_path / "pinned.bin"
    good = sha256(b"data").hexdigest()
    assert sources.pinned("https://x/pinned.bin", target, good) == b"data"
    assert sources.pinned("https://x/pinned.bin", target, good) == b"data"
    assert len(calls) == 1
    target.write_bytes(b"tampered")
    with pytest.raises(sources.DigestMismatch):
        sources.pinned("https://x/pinned.bin", target, good)
    assert not target.exists()


def test_multipl_e_reads_one_dataset_by_task_id(tmp_path, monkeypatch):
    tree = {
        "tree": [
            {"path": "datasets/mbpp-typed/mbpp_2_f.py"},
            {"path": "datasets/mbpp-typed/notes.txt"},
            {"path": "datasets/originals-with-cleaned-doctests/HumanEval_1_g.py"},
        ]
    }

    def download(url, target, limit=None):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(json.dumps(tree) if url.endswith("recursive=1") else url)
        return target

    monkeypatch.setattr(sources, "CACHE", tmp_path)
    monkeypatch.setattr(sources, "download", download)
    files = sources.multipl_e("mbpp")
    assert list(files) == ["mbpp/2"]
    assert files["mbpp/2"].endswith("/datasets/mbpp-typed/mbpp_2_f.py")


def test_mbpp_canonical_and_livecodebench_read_the_cached_files(tmp_path, monkeypatch):
    def download(url, target, limit=None):
        target.parent.mkdir(parents=True, exist_ok=True)
        if url.endswith(".json"):
            target.write_text(json.dumps([{"task_id": 7, "code": "def f(): pass"}]))
        else:
            target.write_text('{"question_id": "1"}\n\n{"question_id": "2"}\n')
        return target

    monkeypatch.setattr(sources, "CACHE", tmp_path)
    monkeypatch.setattr(sources, "download", download)
    assert sources.mbpp_canonical() == {"mbpp/7": "def f(): pass"}
    assert [r["question_id"] for r in sources.livecodebench()] == ["1", "2"]


def sample(source: str, ident: int) -> Task:
    return Task(
        id=f"{source}/{ident}",
        source=source,
        name="f",
        params=[("x", Prim("int"))],
        returns=Prim("int"),
        doc="",
        tests=[Case([ident], ident)],
    )


def test_write_then_load_round_trips_the_task_set(tmp_path):
    tasks = [sample("humaneval", 1), sample("mbpp", 2), sample("livecodebench", 3)]
    digests = build.write(tasks, tmp_path)
    assert set(digests) == set(build.SOURCES)
    assert build.load(tmp_path) == tasks
    assert build.load(tmp_path, only=("mbpp",)) == [tasks[1]]


def test_load_says_how_to_build_a_missing_task_set(tmp_path):
    with pytest.raises(FileNotFoundError, match=r"lotml_harness.tasks.build"):
        build.load(tmp_path)


def test_markdown_reports_counts_refusals_and_the_sample_size():
    report = build.Report()
    report.read.update({"humaneval": 2, "mbpp": 1, "livecodebench": 1})
    report.refuse("humaneval", "signature: `Any` has no lotml equivalent")
    report.unchecked["livecodebench"] = 1
    tasks = [sample("humaneval", 1), sample("mbpp", 2), sample("livecodebench", 3)]
    text = build.markdown(tasks, report, dict.fromkeys(build.SOURCES, "abc"))
    assert "| humaneval | 2 | 1 | 1 | 1 |" in text
    assert "| total | 4 | 3 | 3 | 2 |" in text
    assert "| humaneval | signature | 1 |" in text
    assert "(not met)" in text
    assert "- `mbpp.jsonl`: `abc`" in text


def test_from_livecodebench_counts_refusals():
    report = build.Report()
    assert build.from_livecodebench([{"metadata": "{}", "starter_code": "x"}], report) == []
    assert report.refused["livecodebench"]["no function name for the starter code"] == 1
