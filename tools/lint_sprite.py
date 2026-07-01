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


SCALE_HIERARCHY = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "knowledge",
    "scale-hierarchy.json",
)


def load_scale_hierarchy(path=SCALE_HIERARCHY):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)["tiers"]


def derive_budget(longer_side, tiers=None):
    """Colour budget for a sprite whose longer side is `longer_side` px: the first
    tier whose max_side >= that side (knowledge/scale-hierarchy.json). Returns
    (budget, tier) so callers can surface the tier note."""
    if tiers is None:
        tiers = load_scale_hierarchy()
    for tier in tiers:
        if longer_side <= tier["max_side"]:
            return tier["color_budget"], tier
    return tiers[-1]["color_budget"], tiers[-1]


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
    ap.add_argument("--no-auto-budget", action="store_true",
                    help="don't derive a colour budget from canvas size when --budget is omitted")
    ap.add_argument("--warn-only", action="store_true")
    args = ap.parse_args()

    palette = load_palette(args.palette) if args.palette else None
    width, height, pixels = read_png(args.image)

    # Auto-derive the colour budget from the sprite's size (scale hierarchy) when the
    # caller didn't pin one, so "too many colours for this size" is gated by default.
    budget = args.budget
    budget_auto = False
    budget_note = None
    if budget is None and not args.no_auto_budget:
        budget, tier = derive_budget(max(width, height))
        budget_auto = True
        budget_note = tier.get("note")

    findings, counts, distinct = lint(width, height, pixels, palette, budget)

    report = {
        "image": args.image,
        "size": [width, height],
        "distinctOpaqueColors": distinct,
        "budget": budget,
        "budgetAuto": budget_auto,
        "budgetNote": budget_note,
        "ok": len(findings) == 0,
        "counts": counts,
        "findings": findings[:200],
    }
    print(json.dumps(report, indent=2))
    if findings and not args.warn_only:
        sys.exit(1)


if __name__ == "__main__":
    main()
