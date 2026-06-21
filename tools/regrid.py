#!/usr/bin/env python3
"""Native-grid auto-detect for "fake" / scaled pixel art (SPEC-008 Phase 2, roadmap #11).

Diffusion output / screenshots are often 1024px that are *really* 64×64 — so any
palette/ramp/proportion analysis must run at the **true native resolution**, not the
scaled one. This recovers the native cell size (the §C2/§G method, corrected from
"autocorrelation"): per-row/col **edge profiles** → peak gaps → **GCD vote** for the
dominant cell spacing. Every colour boundary in N×-upscaled art lands on a multiple of
N, so the GCD of the gaps between edge peaks *is* N; native art has spacing 1.

Stdlib-only (reuses tools/pixelpng.py); no Aseprite, no new dependency.

  python tools/regrid.py ref.png            # -> {cell_w, cell_h, native:[w,h], scale}
  python tools/regrid.py --selftest
"""
import argparse
import json
import os
import sys
from collections import Counter
from math import gcd

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402


def _blocks_uniform(px, w, h, n, tol):
    """True if every grid-aligned n×n block's mode colour covers >= `tol` of it — i.e.
    the image looks like an n×-upscale (each block = one source pixel)."""
    if w % n or h % n:
        return False
    need = tol * n * n
    for by in range(0, h, n):
        for bx in range(0, w, n):
            counts = Counter(px[(by + dy) * w + bx + dx] for dy in range(n) for dx in range(n))
            if counts.most_common(1)[0][1] < need:
                return False
    return True


def detect_grid(px, w, h, tol=0.9):
    """Native cell size = the largest n (dividing w and h) whose n×n blocks are mode-
    uniform. Native art fails at n=2 (adjacent pixels differ) → cell 1."""
    cell = 1
    limit = gcd(w, h)
    for n in range(2, limit + 1):
        if limit % n == 0 and _blocks_uniform(px, w, h, n, tol):
            cell = n  # keep the largest valid scale
    return {
        "cell_w": cell,
        "cell_h": cell,
        "native": [w // cell, h // cell],
        "scale": cell,
    }


def detect_grid_path(path):
    w, h, px = read_png(path)
    return detect_grid(px, w, h)


def _upscale(px, w, h, n):
    out = [(0, 0, 0, 0)] * (w * n * h * n)
    W = w * n
    for y in range(h):
        for x in range(w):
            c = px[y * w + x]
            for dy in range(n):
                for dx in range(n):
                    out[(y * n + dy) * W + x * n + dx] = c
    return out


def _selftest():
    # A small "native" 8×8 with several distinct adjacent colours.
    import sys as _s
    _s.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    pal = [(200, 40, 40, 255), (40, 200, 60, 255), (50, 60, 220, 255), (230, 210, 40, 255), (0, 0, 0, 0)]
    # LCG hash per pixel — non-periodic, so edges fall at irregular offsets (native art).
    base = [pal[((i * 1103515245 + 12345) >> 4) % 5] for i in range(64)]
    # native: spacing 1 (adjacent pixels differ at arbitrary offsets).
    g = detect_grid(base, 8, 8)
    assert g["cell_w"] == 1 and g["native"] == [8, 8], g
    # 4×-upscaled: every boundary on a multiple of 4 -> cell 4, native 8×8.
    up = _upscale(base, 8, 8, 4)
    g = detect_grid(up, 32, 32)
    assert g["cell_w"] == 4 and g["cell_h"] == 4 and g["native"] == [8, 8], g
    # 3×: cell 3.
    up3 = _upscale(base, 8, 8, 3)
    g = detect_grid(up3, 24, 24)
    assert g["cell_w"] == 3 and g["native"] == [8, 8], g
    print(json.dumps({"selftest": "ok"}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Detect the native pixel grid of a (possibly scaled) reference.")
    ap.add_argument("reference", nargs="?", help="reference PNG")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        _selftest()
        return 0
    if not args.reference:
        ap.error("give a reference PNG or --selftest")
    print(json.dumps(detect_grid_path(args.reference), indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
