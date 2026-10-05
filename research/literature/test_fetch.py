import hashlib
import io

import fetch
import pytest

PDF = b"%PDF-1.7 a paper"
DIGEST = hashlib.sha256(PDF).hexdigest()


def opener(data: bytes):
    return lambda request, timeout: io.BytesIO(data)


@pytest.fixture
def cache(tmp_path, monkeypatch):
    monkeypatch.setattr(fetch, "CACHE", tmp_path)
    return tmp_path


def test_a_matching_download_lands_in_the_cache(cache):
    source = {"id": "s1", "url": "https://x/s1.pdf", "sha256": DIGEST}
    assert fetch.fetch_one(source, opener(PDF)) is None
    assert (cache / "s1.pdf").read_bytes() == PDF
    assert not list(cache.glob("*.part"))


def test_a_mismatching_download_is_deleted_and_reported(cache):
    source = {"id": "s1", "url": "https://x/s1.pdf", "sha256": DIGEST}
    assert "checksum" in fetch.fetch_one(source, opener(b"%PDF other bytes"))
    assert not list(cache.iterdir())


def test_a_cached_file_that_no_longer_matches_is_deleted(cache):
    (cache / "s1.pdf").write_bytes(b"tampered")
    source = {"id": "s1", "url": "https://x/s1.pdf", "sha256": DIGEST}
    assert "checksum" in fetch.fetch_one(source, opener(PDF))
    assert not (cache / "s1.pdf").exists()


def test_a_new_source_records_its_checksum(cache):
    source = {"id": "s1", "url": "https://x/s1.pdf"}
    assert fetch.fetch_one(source, opener(PDF)) is None
    assert source["sha256"] == DIGEST


def test_an_oversized_download_is_refused(cache, monkeypatch):
    monkeypatch.setattr(fetch, "MAX_BYTES", 4)
    assert "larger" in fetch.fetch_one(
        {"id": "s1", "url": "https://x/s1.pdf"}, opener(PDF)
    )
    assert not list(cache.iterdir())
