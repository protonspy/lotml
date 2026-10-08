"""The uv a release of lotml ships (adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default,
plans/python-via-uv.md 1.2), pinned in `uv.json` beside this file.

    python release/uv.py fetch <target> <directory>   # uv and its licences, each hash checked
    python release/uv.py check <archive> <target>     # the archive holds lotml, uv, the licences
    python release/uv.py check-pin <target>           # the pin still matches what uv publishes

`fetch` downloads over HTTPS from uv's own release and refuses a file whose SHA-256 differs from
the pin. It writes uv's binary, its licences named `uv-LICENSE-*`, and a `SHA256SUMS` of every file
in the directory, so the archive carries the sums of what it holds.
"""

import hashlib
import io
import json
import sys
import tarfile
import urllib.request
import zipfile
from collections.abc import Callable
from pathlib import Path

PIN = Path(__file__).with_name("uv.json")
RELEASES = "https://github.com/astral-sh/uv/releases/download"
SOURCES = "https://raw.githubusercontent.com/astral-sh/uv"


def pin() -> dict:
    return json.loads(PIN.read_text(encoding="utf-8"))


def download(url: str) -> bytes:
    """The bytes at `url`, over HTTPS only."""
    if not url.startswith("https://"):
        raise SystemExit(f"refusing to download over anything but HTTPS: {url}")
    with urllib.request.urlopen(url, timeout=120) as response:  # noqa: S310 - HTTPS checked above
        return response.read()


def checked(data: bytes, expected: str, what: str) -> bytes:
    """`data`, when its SHA-256 is `expected`; else the release stops, naming `what`."""
    found = hashlib.sha256(data).hexdigest()
    if found != expected:
        raise SystemExit(f"{what}: SHA-256 {found}, the pin says {expected}")
    return data


def binary(archive: bytes, name: str) -> bytes:
    """uv's executable, `uv` or `uv.exe`, out of a release archive of uv."""
    if name.endswith(".zip"):
        with zipfile.ZipFile(io.BytesIO(archive)) as z:
            return z.read("uv.exe")
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as t:
        member = next(m for m in t.getmembers() if m.isfile() and Path(m.name).name == "uv")
        extracted = t.extractfile(member)
        if extracted is None:
            raise SystemExit(f"{name}: uv is not a file")
        return extracted.read()


def executable(target: str) -> str:
    return "uv.exe" if "windows" in target else "uv"


def fetch(target: str, directory: Path, get: Callable[[str], bytes] = download) -> list[Path]:
    """uv for `target` and its licences into `directory`, each checked; the files written."""
    pinned = pin()
    archive = pinned["archives"].get(target)
    if archive is None:
        raise SystemExit(f"uv.json pins no uv for {target}")
    version = pinned["version"]
    url = f"{RELEASES}/{version}/{archive['file']}"
    data = checked(get(url), archive["sha256"], archive["file"])
    directory.mkdir(parents=True, exist_ok=True)
    written = []
    path = directory / executable(target)
    path.write_bytes(binary(data, archive["file"]))
    path.chmod(0o755)
    written.append(path)
    for licence, expected in pinned["licences"].items():
        text = checked(get(f"{SOURCES}/{version}/{licence}"), expected, licence)
        path = directory / f"uv-{licence}"
        path.write_bytes(text)
        written.append(path)
    sums = [
        f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}"
        for p in sorted(directory.iterdir())
        if p.is_file() and p.name != "SHA256SUMS"
    ]
    (directory / "SHA256SUMS").write_text("\n".join(sums) + "\n", encoding="utf-8")
    return written


def names(archive: Path) -> list[str]:
    """The files a release archive of lotml holds, without the directory they sit in."""
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as z:
            listed = [i.filename for i in z.infolist() if not i.is_dir()]
    else:
        with tarfile.open(archive, mode="r:gz") as t:
            listed = [m.name for m in t.getmembers() if m.isfile()]
    return sorted(name.split("/", 1)[-1] for name in listed)


def check(archive: Path, target: str) -> list[str]:
    """What is missing from a release archive of lotml for `target`, or wrong in its sums."""
    held = names(archive)
    lotml = "lotml.exe" if "windows" in target else "lotml"
    wanted = [lotml, executable(target), "SHA256SUMS", *(f"uv-{n}" for n in pin()["licences"])]
    problems = [f"no {name}" for name in wanted if name not in held]
    if "SHA256SUMS" in held:
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as z:
                sums = z.read(next(n for n in z.namelist() if n.endswith("SHA256SUMS"))).decode()
        else:
            with tarfile.open(archive, mode="r:gz") as t:
                member = next(m for m in t.getmembers() if m.name.endswith("SHA256SUMS"))
                extracted = t.extractfile(member)
                sums = extracted.read().decode() if extracted else ""
        summed = {line.split()[-1] for line in sums.splitlines() if line.strip()}
        problems += [
            f"SHA256SUMS lists no {n}" for n in wanted if n != "SHA256SUMS" and n not in summed
        ]
    return problems


def main(argv: list[str]) -> int:
    match argv:
        case ["fetch", target, directory]:
            for path in fetch(target, Path(directory)):
                print(path)
            return 0
        case ["check", archive, target]:
            problems = check(Path(archive), target)
            for problem in problems:
                print(f"{archive}: {problem}", file=sys.stderr)
            return 1 if problems else 0
        case ["check-pin", target]:
            with_pin = pin()
            archive = with_pin["archives"][target]
            url = f"{RELEASES}/{with_pin['version']}/{archive['file']}"
            checked(download(url), archive["sha256"], archive["file"])
            print(f"{archive['file']} matches the pin")
            return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
