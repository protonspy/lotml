"""The typeshed stubs lotml carries (specs/rust-binder R2.1): one commit's `stdlib/` and `LICENSE`,
written by plain paths only, with the commit recorded beside them."""

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


def test_the_commit_s_stdlib_and_licence_are_vendored_with_the_commit(tmp_path):
    data = archive(
        {
            "typeshed-x/LICENSE": b"LICENCE",
            "typeshed-x/stdlib/VERSIONS": b"json: 3.0-\n",
            "typeshed-x/stdlib/json/__init__.pyi": b"def dumps(obj: object) -> str: ...\n",
            "typeshed-x/stubs/requests/api.pyi": b"",
            "typeshed-x/README.md": b"",
        }
    )
    (tmp_path / "stdlib" / "gone").mkdir(parents=True)
    (tmp_path / "stdlib" / "gone" / "old.pyi").write_text("", encoding="utf-8")
    assert typeshed.vendor(COMMIT, tmp_path, get=served(data)) == 3
    assert (tmp_path / "stdlib" / "json" / "__init__.pyi").read_bytes().startswith(b"def dumps")
    assert (tmp_path / "LICENSE").read_bytes() == b"LICENCE"
    assert (tmp_path / "COMMIT").read_text(encoding="utf-8") == COMMIT + "\n"
    assert not (tmp_path / "stubs").exists(), "only the standard library is carried"
    assert not (tmp_path / "stdlib" / "gone").exists(), "a stub the commit dropped goes"


def test_a_path_that_is_not_plain_and_a_link_are_never_written(tmp_path):
    data = archive(
        {
            "typeshed-x/LICENSE": b"L",
            "typeshed-x/stdlib/os.pyi": b"",
            "typeshed-x/stdlib/../../escape.pyi": b"",
        },
        links={"typeshed-x/stdlib/link.pyi": "/etc/passwd"},
    )
    typeshed.vendor(COMMIT, tmp_path / "out", get=served(data))
    assert not (tmp_path / "escape.pyi").exists()
    assert not (tmp_path / "out" / "stdlib" / "link.pyi").exists()
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
