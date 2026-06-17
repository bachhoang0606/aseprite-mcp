#!/usr/bin/env python3
"""Generate deterministic PNG fixtures for the visual + linter tests.

Stdlib-only, no Aseprite. Re-run to regenerate identical bytes (deterministic):
    python tests/visual/gen_fixtures.py

Produces (16x16 RGBA):
  fixtures/good_swatch.png      — goblin skin ramp bands, all on-palette, connected
  fixtures/bad_offpalette.png   — good_swatch + one off-palette pixel
  fixtures/bad_orphan.png       — a block plus a lone stray pixel
  golden/good_swatch.png        — byte-identical golden for the diff test
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from pixelpng import write_png  # noqa: E402

W = H = 16
TRANSPARENT = (0, 0, 0, 0)


def hex_to_rgba(h):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), 255)


def skin_ramp():
    with open(os.path.join(ROOT, "knowledge", "palettes", "goblin-default.json"), encoding="utf-8") as f:
        pal = json.load(f)
    return [hex_to_rgba(c) for c in pal["ramps"]["skin"]]


def blank():
    return [TRANSPARENT] * (W * H)


def put(px, x, y, c):
    px[y * W + x] = c


def good_swatch():
    """Horizontal bands of the skin ramp filling the whole canvas (connected)."""
    ramp = skin_ramp()
    px = blank()
    band = H // len(ramp)
    for y in range(H):
        c = ramp[min(y // band, len(ramp) - 1)]
        for x in range(W):
            put(px, x, y, c)
    return px


def main():
    os.makedirs(os.path.join(HERE, "fixtures"), exist_ok=True)
    os.makedirs(os.path.join(HERE, "golden"), exist_ok=True)

    good = good_swatch()
    write_png(os.path.join(HERE, "fixtures", "good_swatch.png"), W, H, good)
    write_png(os.path.join(HERE, "golden", "good_swatch.png"), W, H, good)

    bad_off = list(good)
    put(bad_off, 1, 1, (0x12, 0x34, 0x56, 255))  # not in the goblin palette
    write_png(os.path.join(HERE, "fixtures", "bad_offpalette.png"), W, H, bad_off)

    bad_orphan = blank()
    for y in range(4, 10):  # a solid 6x6 block (connected)
        for x in range(4, 10):
            put(bad_orphan, x, y, (0x4C, 0xA0, 0x2C, 255))
    put(bad_orphan, 14, 1, (0x4C, 0xA0, 0x2C, 255))  # lone stray pixel
    write_png(os.path.join(HERE, "fixtures", "bad_orphan.png"), W, H, bad_orphan)

    print("fixtures written to", os.path.join(HERE, "fixtures"))


if __name__ == "__main__":
    main()
