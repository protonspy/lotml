"""The LotML file icon: four sizes, one Windows icon, the small sizes drawn and the large ones
scaled from them (specs/file-icons/ R1.1, R1.2)."""

import struct
import zlib

import pytest

from lotml_harness import icons

SIZES = (16, 32, 48, 256)


def decoded(png: bytes) -> tuple[int, int, list[bytes]]:
    """Width, height and RGBA rows of an unfiltered 8-bit RGBA PNG."""
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    width, height, depth, colour = struct.unpack(">IIBB", png[16:26])
    assert (depth, colour) == (8, 6), "8-bit RGBA"
    data, pos = b"", 8
    while pos < len(png):
        (length,) = struct.unpack(">I", png[pos : pos + 4])
        if png[pos + 4 : pos + 8] == b"IDAT":
            data += png[pos + 8 : pos + 8 + length]
        pos += 12 + length
    raw, stride = zlib.decompress(data), 1 + 4 * width
    return width, height, [raw[y * stride + 1 : (y + 1) * stride] for y in range(height)]


@pytest.mark.parametrize("grid, size", [(icons.GRID_16, 16), (icons.GRID_32, 32)])
def test_each_small_size_is_its_own_grid_in_the_palette(grid, size):
    assert len(grid) == size and all(len(row) == size for row in grid)
    assert set("".join(grid)) <= set(icons.PALETTE)
    assert {"K", "W", "F"} <= set("".join(grid)), "a page with a folded corner"
    assert {"A", "T"} <= set("".join(grid)), "an amber lotus on teal leaves"


def test_every_size_is_a_png_of_that_size():
    for size, image in icons.images().items():
        width, height, rows = decoded(image)
        assert (width, height, len(rows)) == (size, size, size)
    assert sorted(icons.images()) == list(SIZES)


@pytest.mark.parametrize("large, small, factor", [(48, 16, 3), (256, 32, 8)])
def test_a_large_size_is_a_small_grid_scaled_by_a_whole_number(large, small, factor):
    _, _, big = decoded(icons.images()[large])
    _, _, little = decoded(icons.images()[small])
    for y, row in enumerate(big):
        for x in range(large):
            source = little[y // factor][(x // factor) * 4 : (x // factor) * 4 + 4]
            assert row[x * 4 : x * 4 + 4] == source, (x, y)


def test_the_windows_icon_holds_the_four_sizes_as_pngs():
    data = icons.outputs()[icons.ICONS / "lotml-file.ico"]
    reserved, kind, count = struct.unpack("<HHH", data[:6])
    assert (reserved, kind, count) == (0, 1, 4)
    for i, size in enumerate(SIZES):
        entry = struct.unpack("<BBBBHHII", data[6 + 16 * i : 22 + 16 * i])
        width, height, _, _, planes, bits, length, offset = entry
        assert (width, height) == ((0, 0) if size == 256 else (size, size))
        assert (planes, bits) == (1, 32)
        assert decoded(data[offset : offset + length])[:2] == (size, size)


def test_the_committed_files_are_what_the_generator_writes():
    for path, data in icons.outputs().items():
        assert path.is_file(), f"{path} is missing: python -m lotml_harness.icons"
        assert path.read_bytes() == data, f"{path} is stale: python -m lotml_harness.icons"
