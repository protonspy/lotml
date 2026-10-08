"""The uv a release ships (plans/python-via-uv.md 1.2): fetched only when its SHA-256 matches the
pin, written beside lotml with its licences and the sums of what the archive holds, and every
target the release builds pinned."""

import hashlib
import importlib.util
import io
import json
import re
import tarfile

import pytest

from lotml_harness import ROOT

RELEASE = ROOT / "release"


def load():
    spec = importlib.util.spec_from_file_location("release_uv", RELEASE / "uv.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


uv = load()


def tar_gz(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as t:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            t.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


@pytest.fixture
def pinned(tmp_path, monkeypatch):
    """A pin of a fake uv for Linux, and what each of its URLs serves."""
    archive = tar_gz(
        {"uv-x86_64-unknown-linux-gnu/uv": b"UV", "uv-x86_64-unknown-linux-gnu/uvx": b"X"}
    )
    licences = {"LICENSE-MIT": b"MIT", "LICENSE-APACHE": b"APACHE"}
    pin = {
        "version": "9.9.9",
        "archives": {
            "x86_64-unknown-linux-gnu": {
                "file": "uv-x86_64-unknown-linux-gnu.tar.gz",
                "sha256": hashlib.sha256(archive).hexdigest(),
            }
        },
        "licences": {name: hashlib.sha256(text).hexdigest() for name, text in licences.items()},
    }
    path = tmp_path / "uv.json"
    path.write_text(json.dumps(pin), encoding="utf-8")
    monkeypatch.setattr(uv, "PIN", path)
    served = {f"{uv.RELEASES}/9.9.9/uv-x86_64-unknown-linux-gnu.tar.gz": archive}
    served |= {f"{uv.SOURCES}/9.9.9/{name}": text for name, text in licences.items()}
    return served


def test_fetch_writes_uv_its_licences_and_the_sums_of_the_directory(tmp_path, pinned):
    out = tmp_path / "lotml-1.0.0-x86_64-unknown-linux-gnu"
    out.mkdir()
    (out / "lotml").write_bytes(b"LOTML")
    uv.fetch("x86_64-unknown-linux-gnu", out, get=pinned.__getitem__)
    assert (out / "uv").read_bytes() == b"UV"
    assert (out / "uv-LICENSE-MIT").read_bytes() == b"MIT"
    sums = (out / "SHA256SUMS").read_text(encoding="utf-8")
    assert f"{hashlib.sha256(b'LOTML').hexdigest()}  lotml" in sums
    assert f"{hashlib.sha256(b'UV').hexdigest()}  uv" in sums


def test_a_file_whose_hash_differs_from_the_pin_stops_the_release(tmp_path, pinned):
    tampered = dict(pinned)
    key = next(k for k in tampered if k.endswith(".tar.gz"))
    tampered[key] = tar_gz({"uv": b"SOMETHING ELSE"})
    with pytest.raises(SystemExit, match="the pin says"):
        uv.fetch("x86_64-unknown-linux-gnu", tmp_path / "out", get=tampered.__getitem__)
    with pytest.raises(SystemExit, match="pins no uv"):
        uv.fetch("riscv64-unknown-linux-gnu", tmp_path / "out", get=pinned.__getitem__)


def test_nothing_is_downloaded_but_over_https():
    with pytest.raises(SystemExit, match="HTTPS"):
        uv.download("http://example.com/uv.tar.gz")


def test_the_archive_check_names_what_is_missing(tmp_path, pinned):
    out = tmp_path / "lotml-1.0.0-x86_64-unknown-linux-gnu"
    out.mkdir()
    (out / "lotml").write_bytes(b"LOTML")
    uv.fetch("x86_64-unknown-linux-gnu", out, get=pinned.__getitem__)
    whole = tmp_path / "whole.tar.gz"
    with tarfile.open(whole, mode="w:gz") as t:
        t.add(out, arcname=out.name)
    assert uv.check(whole, "x86_64-unknown-linux-gnu") == []
    (out / "uv").unlink()
    partial = tmp_path / "partial.tar.gz"
    with tarfile.open(partial, mode="w:gz") as t:
        t.add(out, arcname=out.name)
    assert "no uv" in uv.check(partial, "x86_64-unknown-linux-gnu")


def test_every_target_the_release_builds_has_a_pinned_uv():
    workflow = (ROOT / ".github" / "workflows" / "release.yml").read_text(encoding="utf-8")
    targets = set(re.findall(r"- target: (\S+)", workflow))
    assert targets, "the release builds some target"
    assert targets <= set(uv.pin()["archives"]), targets - set(uv.pin()["archives"])
    for archive in uv.pin()["archives"].values():
        assert re.fullmatch(r"[0-9a-f]{64}", archive["sha256"]), archive
