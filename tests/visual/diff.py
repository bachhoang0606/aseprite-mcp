#!/usr/bin/env python3
"""Golden pixel-diff for sprite visual-regression (checklist 9.3). Stdlib-only.

Compares two PNGs pixel-by-pixel with a per-channel tolerance. On mismatch it
writes a diff image (changed pixels in magenta) and exits non-zero, so CI catches
unintended art changes.

Usage:
    python tests/visual/diff.py actual.png golden.png [--tolerance 0] [--out diff.png]
"""
import argparse
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "tools"))
from pixelpng import read_png, write_png  # noqa: E402


def diff(actual_path, golden_path, tolerance=0, out_path=None):
    aw, ah, ap = read_png(actual_path)
    gw, gh, gp = read_png(golden_path)
    if (aw, ah) != (gw, gh):
        return {
            "match": False,
            "reason": f"size mismatch: actual {aw}x{ah} vs golden {gw}x{gh}",
            "changed": aw * ah,
        }
    changed = 0
    diff_pixels = []
    for (ar, ag, ab, aa), (gr, gg, gb, ga) in zip(ap, gp):
        d = max(abs(ar - gr), abs(ag - gg), abs(ab - gb), abs(aa - ga))
        if d > tolerance:
            changed += 1
            diff_pixels.append((255, 0, 255, 255))  # magenta highlight
        else:
            diff_pixels.append((gr, gg, gb, max(40, ga // 4) if ga else 0))  # dim ghost
    if out_path and changed:
        write_png(out_path, gw, gh, diff_pixels)
    return {"match": changed == 0, "changed": changed, "total": aw * ah, "tolerance": tolerance}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("actual")
    ap.add_argument("golden")
    ap.add_argument("--tolerance", type=int, default=0)
    ap.add_argument("--out", default=None)
    args = ap.parse_args()
    out = args.out or (os.path.splitext(args.actual)[0] + ".diff.png")
    result = diff(args.actual, args.golden, args.tolerance, out)
    status = "MATCH" if result["match"] else "DIFF"
    print(f"[{status}] {args.actual} vs {args.golden}: {result}")
    sys.exit(0 if result["match"] else 1)


if __name__ == "__main__":
    main()
