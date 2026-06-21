#!/usr/bin/env python3
"""Silhouette-IoU drift metric for animation (SPEC-007 Phase 1, roadmap #9).

Cross-frame **proportion drift** is SwordsBench's #1 animation failure: the
character's body volume/shape jumps between frames. This measures it objectively —
no Aseprite, no LLM — so it can be a *hard CI gate* (the donut-test antidote, §F).

A frame's **silhouette mask** = the set of its non-transparent pixels. Between two
frames, `iou = |A ∩ B| / |A ∪ B|` (1.0 = identical silhouette, 0.0 = disjoint). For an
animation, report each adjacent-pair IoU + the minimum; a sudden low IoU = drift.

For **high-motion** tags (an attack lunge *should* move a lot), compare silhouette
bounding-box **area** stability (volume preserved) instead of raw overlap — that
tolerates translation but still catches a volume blow-up.

Frames come from a horizontal film-strip PNG (N frames of equal width, optional gap)
or an explicit list of frame PNGs. Stdlib-only (reuses tools/pixelpng.py); no new dep.

Usage:
    python tools/silhouette_iou.py strip.png --frame-width 24 [--gap 0] [--min-iou 0.8] [--high-motion]
    python tools/silhouette_iou.py --frames a.png b.png c.png [--min-iou 0.8]
    python tools/silhouette_iou.py --selftest
Prints a JSON report; exit 1 if the minimum is below --min-iou (so it doubles as a gate).
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402


def mask(px, w, h, x0=0, frame_w=None):
    """Silhouette mask (set of (x,y) with alpha>0) of a w×h frame, or of the
    [x0, x0+frame_w) column band of a wider strip."""
    fw = frame_w if frame_w is not None else w
    out = set()
    for y in range(h):
        row = y * w
        for x in range(fw):
            if px[row + x0 + x][3] > 0:
                out.add((x, y))
    return frozenset(out)


def iou(a, b):
    """Intersection-over-union of two masks. Two empty masks are identical (1.0)."""
    if not a and not b:
        return 1.0
    union = len(a | b)
    return len(a & b) / union if union else 1.0


def _bbox_area(m):
    if not m:
        return 0
    xs = [x for x, _ in m]
    ys = [y for _, y in m]
    return (max(xs) - min(xs) + 1) * (max(ys) - min(ys) + 1)


def area_ratio(a, b):
    """Translation-invariant volume stability: min/max of the two silhouette
    bbox areas (1.0 = same size, low = a blow-up/shrink). For high-motion tags."""
    aa, bb = _bbox_area(a), _bbox_area(b)
    if aa == 0 and bb == 0:
        return 1.0
    hi = max(aa, bb)
    return min(aa, bb) / hi if hi else 1.0


def strip_masks(w, h, px, frame_w, gap=0):
    """Slice a horizontal film-strip into per-frame silhouette masks."""
    stride = frame_w + gap
    if stride <= 0:
        raise ValueError("frame_width + gap must be > 0")
    n = (w + gap) // stride
    return [mask(px, w, h, x0=k * stride, frame_w=frame_w) for k in range(n)]


def series(masks, high_motion=False):
    """Per-adjacent-pair metric + the minimum across the loop."""
    metric = area_ratio if high_motion else iou
    pairs = [round(metric(a, b), 4) for a, b in zip(masks, masks[1:])]
    return {
        "frames": len(masks),
        "metric": "bbox_area_ratio" if high_motion else "iou",
        "pairs": pairs,
        "min": min(pairs) if pairs else 1.0,
    }


def _selftest():
    sq = frozenset({(0, 0), (0, 1), (1, 0), (1, 1)})  # a 2×2 block, 4 px
    assert iou(sq, sq) == 1.0
    # 1px right-shift: |∩| = 2, |∪| = 6 → exactly 1/3 (hand-computable).
    shifted = frozenset({(x + 1, y) for x, y in sq})
    assert abs(iou(sq, shifted) - 1 / 3) < 1e-9, iou(sq, shifted)
    assert iou(sq, frozenset({(10, 10)})) == 0.0  # disjoint
    assert iou(frozenset(), frozenset()) == 1.0  # both empty
    # high-motion: a 2×2 vs a 4×2 (double bbox area) → 4/8 = 0.5.
    wide = frozenset({(x, y) for x in range(4) for y in range(2)})
    assert abs(area_ratio(sq, wide) - 0.5) < 1e-9, area_ratio(sq, wide)
    # strip slicing: a 6×1 strip of 3 frames (width 2) → 3 masks, 2 pairs.
    O, T = (0, 0, 0, 255), (0, 0, 0, 0)
    px = [O, T, T, T, O, O]  # f0={(0,0)}, f1={}, f2={(0,0),(1,0)}
    masks = strip_masks(6, 1, px, frame_w=2)
    assert len(masks) == 3 and masks[0] == frozenset({(0, 0)}) and masks[1] == frozenset()
    r = series(masks)
    assert r["frames"] == 3 and len(r["pairs"]) == 2 and r["min"] == 0.0
    print(json.dumps({"selftest": "ok"}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Silhouette-IoU animation drift metric.")
    ap.add_argument("strip", nargs="?", help="a horizontal film-strip PNG")
    ap.add_argument("--frame-width", type=int, help="frame width in px (for a strip)")
    ap.add_argument("--gap", type=int, default=0, help="px between frames in the strip")
    ap.add_argument("--frames", nargs="+", help="explicit list of frame PNGs instead of a strip")
    ap.add_argument("--min-iou", type=float, default=0.80, help="drift floor; exit 1 if min below it")
    ap.add_argument("--high-motion", action="store_true", help="use bbox-area stability (lunge/attack)")
    ap.add_argument("--selftest", action="store_true", help="run pure-logic asserts and exit")
    args = ap.parse_args(argv)

    if args.selftest:
        _selftest()
        return 0

    if args.frames:
        masks = []
        for f in args.frames:
            w, h, px = read_png(f)
            masks.append(mask(px, w, h))
    elif args.strip:
        if not args.frame_width:
            ap.error("--frame-width is required with a strip")
        w, h, px = read_png(args.strip)
        masks = strip_masks(w, h, px, args.frame_width, args.gap)
    else:
        ap.error("give a strip PNG (+ --frame-width) or --frames a.png b.png ...")

    r = series(masks, high_motion=args.high_motion)
    r["min_iou_floor"] = args.min_iou
    r["pass"] = r["min"] >= args.min_iou
    print(json.dumps(r, indent=2))
    return 0 if r["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
