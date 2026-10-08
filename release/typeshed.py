"""The typeshed stubs lotml carries (specs/rust-binder R2.1, adr:0032): `stdlib/` and `LICENSE`
of one typeshed commit, vendored as text under `compiler/crates/lotml-bind/typeshed/` with the
commit in `COMMIT`, so moving the pin is a pull request whose diff shows the stubs that changed.

    python release/typeshed.py <commit>     replace the vendored stubs with that commit's
    python release/typeshed.py --verify     check the vendored stubs are COMMIT's, byte for byte

Only a full commit SHA is taken, downloaded over HTTPS from GitHub's archive of the repository and
from nowhere a redirect leads; the archive's top directory must name that commit; only regular
files under `stdlib/`, and `LICENSE`, are kept, each by a path of plain segments and of a bounded
size.
"""

import io
import re
import shutil
import sys
import tarfile
import urllib.request
from collections.abc import Callable
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
VENDORED = ROOT / "compiler" / "crates" / "lotml-bind" / "typeshed"
ARCHIVE = "https://codeload.github.com/python/typeshed/tar.gz"
SHA = re.compile(r"[0-9a-f]{40}")
SEGMENT = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_.-]*")
LARGEST_ARCHIVE = 64 << 20
LARGEST_FILE = 8 << 20


def download(url: str) -> bytes:
    if not url.startswith("https://"):
        raise SystemExit(f"{url}: lotml downloads over HTTPS only")
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 - HTTPS checked above
        if not response.geturl().startswith(ARCHIVE + "/"):
            raise SystemExit(f"{url} led to {response.geturl()}, which is not GitHub's archive")
        data = response.read(LARGEST_ARCHIVE + 1)
    if len(data) > LARGEST_ARCHIVE:
        raise SystemExit(f"{url} is past {LARGEST_ARCHIVE} bytes")
    return data


def kept(name: str, commit: str) -> PurePosixPath | None:
    """The path a member of the archive is written at, below its top directory, which must name
    `commit`: `LICENSE`, or a file under `stdlib/` whose every segment is a plain name; None for
    anything else."""
    parts = PurePosixPath(name).parts
    if not parts or parts[0] != f"typeshed-{commit}":
        return None
    parts = parts[1:]
    if not parts or not all(SEGMENT.fullmatch(p) for p in parts):
        return None
    if parts == ("LICENSE",) or (parts[0] == "stdlib" and len(parts) > 1):
        return PurePosixPath(*parts)
    return None


def stubs(commit: str, get: Callable[[str], bytes] = download) -> dict[PurePosixPath, bytes]:
    """The `stdlib/` files and `LICENSE` of typeshed's `commit`, by their paths."""
    if not SHA.fullmatch(commit):
        raise SystemExit(f"{commit!r} is not a full commit SHA: the pin names one commit exactly")
    archive = get(f"{ARCHIVE}/{commit}")
    files: dict[PurePosixPath, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
        for member in tar.getmembers():
            path = kept(member.name, commit)
            if path is None or not member.isfile():
                continue
            if member.size > LARGEST_FILE:
                raise SystemExit(f"typeshed {commit}: {path} is past {LARGEST_FILE} bytes")
            data = tar.extractfile(member)
            if data is not None:
                files[path] = data.read(LARGEST_FILE + 1)
    if PurePosixPath("LICENSE") not in files or not any(p.parts[0] == "stdlib" for p in files):
        raise SystemExit(f"typeshed {commit} has no stdlib/ or no LICENSE")
    return files


def vendor(commit: str, out: Path = VENDORED, get: Callable[[str], bytes] = download) -> int:
    """Replace `out`'s stubs with those of typeshed's `commit`; the number of files written."""
    files = stubs(commit, get)
    shutil.rmtree(out / "stdlib", ignore_errors=True)
    for path, data in sorted(files.items()):
        target = out.joinpath(*path.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    (out / "COMMIT").write_text(commit + "\n", encoding="utf-8")
    return len(files)


def verify(out: Path = VENDORED, get: Callable[[str], bytes] = download) -> list[str]:
    """How the stubs vendored in `out` differ from those of the commit `COMMIT` names: a file
    changed, added or missing, each by its path; empty when they are that commit's."""
    commit = (out / "COMMIT").read_text(encoding="utf-8").strip()
    expected = stubs(commit, get)
    found = {
        PurePosixPath(*p.relative_to(out).parts): p.read_bytes()
        for p in [out / "LICENSE", *sorted((out / "stdlib").rglob("*"))]
        if p.is_file()
    }
    problems = [f"{p} is missing" for p in sorted(expected.keys() - found.keys())]
    problems += [f"{p} is not typeshed's" for p in sorted(found.keys() - expected.keys())]
    problems += [
        f"{p} differs from typeshed {commit}"
        for p in sorted(expected.keys() & found.keys())
        if expected[p] != found[p]
    ]
    return problems


def main(argv: list[str]) -> int:
    match argv:
        case ["--verify"]:
            problems = verify()
            for problem in problems:
                print(problem, file=sys.stderr)
            return 1 if problems else 0
        case [commit]:
            print(f"{vendor(commit)} files of typeshed {commit} in {VENDORED}")
            return 0
        case _:
            print(__doc__, file=sys.stderr)
            return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
