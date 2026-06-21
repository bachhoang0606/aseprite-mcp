#!/usr/bin/env python3
"""Derive a machine-checkable StyleProfile from a reference sprite (SPEC-008, roadmap #11).

Turns "match my hero sheet" into a deterministic contract: extract the palette, sort it
into ramps, and read off light direction, head proportion, and outline policy — so
rig-builder / animation-director can consume it as hard constraints and the linter can
check against it (§G). Phase 1 (this build) is pure Python on a native-resolution
reference; grid auto-detect + the live tool are Phase 2.

  python tools/style_profile.py ref.png [--colors N] [--save profile.json]
  python tools/style_profile.py --selftest

Stdlib-only — reuses tools/{pixelpng,extract_palette,ramp_lint}.py. No new dependency.
"""
import argparse
import colorsys
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pixelpng import read_png  # noqa: E402
import extract_palette  # noqa: E402
import ramp_lint  # noqa: E402


def _hsv(rgb):
    return colorsys.rgb_to_hsv(rgb[0] / 255, rgb[1] / 255, rgb[2] / 255)


def _hue_role(rgb):
    """Heuristic material role from a colour's hue/saturation (overridable in the JSON)."""
    h, s, v = _hsv(rgb)
    if v < 0.12:
        return "outline"
    if s < 0.12:
        return "neutral"  # greys/metal
    deg = h * 360.0
    if deg < 20 or deg >= 330:
        return "red"
    if deg < 45:
        return "leather"  # orange/brown
    if deg < 70:
        return "gold"
    if deg < 170:
        return "skin"  # the goblin greens
    if deg < 200:
        return "cyan"
    if deg < 260:
        return "blue"
    return "magenta"


def ramp_sort(palette_hex):
    """Group a flat palette into ramps by hue role, ordered dark→light, lint each."""
    groups = {}
    for hx in palette_hex:
        groups.setdefault(_hue_role(ramp_lint._rgb(hx)), []).append(hx)
    ramps = []
    for role, colors in groups.items():
        colors = sorted(colors, key=lambda c: ramp_lint._luma(ramp_lint._rgb(c)))
        ramp = {"role": role, "colors": colors, "length": len(colors)}
        if len(colors) >= 2:
            ramp["lint"] = ramp_lint.lint_ramp(colors)["score"]
        ramps.append(ramp)
    return sorted(ramps, key=lambda r: -r["length"])


def _opaque(px, w, h):
    return [(x, y, (r, g, b)) for i, (r, g, b, a) in enumerate(px) if a > 0
            for x in [i % w] for y in [i // w]]


def light_dir(px, w, h):
    """Compare mean luma of opaque pixels in the top-left vs bottom-right quadrants."""
    tl = [ramp_lint._luma(c) for (x, y, c) in _opaque(px, w, h) if x < w / 2 and y < h / 2]
    br = [ramp_lint._luma(c) for (x, y, c) in _opaque(px, w, h) if x >= w / 2 and y >= h / 2]
    if not tl or not br:
        return "unknown"
    return "top-left" if sum(tl) / len(tl) >= sum(br) / len(br) else "bottom-right"


def heads_tall(px, w, h):
    """Silhouette height ÷ head height (top rows narrower than 0.7× the max body width)."""
    rows = {}
    top = bottom = None
    for (x, y, _c) in _opaque(px, w, h):
        rows.setdefault(y, []).append(x)
        top = y if top is None else min(top, y)
        bottom = y if bottom is None else max(bottom, y)
    if top is None:
        return None
    widths = {y: (max(xs) - min(xs) + 1) for y, xs in rows.items()}
    max_w = max(widths.values())
    head_h = 0
    for y in range(top, bottom + 1):
        if widths.get(y, 0) <= 0.7 * max_w:
            head_h += 1
        else:
            break
    total_h = bottom - top + 1
    return round(total_h / head_h, 2) if head_h else None


def outline_policy(px, w, h):
    """Sample silhouette-boundary colours: one dominant dark colour → uniform; else selective."""
    opaque = {(x, y): c for (x, y, c) in _opaque(px, w, h)}
    boundary = []
    for (x, y), c in opaque.items():
        if any((x + dx, y + dy) not in opaque for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
            boundary.append(c)
    if not boundary:
        return "none"
    dark = [c for c in boundary if ramp_lint._luma(c) < 80]
    if not dark:
        return "none"
    from collections import Counter
    (top_c, cnt) = Counter(dark).most_common(1)[0]
    if cnt >= 0.5 * len(boundary):
        return "uniform #%02x%02x%02x" % top_c
    return "selective"


def derive(path, colors=12):
    w, h, px = read_png(path)
    samples = extract_palette.opaque_samples(px)
    palette = ["#%02x%02x%02x" % c for c in extract_palette.frequency(samples, colors)]
    return {
        "source": os.path.basename(path),
        "size": [w, h],
        "grid": None,            # Phase 2 (Sobel auto-detect)
        "frame_counts": None,    # Phase 2
        "palette": palette,
        "ramps": ramp_sort(palette),
        "light_dir": light_dir(px, w, h),
        "heads_tall": heads_tall(px, w, h),
        "outline_policy": outline_policy(px, w, h),
    }


def _selftest():
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    from pixelpng import write_png
    # Synthetic 16×24 figure: a 6-wide head (rows 0-7) over a 12-wide body (rows 8-23),
    # brighter on the LEFT (light from top-left), green body + dark outline col.
    W, H = 16, 24
    T = (0, 0, 0, 0)
    px = [T] * (W * H)
    def put(x, y, c):
        if 0 <= x < W and 0 <= y < H:
            px[y * W + x] = c
    for y in range(H):
        if y < 8:
            x0, x1, wmid = 5, 11, 8        # narrow head
        else:
            x0, x1, wmid = 2, 14, 8        # wider body
        for x in range(x0, x1):
            # left half brighter (light_dir top-left), green material
            base = (60, 180, 70, 255) if x < wmid else (40, 120, 50, 255)
            put(x, y, base)
    os.makedirs("C:/tmp/refmotion", exist_ok=True)
    write_png("C:/tmp/refmotion/_sp_ref.png", W, H, px)
    p = derive("C:/tmp/refmotion/_sp_ref.png", colors=8)
    assert p["light_dir"] == "top-left", p["light_dir"]
    assert p["heads_tall"] and 2.5 <= p["heads_tall"] <= 3.5, p["heads_tall"]  # 24/8 = 3
    assert any(r["role"] == "skin" for r in p["ramps"]), p["ramps"]
    os.remove("C:/tmp/refmotion/_sp_ref.png")
    print(json.dumps({"selftest": "ok", "light_dir": p["light_dir"], "heads_tall": p["heads_tall"]}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Derive a StyleProfile from a reference sprite.")
    ap.add_argument("reference", nargs="?", help="reference PNG (native resolution)")
    ap.add_argument("--colors", type=int, default=12, help="palette size to extract")
    ap.add_argument("--save", help="write the StyleProfile JSON here")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        _selftest()
        return 0
    if not args.reference:
        ap.error("give a reference PNG or --selftest")
    profile = derive(args.reference, args.colors)
    out = json.dumps(profile, indent=2)
    if args.save:
        with open(args.save, "w", encoding="utf-8") as f:
            f.write(out + "\n")
        print(f"wrote {args.save}")
    else:
        print(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
