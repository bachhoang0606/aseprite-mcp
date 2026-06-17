#!/usr/bin/env python3
"""Reference-palette extractor — deterministic, stdlib-only.

Recovers the intended palette of a reference image (concept art, an existing game
sprite, an AI render) so the agent can match an existing style instead of inventing
colours. This is the first step of the "reference -> style profile" workflow
(research doc Path 4 / SPEC pending): extract -> (ramp-sort) -> enforce.

Three methods, because the right one differs per task:
  - frequency  : the N most common colours. Exact + ideal for art that is already
                 limited-palette pixel art (the common case).
  - median_cut : recursively split the colour box on its widest channel. Good
                 general reduction of a photo / gradient-y reference.
  - kmeans     : Lloyd's algorithm (deterministically seeded from median_cut).
                 Lets small-but-important colours survive better than averaging.

Fully-transparent pixels (alpha 0) are ignored. The palette is sorted by luma
(shadow -> highlight); recovering ramp *structure* (collinearity / dendrogram) is
the documented Path-4 follow-up.

Usage:
    python tools/extract_palette.py ref.png [--method frequency|median_cut|kmeans]
                                            [--colors N] [--save out.json]
Prints a JSON report to stdout; --save also writes a lint-compatible palette JSON.
"""
import argparse
import json
import os
import sys
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402

# Cap the working set so a large reference stays fast; pixel art is tiny, this only
# bites on photos. Deterministic stride sampling (no RNG) keeps output reproducible.
MAX_SAMPLES = 50_000


def opaque_samples(pixels):
    """Flat [(r,g,b,a)] -> [(r,g,b)] for alpha>0, stride-capped at MAX_SAMPLES."""
    samples = [(r, g, b) for (r, g, b, a) in pixels if a != 0]
    if len(samples) > MAX_SAMPLES:
        stride = (len(samples) + MAX_SAMPLES - 1) // MAX_SAMPLES
        samples = samples[::stride]
    return samples


def _luma(c):
    return 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]


def frequency(samples, n):
    return [color for color, _ in Counter(samples).most_common(n)]


def median_cut(samples, n):
    if not samples or n < 1:
        return []
    buckets = [list(samples)]
    while len(buckets) < n:
        best_i, best_range, best_ch = -1, -1, 0
        for i, b in enumerate(buckets):
            if len(b) < 2:
                continue
            for ch in range(3):
                vals = [c[ch] for c in b]
                rng = max(vals) - min(vals)
                if rng > best_range:
                    best_range, best_i, best_ch = rng, i, ch
        if best_i < 0:  # nothing left to split
            break
        b = buckets.pop(best_i)
        b.sort(key=lambda c: c[best_ch])
        mid = len(b) // 2
        buckets.append(b[:mid])
        buckets.append(b[mid:])
    return [_avg(b) for b in buckets if b]


def _avg(bucket):
    m = len(bucket)
    return (
        sum(c[0] for c in bucket) // m,
        sum(c[1] for c in bucket) // m,
        sum(c[2] for c in bucket) // m,
    )


def kmeans(samples, n, iters=12):
    if not samples or n < 1:
        return []
    centroids = median_cut(samples, n)  # deterministic seed
    if not centroids:
        return []
    for _ in range(iters):
        clusters = [[] for _ in centroids]
        for c in samples:
            bi, bd = 0, None
            for i, ct in enumerate(centroids):
                d = (c[0] - ct[0]) ** 2 + (c[1] - ct[1]) ** 2 + (c[2] - ct[2]) ** 2
                if bd is None or d < bd:
                    bd, bi = d, i
            clusters[bi].append(c)
        new = [_avg(cl) if cl else centroids[i] for i, cl in enumerate(clusters)]
        if new == centroids:
            break
        centroids = new
    return centroids


METHODS = {"frequency": frequency, "median_cut": median_cut, "kmeans": kmeans}


def extract(pixels, method="frequency", colors=16):
    samples = opaque_samples(pixels)
    palette = METHODS[method](samples, colors)
    palette.sort(key=_luma)
    # dedup while preserving the sorted order (kmeans empty clusters can repeat)
    seen, ordered = set(), []
    for c in palette:
        if c not in seen:
            seen.add(c)
            ordered.append(c)
    return ["#%02X%02X%02X" % c for c in ordered]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("image")
    ap.add_argument("--method", choices=list(METHODS), default="frequency")
    ap.add_argument("--colors", type=int, default=16)
    ap.add_argument("--save", default=None, help="write a lint-compatible palette JSON")
    args = ap.parse_args()

    width, height, pixels = read_png(args.image)
    hexes = extract(pixels, args.method, args.colors)
    report = {
        "image": os.path.basename(args.image),
        "size": [width, height],
        "method": args.method,
        "requested": args.colors,
        "count": len(hexes),
        "colors": hexes,
    }
    if args.save:
        with open(args.save, "w", encoding="utf-8") as f:
            json.dump(
                {
                    "name": f"Extracted from {os.path.basename(args.image)}",
                    "source": f"extract_palette.py --method {args.method} --colors {args.colors}",
                    "colors": hexes,
                },
                f,
                indent=2,
            )
        report["saved"] = args.save
    print(json.dumps(report))


if __name__ == "__main__":
    main()
