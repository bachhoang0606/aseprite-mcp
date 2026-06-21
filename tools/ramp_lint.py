#!/usr/bin/env python3
"""Ramp-quality lint — an objective scoring axis for colour ramps (SPEC-008, roadmap #11).

A *ramp* is an ordered dark→light sequence of colours for one material. "Good" ramps
follow codifiable rules (`rules/01-palette-and-color.md` + SLYNYRD), so ramp quality is a
**deterministic** eval axis, not vibes — the keystone of the StyleProfile contract and a
SPEC-007 gate. Each rule emits a finding when it fails:

  value_monotonic (MUST): luma strictly increases dark→light.
  hue_shift       : hue rotates across the ramp, darker cooler / lighter warmer (rules/01 §3).
  mid_peaked_sat  : saturation peaks in the middle steps, not at an endpoint (SLYNYRD).
  no_max_corner   : no step at both near-max saturation AND near-max value (SLYNYRD).
  length          : 3–5 steps (rules/01 §2; 2 = flat, >5 = wasteful).

A ramp PASSES when score >= 0.70 AND value_monotonic holds. Stdlib-only; no Aseprite, no dep.

Usage:
  python tools/ramp_lint.py knowledge/palettes/goblin-default.json [--ramp skin]
  python tools/ramp_lint.py --colors "#1b4d3e,#2e7d32,#4ca02c,#6abe30,#a6d94a"
  python tools/ramp_lint.py --selftest
Prints a JSON report; exit 1 if any linted ramp fails.
"""
import argparse
import colorsys
import json
import math
import sys

PASS_FLOOR = 0.70
WEIGHTS = {"value_monotonic": 0.30, "hue_shift": 0.30, "mid_peaked_sat": 0.15,
           "no_max_corner": 0.15, "length": 0.10}


def _rgb(hexs):
    h = hexs.lstrip("#")
    return int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)


def _hsv(rgb):
    return colorsys.rgb_to_hsv(rgb[0] / 255, rgb[1] / 255, rgb[2] / 255)  # h,s,v in 0..1


def _luma(rgb):
    return 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]


def _warmth(h_deg):
    """How warm a hue is: +1 at orange (~45°), −1 at blue (~225°)."""
    return math.cos(math.radians(h_deg - 45.0))


def _circular_span(hues_deg):
    """Span (deg) the hues occupy on the colour wheel = 360 − largest gap."""
    if len(hues_deg) < 2:
        return 0.0
    hs = sorted(hues_deg)
    gaps = [hs[i + 1] - hs[i] for i in range(len(hs) - 1)] + [360 - hs[-1] + hs[0]]
    return 360.0 - max(gaps)


def lint_ramp(colors):
    """Score a dark→light hex ramp. Returns {score, pass, findings:[{id, ok, detail}]}."""
    n = len(colors)
    rgb = [_rgb(c) for c in colors]
    hsv = [_hsv(c) for c in rgb]
    luma = [_luma(c) for c in rgb]
    hues = [h * 360.0 for (h, _s, _v) in hsv]
    sat = [s for (_h, s, _v) in hsv]
    val = [v for (_h, _s, v) in hsv]
    f = {}

    # value_monotonic (must-pass): luma strictly increases.
    inc = sum(1 for i in range(n - 1) if luma[i + 1] > luma[i])
    vm = inc / (n - 1) if n > 1 else 1.0
    f["value_monotonic"] = (vm >= 0.999, f"{inc}/{max(n-1,1)} steps increase in luma")

    # hue_shift: rotation present AND warmer as it lightens.
    span = _circular_span(hues)
    warmth = [_warmth(h) for h in hues]
    warm_ok = sum(1 for i in range(n - 1) if warmth[i + 1] >= warmth[i] - 1e-9)
    warm_frac = warm_ok / (n - 1) if n > 1 else 1.0
    # Warmth only counts when there is real hue rotation (else a grey ramp scores 0).
    hue_score = min(span / 10.0, 1.0) * (0.5 + 0.5 * warm_frac)
    f["hue_shift"] = (hue_score >= 0.6, f"hue span {span:.0f}deg, warmer-as-lighter {warm_frac:.0%}")

    # mid_peaked_sat: saturation peaks in the interior, not at an endpoint.
    if n >= 3:
        peak = max(range(n), key=lambda i: sat[i])
        mp = 0 < peak < n - 1
    else:
        mp = True  # n/a for a 2-step ramp
    f["mid_peaked_sat"] = (mp, "saturation peaks mid-ramp" if mp else "saturation peaks at an endpoint")

    # no_max_corner: no step at both near-max saturation and near-max value.
    corner = [i for i in range(n) if sat[i] > 0.9 and val[i] > 0.95]
    f["no_max_corner"] = (not corner, "no max-sat+max-value step" if not corner else f"step(s) {corner} at the garish corner")

    # length: 3–5 steps.
    f["length"] = (3 <= n <= 5, f"{n} steps")

    score = sum(WEIGHTS[k] * (1.0 if ok else 0.0) for k, (ok, _d) in f.items())
    # hue_shift contributes its graded score, not just pass/fail, for a smoother metric.
    score = score - WEIGHTS["hue_shift"] * (1.0 if f["hue_shift"][0] else 0.0) + WEIGHTS["hue_shift"] * hue_score
    findings = [{"id": k, "ok": ok, "detail": d} for k, (ok, d) in f.items()]
    passed = score >= PASS_FLOOR and f["value_monotonic"][0]
    return {"steps": n, "score": round(score, 3), "pass": passed, "findings": findings}


def lint_palette_ramps(ramps):
    """ramps: {role: [hex...]}. Returns {role: report}."""
    return {role: lint_ramp(colors) for role, colors in ramps.items() if len(colors) >= 2}


def _selftest():
    good = ["#1b4d3e", "#2e7d32", "#4ca02c", "#6abe30", "#a6d94a"]  # goblin skin
    r = lint_ramp(good)
    assert r["pass"] and r["score"] >= 0.7, r
    gray = ["#222222", "#555555", "#888888", "#bbbbbb", "#eeeeee"]  # value-only
    r = lint_ramp(gray)
    assert not _ok(r, "hue_shift") and not r["pass"], r
    short = ["#222222", "#cccccc"]
    assert not _ok(lint_ramp(short), "length"), "2-step should fail length"
    corner = ["#1b4d3e", "#2e7d32", "#00ff00"]  # #00ff00 = max sat + max value
    assert not _ok(lint_ramp(corner), "no_max_corner"), "pure #00ff00 is the garish corner"
    nonmono = ["#a6d94a", "#2e7d32", "#1b4d3e"]  # light→dark (reversed)
    r = lint_ramp(nonmono)
    assert not _ok(r, "value_monotonic") and not r["pass"], "non-monotonic must fail"
    print(json.dumps({"selftest": "ok"}))


def _ok(report, fid):
    return next(f["ok"] for f in report["findings"] if f["id"] == fid)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Lint colour-ramp quality.")
    ap.add_argument("palette", nargs="?", help="a palette JSON with a `ramps` map")
    ap.add_argument("--ramp", help="lint only this ramp role")
    ap.add_argument("--colors", help="lint a single comma-separated hex ramp")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        _selftest()
        return 0
    if args.colors:
        report = {"_": lint_ramp([c.strip() for c in args.colors.split(",")])}
    elif args.palette:
        with open(args.palette, encoding="utf-8") as fh:
            ramps = json.load(fh).get("ramps", {})
        if args.ramp:
            ramps = {args.ramp: ramps[args.ramp]}
        report = lint_palette_ramps(ramps)
    else:
        ap.error("give a palette JSON, --colors, or --selftest")
    print(json.dumps(report, indent=2))
    return 0 if all(r["pass"] for r in report.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
