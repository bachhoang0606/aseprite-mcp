#!/usr/bin/env python3
"""Seam / edge-match lint for tiles (SPEC-003 Phase 4). Deterministic, stdlib-only.

Two adjacent tiles connect seamlessly only if their touching edges match pixel for
pixel. That is a fully deterministic, false-positive-free check — the most
agent-friendly verifiable art gate the project can add (research doc Path E).

Modes:
  --pair A.png B.png --side {right|left|top|bottom}
        A's <side> edge must equal B's opposite edge.
  --strip sheet.png --tile-width N --tile-height N
        Slice a sheet into a tile grid (row-major) and assert every horizontally
        adjacent pair shares a matching seam.

Reuses tools/pixelpng.py (no new deps). Prints a JSON report; exit 1 on any seam
mismatch unless --warn-only. The blob-47-aware "which pairs MUST connect"
orchestration (which uses src/autotile.rs) is the documented Phase-4b follow-up.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402

OPP = {"left": "right", "right": "left", "top": "bottom", "bottom": "top"}


def _at(pixels, w, x, y):
    return pixels[y * w + x]


def edge(pixels, w, h, side):
    """The list of pixels along one side, top->bottom or left->right."""
    if side == "left":
        return [_at(pixels, w, 0, y) for y in range(h)]
    if side == "right":
        return [_at(pixels, w, w - 1, y) for y in range(h)]
    if side == "top":
        return [_at(pixels, w, x, 0) for x in range(w)]
    if side == "bottom":
        return [_at(pixels, w, x, h - 1) for x in range(w)]
    raise ValueError(f"bad side: {side}")


def check_pair(tile_a, tile_b, side):
    """tile_* = (w, h, pixels). `side` is which edge of A touches B."""
    wa, ha, pa = tile_a
    wb, hb, pb = tile_b
    ea = edge(pa, wa, ha, side)
    eb = edge(pb, wb, hb, OPP[side])
    if len(ea) != len(eb):
        return {"ok": False, "reason": "edge_length_mismatch", "a": len(ea), "b": len(eb)}
    mismatches = [i for i, (x, y) in enumerate(zip(ea, eb)) if x != y]
    return {"ok": not mismatches, "side": side, "mismatches": mismatches}


def slice_tiles(w, h, pixels, tw, th):
    """Row-major grid of (col, row, (tw, th, tile_pixels)). Ignores a ragged remainder."""
    cols, rows = w // tw, h // th
    grid = {}
    for ry in range(rows):
        for cx in range(cols):
            tp = []
            for y in range(th):
                base = (ry * th + y) * w + cx * tw
                tp.extend(pixels[base : base + tw])
            grid[(cx, ry)] = (tw, th, tp)
    return cols, rows, grid


def check_strip(w, h, pixels, tw, th):
    """Every horizontally-adjacent tile pair must share a matching seam."""
    if tw <= 0 or th <= 0 or w % tw or h % th:
        return {"ok": False, "reason": "tile size does not divide the sheet"}
    cols, rows, grid = slice_tiles(w, h, pixels, tw, th)
    findings = []
    for ry in range(rows):
        for cx in range(cols - 1):
            r = check_pair(grid[(cx, ry)], grid[(cx + 1, ry)], "right")
            if not r["ok"]:
                findings.append({"a": [cx, ry], "b": [cx + 1, ry], **r})
    return {"ok": not findings, "cols": cols, "rows": rows, "findings": findings}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pair", nargs=2, metavar=("A", "B"))
    ap.add_argument("--side", choices=list(OPP), default="right")
    ap.add_argument("--strip")
    ap.add_argument("--tile-width", type=int)
    ap.add_argument("--tile-height", type=int)
    ap.add_argument("--warn-only", action="store_true")
    args = ap.parse_args()

    if args.pair:
        a = read_png(args.pair[0])
        b = read_png(args.pair[1])
        report = check_pair(a, b, args.side)
    elif args.strip:
        w, h, px = read_png(args.strip)
        tw = args.tile_width or 0
        th = args.tile_height or tw
        report = check_strip(w, h, px, tw, th)
    else:
        ap.error("pass --pair A B or --strip sheet.png --tile-width N")

    print(json.dumps(report))
    sys.exit(0 if (report.get("ok") or args.warn_only) else 1)


if __name__ == "__main__":
    main()
