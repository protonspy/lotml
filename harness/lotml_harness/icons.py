"""The LotML file icon, the lotus on a page, drawn as pixel grids and written as PNG images and a
Windows icon (specs/file-icons/).

    python -m lotml_harness.icons

The 16- and 32-pixel images are drawn pixel by pixel, since a reduced image blurs at those sizes;
48 is the 16 grid tripled and 256 the 32 grid multiplied by eight, so every size stays pixel art.
Only the standard library writes them, so the bytes depend on nothing but this file.
"""

import struct
import zlib
from pathlib import Path

from lotml_harness import ROOT

ICONS = ROOT / "editors" / "icons"
VSCODE = ROOT / "editors" / "vscode" / "icons"
NAME = "lotml-file"

PALETTE: dict[str, tuple[int, int, int, int]] = {
    ".": (0, 0, 0, 0),
    "K": (0x4A, 0x55, 0x60, 0xFF),  # the page's outline
    "W": (0xFD, 0xFD, 0xFD, 0xFF),  # the page
    "F": (0xC9, 0xD0, 0xD6, 0xFF),  # the folded corner
    "O": (0x77, 0x29, 0x19, 0xFF),  # the petals' outline
    "R": (0xBC, 0x4C, 0x14, 0xFF),  # petal, deepest shade
    "D": (0xE7, 0x75, 0x0E, 0xFF),  # petal, shade
    "N": (0xF9, 0x9B, 0x10, 0xFF),  # petal, orange
    "A": (0xFD, 0xC8, 0x2F, 0xFF),  # petal, amber
    "L": (0xFD, 0xF2, 0x8D, 0xFF),  # petal, light
    "G": (0x01, 0x4E, 0x58, 0xFF),  # leaf, outline
    "E": (0x03, 0x6E, 0x73, 0xFF),  # leaf, shade
    "T": (0x12, 0x95, 0x89, 0xFF),  # leaf, teal
    "M": (0x57, 0xD0, 0xA8, 0xFF),  # leaf, light
}

LOTUS_16 = (
    "....O",
    "...OL",
    "...OA",
    ".O.OA",
    "OLOOA",
    "OAONA",
    ".OANA",
    "..ODN",
    "GTTOO",
    ".GGG.",
)
"""The left half of the lotus on the 16-pixel page, mirrored for the right: '.' shows the page."""

LOTUS_32 = (
    ".........O",
    "........OL",
    ".......OLA",
    "...O..OLAA",
    "..OLO.OLAA",
    "..OLAOOLAA",
    ".OLAAOOLAA",
    ".OLAANOLAA",
    "O.OLANOLAA",
    "OLOLANOAAN",
    "OLAONNOAAN",
    "OLAAONONAN",
    ".OLAAONONN",
    ".OOAANONNN",
    "..OODNNNDD",
    "....OODDRR",
    "......OOOO",
    ".GMTG..GGG",
    "GMTTTG.GTM",
    "GTTEEGGTTE",
    ".GGGG..GGG",
)
"""The left half of the lotus on the 32-pixel page."""


def page(size: int, left: int, right: int, fold: int) -> list[list[str]]:
    """A page from column `left` to `right` filling `size` rows, its top right corner folded
    `fold` pixels down."""
    corner = right - fold
    rows = []
    for y in range(size):
        row = []
        for x in range(size):
            if x < left or x > right:
                cell = "."
            elif y == 0:
                cell = "K" if x <= corner else "."
            elif y == size - 1 or x == left:
                cell = "K"
            elif y < fold:
                diagonal = corner + y
                cell = "W" if x < corner else "F" if corner < x < diagonal else "."
                cell = "K" if x in (corner, diagonal) else cell
            elif y == fold:
                cell = "K" if x >= corner else "W"
            else:
                cell = "K" if x == right else "W"
            row.append(cell)
        rows.append(row)
    return rows


def drawn(size: int, fold: int, half: tuple[str, ...], top: int) -> tuple[str, ...]:
    """The page with the lotus mirrored from `half`, centred, its first row at `top`."""
    left, right = size // 8, size - 1 - size // 8
    rows = page(size, left, right, fold)
    lotus = [line + line[::-1] for line in half]
    start = left + 1 + (right - left - 1 - len(lotus[0])) // 2
    for dy, line in enumerate(lotus):
        for dx, cell in enumerate(line):
            if cell != ".":
                rows[top + dy][start + dx] = cell
    return tuple("".join(row) for row in rows)


GRID_16 = drawn(16, 4, LOTUS_16, 5)
GRID_32 = drawn(32, 7, LOTUS_32, 9)


def scaled(grid: tuple[str, ...], factor: int) -> tuple[str, ...]:
    """`grid` with every pixel repeated `factor` times each way."""
    return tuple("".join(c * factor for c in row) for row in grid for _ in range(factor))


def png(grid: tuple[str, ...]) -> bytes:
    """`grid` as an 8-bit RGBA PNG, every row unfiltered: the same grid gives the same bytes."""
    raw = b"".join(b"\x00" + b"".join(bytes(PALETTE[c]) for c in row) for row in grid)

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    header = struct.pack(">IIBBBBB", len(grid[0]), len(grid), 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico(images: list[tuple[int, bytes]]) -> bytes:
    """A Windows icon holding `images`, each a size and its PNG; 256 is written as 0, as the
    format has it."""
    offset = 6 + 16 * len(images)
    directory, data = b"", b""
    for size, image in images:
        side = 0 if size == 256 else size
        directory += struct.pack("<BBBBHHII", side, side, 0, 0, 1, 32, len(image), offset)
        data += image
        offset += len(image)
    return struct.pack("<HHH", 0, 1, len(images)) + directory + data


def images() -> dict[int, bytes]:
    """The icon at each size, as PNG."""
    grids = {16: GRID_16, 32: GRID_32, 48: scaled(GRID_16, 3), 256: scaled(GRID_32, 8)}
    return {size: png(grid) for size, grid in grids.items()}


def outputs() -> dict[Path, bytes]:
    """Every file the generator writes, by path."""
    made = images()
    files = {ICONS / f"{NAME}-{size}.png": image for size, image in made.items()}
    files[ICONS / f"{NAME}.ico"] = ico(sorted(made.items()))
    files[VSCODE / f"{NAME}.png"] = made[32]
    return files


def main() -> None:
    for path, data in outputs().items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        print(path.relative_to(ROOT).as_posix())


if __name__ == "__main__":
    main()
