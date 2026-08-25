#!/usr/bin/env python3
"""Generate the app icon (1024px) and menu-bar template icon (44px) with real
alpha — no image libraries needed. qlmanage-rendered SVGs bake in an opaque
white background, which makes macOS template tray icons draw as a solid square
(templates render the alpha channel only)."""

import math
import struct
import sys
import zlib


def write_png(path, w, h, rows):
    def chunk(tag, data):
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    raw = b"".join(b"\x00" + bytes(v for px in row for v in px) for row in rows)
    out = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(out)


def clamp01(v):
    return max(0.0, min(1.0, v))


def capsule_alpha(x, y, cx, y0, y1, r):
    """Anti-aliased coverage of a round-capped vertical bar."""
    py = min(max(y, y0), y1)
    return clamp01(0.5 - (math.hypot(x - cx, y - py) - r))


def rounded_rect_alpha(x, y, x0, y0, x1, y1, r):
    nx = min(max(x, x0 + r), x1 - r)
    ny = min(max(y, y0 + r), y1 - r)
    return clamp01(0.5 - (math.hypot(x - nx, y - ny) - r))


# Waveform bars as (center_x, y0, y1) in 1024-space; stroke radius 28.
BARS_1024 = [
    (312, 472, 552),
    (412, 392, 632),
    (512, 312, 712),
    (612, 392, 632),
    (712, 472, 552),
]


def app_icon(path):
    w = h = 1024
    rows = []
    for y in range(h):
        row = []
        for x in range(w):
            base_a = rounded_rect_alpha(x, y, 64, 64, 960, 960, 200)
            if base_a <= 0.0:
                row.append((0, 0, 0, 0))
                continue
            t = (x + y) / (2.0 * 1024.0)  # diagonal gradient blue -> violet
            br = 0x3B + (0x7C - 0x3B) * t
            bg = 0x82 + (0x3A - 0x82) * t
            bb = 0xF6 + (0xED - 0xF6) * t
            bar_a = max(capsule_alpha(x, y, cx, y0, y1, 28) for cx, y0, y1 in BARS_1024)
            # white bars composited over the gradient
            r = br + (255 - br) * bar_a
            g = bg + (255 - bg) * bar_a
            b = bb + (255 - bb) * bar_a
            row.append((int(r), int(g), int(b), int(base_a * 255)))
        rows.append(row)
    write_png(path, w, h, rows)


def tray_icon(path):
    """44px (22pt @2x) template: black bars, transparent background."""
    w = h = 44
    bars = [(8, 19, 25), (15, 14, 30), (22, 9, 35), (29, 14, 30), (36, 19, 25)]
    rows = []
    for y in range(h):
        row = []
        for x in range(w):
            a = max(capsule_alpha(x, y, cx, y0, y1, 2.0) for cx, y0, y1 in bars)
            row.append((0, 0, 0, int(a * 255)))
        rows.append(row)
    write_png(path, w, h, rows)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "."
    app_icon(f"{out}/icon_1024.png")
    tray_icon(f"{out}/tray.png")
    print(f"wrote {out}/icon_1024.png and {out}/tray.png")
