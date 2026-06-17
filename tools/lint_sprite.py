#!/usr/bin/env python3
"""Sprite linter — deterministic, stdlib-only static checks on an exported PNG.

This is the "sprite linter" reinterpretation of the LSP idea (PROJECT_PLAN) and
the engine behind the palette-lint hook (checklist 7.3) and the pixel-critic /
/pixel-review evals (9.4). It only reports defects it can detect *reliably*:

  - off_palette : opaque pixels whose RGB is not in a reference palette.
  - orphan      : opaque pixels with no orthogonal opaque neighbour (stray dot /
                  single-pixel jaggy / diagonal-only attachment).
  - over_budget : more distinct opaque colours than the size budget allows.

It deliberately does NOT claim to auto-detect banding or pillow-shading — those
need a trained eye; pixel-critic (LLM/manual) covers them. Honest > noisy.

Usage:
    python tools/lint_sprite.py sprite.png [--palette knowledge/palettes/x.json]
                                           [--budget 16] [--warn-only]
Exit code: 0 if no error-level findings (or --warn-only); 1 otherwise.
Always prints a JSON report to stdout.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402


def hex_to_rgb(h: str):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


def load_palette(path):
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    return {hex_to_rgb(c) for c in data.get("colors", [])}


def lint(width, height, pixels, palette=None, budget=None):
    findings = []
    opaque = set()
    colors = {}
    for i, (r, g, b, a) in enumerate(pixels):
        if a == 0:
            continue
        x, y = i % width, i // width
        opaque.add((x, y))
        colors[(r, g, b)] = colors.get((r, g, b), 0) + 1
        if palette is not None and (r, g, b) not in palette:
            findings.append({"type": "off_palette", "x": x, "y": y, "color": "#%02x%02x%02x" % (r, g, b)})

    for (x, y) in opaque:
        n4 = (
            ((x - 1, y) in opaque)
            + ((x + 1, y) in opaque)
            + ((x, y - 1) in opaque)
            + ((x, y + 1) in opaque)
        )
        if n4 == 0:
            findings.append({"type": "orphan", "x": x, "y": y})

    if budget is not None and len(colors) > budget:
        findings.append(
            {"type": "over_budget", "distinct_colors": len(colors), "budget": budget}
        )

    counts = {}
    for f in findings:
        counts[f["type"]] = counts.get(f["type"], 0) + 1
    return findings, counts, len(colors)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--palette", default=None)
    ap.add_argument("--budget", type=int, default=None)
    ap.add_argument("--warn-only", action="store_true")
    args = ap.parse_args()

    palette = load_palette(args.palette) if args.palette else None
    width, height, pixels = read_png(args.image)
    findings, counts, distinct = lint(width, height, pixels, palette, args.budget)

    report = {
        "image": args.image,
        "size": [width, height],
        "distinctOpaqueColors": distinct,
        "ok": len(findings) == 0,
        "counts": counts,
        "findings": findings[:200],
    }
    print(json.dumps(report, indent=2))
    if findings and not args.warn_only:
        sys.exit(1)


if __name__ == "__main__":
    main()
