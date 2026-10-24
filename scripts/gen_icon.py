#!/usr/bin/env python3
"""Generate the SoloMC app icon (a pixel-art grass block) as a 1024x1024 PNG.

Pure standard library (zlib + struct) so it runs anywhere Python 3 is available.
Usage: python3 scripts/gen_icon.py [output.png]
"""
import struct
import sys
import zlib
from pathlib import Path

SIZE = 1024


def make_pixel_art(size: int):
    """Return a list of rows; each row is a list of (r, g, b, a) tuples."""
    # Base: dirt (brown) with a grass (green) top layer of pixel blocks.
    dirt = (124, 89, 65, 255)
    dirt_dark = (102, 72, 52, 255)
    grass = (106, 170, 64, 255)
    grass_dark = (88, 148, 52, 255)
    grass_top = (122, 190, 74, 255)
    outline = (30, 30, 30, 255)

    px = size // 16  # 16x16 pixel grid, Minecraft style
    grid = [[0] * 16 for _ in range(16)]

    # Top-left highlight and bottom-right shadow give it a pseudo-3D cube look:
    # rows 0..5 -> grass side; rows 6..15 -> dirt side.
    for y in range(16):
        for x in range(16):
            if y < 6:
                base = grass_dark if (x + y) % 4 == 0 else grass
            elif y == 6:
                base = grass_top
            else:
                base = dirt_dark if (x + y) % 3 == 0 else dirt
            grid[y][x] = base

    # Face shading: light from top-left.
    for y in range(16):
        for x in range(16):
            c = grid[y][x]
            if x + y < 8:
                c = tuple(min(255, int(v * 1.15)) for v in c)
            elif x + y > 22:
                c = tuple(int(v * 0.8) for v in c)

    # Draw a simple pixel "M" (monogram) in the center of the grass face.
    m = [
        "1010000001",
        "1010000001",
        "1010000001",
        "1011001001",
        "1011111101",
        "1001111001",
    ]
    ox, oy = 3, 0
    for yy, row in enumerate(m):
        for xx, ch in enumerate(row):
            if ch == "1":
                grid[oy + yy][ox + xx] = outline

    rows = []
    for y in range(16):
        row = []
        for x in range(16):
            row.append(grid[y][x])
        rows.append(row)

    img = []
    for y in range(size):
        row = rows[y // px]
        img_row = []
        for x in range(size):
            img_row.append(row[x // px])
        img.append(img_row)
    return img


def write_png(path: Path, rows):
    n = len(rows)
    raw = bytearray()
    for row in rows:
        raw.append(0)  # filter type 0
        for r, g, b, a in row:
            raw += bytes((r, g, b, a))
    compressed = zlib.compress(bytes(raw), 9)

    def chunk(tag: bytes, data: bytes):
        c = struct.pack(">I", len(data)) + tag + data
        c += struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        return c

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", compressed)
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "src-tauri" / "icons" / "icon.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    write_png(out, make_pixel_art(SIZE))
    print(f"icon written: {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
