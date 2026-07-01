#!/usr/bin/env python3
"""Pre-export technical-correctness guard — deterministic, stdlib-only.

`/pixel-export` advises "integer scale only (2x/3x/4x)" in prose, but nothing
*verifies* it: a fractional export scale silently destroys the pixel grid (the
single most common end-of-pipeline ruin). This is a tiny gate for the one thing
that is unambiguously correct/incorrect — the export scale — plus the engine
guidance every pixel-art export needs (Nearest filter, integer scaling).

Intentionally narrow: it does NOT opine on canvas being power-of-two (individual
sprites usually aren't) or VFX frame counts (that's the opt-in combat/VFX
profile). Honest > noisy.

Usage:
    python tools/export_guard.py --scale 4 [--width 32 --height 32] [--color-mode indexed]
    python tools/export_guard.py --selftest

Exit code: 0 if no error-level findings; 1 otherwise. Prints a JSON report.
"""
import argparse
import json
import sys

ENGINE_GUIDANCE = (
    "Set the in-engine texture filter to Nearest/Point and only scale by integer "
    "multiples; never let the engine bilinear-filter or fractionally scale pixel art."
)


def guard(scale, width=None, height=None, color_mode=None):
    """Return (findings, counts). Pure. `scale` may be int or float."""
    findings = []
    try:
        s = float(scale)
    except (TypeError, ValueError):
        findings.append({"type": "invalid_scale", "severity": "error", "scale": scale})
        s = None

    if s is not None:
        if s <= 0:
            findings.append({"type": "nonpositive_scale", "severity": "error", "scale": scale})
        elif not s.is_integer():
            findings.append({"type": "non_integer_scale", "severity": "error", "scale": scale,
                             "fix": "round to an integer multiple (2x/3x/4x)"})

    # Informational only — both indexed and RGBA export are valid; just surface it.
    if color_mode is not None and color_mode.lower() not in ("indexed", "rgba", "rgb", "grayscale", "gray"):
        findings.append({"type": "unknown_color_mode", "severity": "warn", "color_mode": color_mode})

    counts = {}
    for fi in findings:
        counts[fi["type"]] = counts.get(fi["type"], 0) + 1
    return findings, counts


def _errors(findings):
    return [f for f in findings if f.get("severity") != "warn"]


def _selftest():
    assert _errors(guard(4)[0]) == []
    assert _errors(guard(2.0)[0]) == []                      # integer-valued float is fine
    assert guard(1.5)[1].get("non_integer_scale") == 1
    assert guard(0)[1].get("nonpositive_scale") == 1
    assert guard("x")[1].get("invalid_scale") == 1
    assert guard(3, color_mode="indexed")[1] == {}
    print(json.dumps({"selftest": "ok"}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Validate a pixel-art export is technically correct.")
    ap.add_argument("--scale", default=None)
    ap.add_argument("--width", type=int, default=None)
    ap.add_argument("--height", type=int, default=None)
    ap.add_argument("--color-mode", default=None)
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        _selftest()
        return 0
    if args.scale is None:
        ap.error("give --scale (or --selftest)")

    findings, counts = guard(args.scale, args.width, args.height, args.color_mode)
    report = {
        "scale": args.scale,
        "ok": len(_errors(findings)) == 0,
        "counts": counts,
        "findings": findings,
        "engineGuidance": ENGINE_GUIDANCE,
    }
    print(json.dumps(report, indent=2))
    return 1 if _errors(findings) else 0


if __name__ == "__main__":
    sys.exit(main())
