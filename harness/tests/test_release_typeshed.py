"""The typeshed stubs lotml carries (specs/rust-binder R2.1): one commit's `stdlib/` and `LICENSE`,
written by plain paths only, with the commit recorded beside them and checked against it."""

import importlib.util
import io
import tarfile

import pytest

from lotml_harness import ROOT


def load():
    spec = importlib.util.spec_from_file_location(
        "release_typeshed", ROOT / "release" / "typeshed.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


typeshed = load()
COMMIT = "0" * 40
TOP = f"typeshed-{COMMIT}"


def archive(files: dict[str, bytes], links: dict[str, str] | None = None) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as t:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            t.addfile(info, io.BytesIO(data))
        for name, target in (links or {}).items():
            info = tarfile.TarInfo(name)
            info.type = tarfile.SYMTYPE
            info.linkname = target
            t.addfile(info)
    return buffer.getvalue()


def served(data: bytes):
    return lambda url: data if url == f"{typeshed.ARCHIVE}/{COMMIT}" else pytest.fail(url)


STUBS = {
    f"{TOP}/LICENSE": b"LICENCE",
    f"{TOP}/stdlib/VERSIONS": b"json: 3.0-\n",
    f"{TOP}/stdlib/json/__init__.pyi": b"def dumps(obj: object) -> str: ...\n",
    f"{TOP}/stubs/requests/api.pyi": b"",
    f"{TOP}/README.md": b"",
}


def test_the_commit_s_stdlib_and_licence_are_vendored_with_the_commit(tmp_path):
    (tmp_path / "stdlib" / "gone").mkdir(parents=True)
    (tmp_path / "stdlib" / "gone" / "old.pyi").write_text("", encoding="utf-8")
    assert typeshed.vendor(COMMIT, tmp_path, get=served(archive(STUBS))) == 3
    assert (tmp_path / "stdlib" / "json" / "__init__.pyi").read_bytes().startswith(b"def dumps")
    assert (tmp_path / "LICENSE").read_bytes() == b"LICENCE"
    assert (tmp_path / "COMMIT").read_text(encoding="utf-8") == COMMIT + "\n"
    assert not (tmp_path / "stubs").exists(), "only the standard library is carried"
    assert not (tmp_path / "stdlib" / "gone").exists(), "a stub the commit dropped goes"


def test_verify_finds_a_stub_changed_added_or_missing(tmp_path):
    get = served(archive(STUBS))
    typeshed.vendor(COMMIT, tmp_path, get=get)
    assert typeshed.verify(tmp_path, get=get) == []
    (tmp_path / "stdlib" / "json" / "__init__.pyi").write_bytes(b"def dumps(obj) -> int: ...\n")
    (tmp_path / "stdlib" / "extra.pyi").write_bytes(b"")
    (tmp_path / "stdlib" / "VERSIONS").unlink()
    assert typeshed.verify(tmp_path, get=get) == [
        "stdlib/VERSIONS is missing",
        "stdlib/extra.pyi is not typeshed's",
        "stdlib/json/__init__.pyi differs from typeshed " + COMMIT,
    ]


def test_a_path_that_is_not_plain_a_link_and_another_commit_s_files_are_never_written(tmp_path):
    data = archive(
        {
            f"{TOP}/LICENSE": b"L",
            f"{TOP}/stdlib/os.pyi": b"",
            f"{TOP}/stdlib/../../escape.pyi": b"",
            f"typeshed-{'1' * 40}/stdlib/other.pyi": b"",
        },
        links={f"{TOP}/stdlib/link.pyi": "/etc/passwd"},
    )
    typeshed.vendor(COMMIT, tmp_path / "out", get=served(data))
    assert not (tmp_path / "escape.pyi").exists()
    assert not (tmp_path / "out" / "stdlib" / "link.pyi").exists()
    assert not (tmp_path / "out" / "stdlib" / "other.pyi").exists()
    assert (tmp_path / "out" / "stdlib" / "os.pyi").exists()


def test_only_a_full_commit_is_pinned_and_only_over_https(tmp_path):
    with pytest.raises(SystemExit, match="full commit SHA"):
        typeshed.vendor("main", tmp_path)
    with pytest.raises(SystemExit, match="HTTPS"):
        typeshed.download("http://example.com/x.tar.gz")


def test_the_vendored_stubs_name_their_commit():
    commit = (typeshed.VENDORED / "COMMIT").read_text(encoding="utf-8").strip()
    assert typeshed.SHA.fullmatch(commit), commit
    assert (typeshed.VENDORED / "LICENSE").is_file()
    assert (typeshed.VENDORED / "stdlib" / "VERSIONS").is_file()
