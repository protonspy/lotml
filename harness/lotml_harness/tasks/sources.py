"""The source datasets, downloaded at pinned revisions into the git-ignored cache.

MultiPL-E's license forbids using its contents as training data, and LiveCodeBench's
tests are large, so neither is committed: the build fetches them, and the report it writes
records what came out.
"""

import hashlib
import json
import re
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from lotml_harness import ROOT

CACHE = ROOT / "harness" / "cache"

MULTIPL_E = "3025a531af7450e7df8b96fe0440e9804480bbad"
"""nuprl/MultiPL-E: typed Python originals of HumanEval and MBPP."""
LIVECODEBENCH = "0fe84c3912ea0c4d4a78037083943e8f0c4dd505"
"""livecodebench/code_generation_lite on Hugging Face; `test6.jsonl` is release v6's."""
HUMAN_EVAL = "463c980b59e818ace59f6f9803cd92c749ceae61"
"""openai/human-eval: the original HumanEval, MIT-licensed, untyped."""
HUMAN_EVAL_SHA256 = "b796127e635a67f93fb35c04f4cb03cf06f38c8072ee7cee8833d7bee06979ef"
"""`data/HumanEval.jsonl.gz` at `HUMAN_EVAL`."""
MBPP = "f82046ba5aabbbb427dbfd38a254d26bff08b533"
"""google-research/google-research: MBPP's original release, CC BY 4.0."""
MBPP_SHA256 = "ccf64ceae9c5403bf50a044cb6d505bfd2a2963ee58338ba268fd65beab92a9f"
"""`mbpp/mbpp.jsonl` at `MBPP`."""

DIRECTORIES = {
    "humaneval": "originals-with-cleaned-doctests",
    "mbpp": "mbpp-typed",
}
FILENAME = re.compile(r"^(?:HumanEval|mbpp)_(\d+)_\w+\.py$")


DOWNLOAD_LIMIT = 2 * 2**30
"""Bytes one download may take; the largest source, LiveCodeBench's test file, is far below."""
CHUNK = 2**20
MULTIPL_E_LIMIT = 64 * 2**20
"""Bytes for one MultiPL-E file or listing: they are kilobytes."""


class DigestMismatch(ValueError):
    """A downloaded or cached file whose SHA-256 is not the one pinned for it."""


def download(
    url: str, target: Path, limit: int = DOWNLOAD_LIMIT, digest: str | None = None
) -> Path:
    """`url` saved at `target`, unless an earlier run already saved it. The body is written in
    pieces, and one longer than `limit` is refused and removed rather than filling the disk.
    With `digest`, a body whose SHA-256 differs is removed before it is moved into place."""
    if not target.exists():
        target.parent.mkdir(parents=True, exist_ok=True)
        partial = target.with_suffix(target.suffix + ".part")
        written = 0
        hashed = hashlib.sha256()
        with urllib.request.urlopen(url, timeout=120) as response, partial.open("wb") as out:  # noqa: S310
            while piece := response.read(CHUNK):
                written += len(piece)
                if written > limit:
                    break
                hashed.update(piece)
                out.write(piece)
        if written > limit:
            partial.unlink()
            raise ValueError(f"{url} is larger than {limit} bytes")
        if digest is not None and hashed.hexdigest() != digest:
            partial.unlink()
            raise DigestMismatch(f"{target.name}: SHA-256 {hashed.hexdigest()}, pinned {digest}")
        partial.replace(target)
    return target


def pinned(url: str, target: Path, digest: str, limit: int = DOWNLOAD_LIMIT) -> bytes:
    """The bytes of `url`, downloaded once and checked against `digest` on every read. The digest
    is taken from the very bytes returned, and a cached file that differs is removed."""
    data = download(url, target, limit, digest).read_bytes()
    found = hashlib.sha256(data).hexdigest()
    if found != digest:
        target.unlink()
        raise DigestMismatch(f"{target.name}: SHA-256 {found}, pinned {digest}")
    return data


def task_id(source: str, filename: str) -> str | None:
    match = FILENAME.match(filename)
    return f"{source}/{match.group(1)}" if match else None


def multipl_e_paths() -> list[str]:
    url = f"https://api.github.com/repos/nuprl/MultiPL-E/git/trees/{MULTIPL_E}?recursive=1"
    listing = download(url, CACHE / "multipl-e" / MULTIPL_E / "tree.json", MULTIPL_E_LIMIT)
    return [entry["path"] for entry in json.loads(listing.read_text())["tree"]]


def multipl_e_file(path: str) -> Path:
    url = f"https://raw.githubusercontent.com/nuprl/MultiPL-E/{MULTIPL_E}/{path}"
    return download(url, CACHE / "multipl-e" / MULTIPL_E / path, MULTIPL_E_LIMIT)


def multipl_e(source: str) -> dict[str, str]:
    """Each file of one dataset by task id: `humaneval/0`, `mbpp/100`."""
    prefix = f"datasets/{DIRECTORIES[source]}/"
    wanted = {
        path: task_id(source, path.removeprefix(prefix))
        for path in multipl_e_paths()
        if path.startswith(prefix)
    }
    wanted = {path: ident for path, ident in wanted.items() if ident is not None}
    with ThreadPoolExecutor(max_workers=16) as pool:
        files = list(pool.map(multipl_e_file, wanted))
    return {
        ident: file.read_text(encoding="utf-8")
        for ident, file in zip(wanted.values(), files, strict=True)
    }


def mbpp_canonical() -> dict[str, str]:
    """The sanitized MBPP reference solutions, by task id."""
    data = json.loads(multipl_e_file("datasets/sanitized-mbpp.json").read_text(encoding="utf-8"))
    return {f"mbpp/{record['task_id']}": record["code"] for record in data}


def livecodebench() -> list[dict]:
    url = (
        "https://huggingface.co/datasets/livecodebench/code_generation_lite/"
        f"resolve/{LIVECODEBENCH}/test6.jsonl"
    )
    path = download(url, CACHE / "livecodebench" / LIVECODEBENCH / "test6.jsonl")
    with path.open(encoding="utf-8") as lines:
        return [json.loads(line) for line in lines if line.strip()]
