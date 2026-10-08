"""The typeshed stubs lotml carries (specs/rust-binder R2.1, adr:0032): `stdlib/` and `LICENSE`
of one typeshed commit, vendored as text under `compiler/crates/lotml-bind/typeshed/` with the
commit in `COMMIT`, so moving the pin is a pull request whose diff shows the stubs that changed.

    python release/typeshed.py <commit>     replace the vendored stubs with that commit's

Only a full commit SHA is taken, downloaded over HTTPS from GitHub's archive of the repository;
only regular files under `stdlib/`, and `LICENSE`, are written, each by a path of plain segments.
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


def download(url: str) -> bytes:
    if not url.startswith("https://"):
        raise SystemExit(f"{url}: lotml downloads over HTTPS only")
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 - HTTPS checked above
        return response.read()


def kept(name: str) -> PurePosixPath | None:
    """The path a member of the archive is written at, below its top directory: `LICENSE`, or a
    file under `stdlib/` whose every segment is a plain name; None for anything else."""
    parts = PurePosixPath(name).parts[1:]
    if not parts or not all(SEGMENT.fullmatch(p) for p in parts):
        return None
    if parts == ("LICENSE",) or (parts[0] == "stdlib" and len(parts) > 1):
        return PurePosixPath(*parts)
    return None


def vendor(commit: str, out: Path = VENDORED, get: Callable[[str], bytes] = download) -> int:
    """Replace `out`'s stubs with those of typeshed's `commit`; the number of files written."""
    if not SHA.fullmatch(commit):
        raise SystemExit(f"{commit!r} is not a full commit SHA: the pin names one commit exactly")
    archive = get(f"{ARCHIVE}/{commit}")
    files: dict[PurePosixPath, bytes] = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as tar:
        for member in tar.getmembers():
            path = kept(member.name)
            if path is None or not member.isfile():
                continue
            data = tar.extractfile(member)
            if data is not None:
                files[path] = data.read()
    if PurePosixPath("LICENSE") not in files or not any(p.parts[0] == "stdlib" for p in files):
        raise SystemExit(f"typeshed {commit} has no stdlib/ or no LICENSE")
    shutil.rmtree(out / "stdlib", ignore_errors=True)
    for path, data in sorted(files.items()):
        target = out.joinpath(*path.parts)
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
    (out / "COMMIT").write_text(commit + "\n", encoding="utf-8")
    return len(files)


def main(argv: list[str]) -> int:
    match argv:
        case [commit]:
            print(f"{vendor(commit)} files of typeshed {commit} in {VENDORED}")
            return 0
        case _:
            print(__doc__, file=sys.stderr)
            return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
