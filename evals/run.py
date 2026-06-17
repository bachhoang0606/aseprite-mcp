#!/usr/bin/env python3
"""Tier-A eval harness (checklist 9.4). Deterministic, stdlib-only, no Aseprite.

Runs automatable graded checks for the skills/agents/hooks and reports pass/fail
plus per-component coverage. Tier-B (LLM-judged, live) cases are out of scope here
and documented in evals/README.md.

    python evals/run.py        # exit 0 if all pass, 1 otherwise
"""
import colorsys
import importlib.util
import json
import os
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
sys.path.insert(0, os.path.join(ROOT, "tools"))
from pixelpng import read_png  # noqa: E402
import lint_sprite  # noqa: E402

VISUAL = os.path.join(ROOT, "tests", "visual")
PALETTE = os.path.join(ROOT, "knowledge", "palettes", "goblin-default.json")


def _load_module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _hue_deg(hex_color):
    h = hex_color.lstrip("#")
    r, g, b = int(h[0:2], 16) / 255, int(h[2:4], 16) / 255, int(h[4:6], 16) / 255
    hue, _s, v = colorsys.rgb_to_hsv(r, g, b)
    return hue * 360.0, v


# ---- checks: each returns (ok: bool, detail: str) ----

def check_palette_hueshift():
    with open(PALETTE, encoding="utf-8") as f:
        ramp = json.load(f)["ramps"]["skin"]
    hv = [_hue_deg(c) for c in ramp]
    hues = [h for h, _ in hv]
    vals = [v for _, v in hv]
    spread = max(hues) - min(hues)
    monotonic = all(vals[i] < vals[i + 1] for i in range(len(vals) - 1))
    ok = spread >= 8.0 and monotonic
    return ok, f"hue spread={spread:.1f}deg (need >=8), value-monotonic={monotonic}"


def check_guard_decisions():
    guard = _load_module(os.path.join(ROOT, "hooks", "guard_batch_draw.py"), "guard")
    expect = {
        "mcp__aseprite__draw_rectangle": True,
        "mcp__aseprite__fill_area": True,
        "mcp__aseprite__create_canvas": True,
        "mcp__aseprite-live__draw_pixels": True,
        # Batch counterparts of live painting tools are gated (silent disk edits).
        "mcp__aseprite-live__use_tool": True,
        "mcp__aseprite-live__new_cel": True,
        # Metadata creation stays allowed (no pixels painted).
        "mcp__aseprite-live__create_tag": False,
        "mcp__aseprite-live__create_slice": False,
        # Destructive batch ops (no undo on disk) are gated too (10.4).
        "mcp__aseprite__clear_cel": True,
        "mcp__aseprite__remove_frame": True,
        "mcp__aseprite-live__delete_slice": True,
        "mcp__aseprite-live__live_draw_pixels": False,
        "mcp__aseprite-live__live_use_tool": False,
        # live destructive ops stay allowed: undoable in Aseprite (ADR-0003).
        "mcp__aseprite-live__live_clear_cel": False,
        "mcp__aseprite-live__live_delete_tag": False,
        "mcp__aseprite__export_sprite": False,
        "mcp__aseprite-live__get_sprite_info": False,
    }
    bad = [t for t, want in expect.items() if guard.is_blocked(t) != want]
    return len(bad) == 0, ("all correct" if not bad else f"wrong: {bad}")


def _lint(name):
    w, h, px = read_png(os.path.join(VISUAL, "fixtures", name))
    pal = lint_sprite.load_palette(PALETTE)
    findings, counts, _ = lint_sprite.lint(w, h, px, palette=pal)
    return findings, counts


def check_linter_good():
    findings, _ = _lint("good_swatch.png")
    return len(findings) == 0, f"{len(findings)} findings (want 0)"


def check_linter_offpalette():
    _, counts = _lint("bad_offpalette.png")
    return counts.get("off_palette", 0) >= 1, f"off_palette={counts.get('off_palette', 0)}"


def check_linter_orphan():
    _, counts = _lint("bad_orphan.png")
    return counts.get("orphan", 0) >= 1, f"orphan={counts.get('orphan', 0)}"


def _diff(actual, golden):
    diffmod = _load_module(os.path.join(VISUAL, "diff.py"), "diffmod")
    return diffmod.diff(
        os.path.join(VISUAL, actual), os.path.join(VISUAL, golden), tolerance=0, out_path=None
    )


def check_visual_golden_match():
    r = _diff("fixtures/good_swatch.png", "golden/good_swatch.png")
    return r["match"], f"changed={r.get('changed')}"


def check_visual_detects_change():
    r = _diff("fixtures/bad_offpalette.png", "golden/good_swatch.png")
    return (not r["match"]) and r["changed"] >= 1, f"changed={r.get('changed')} (want >=1)"


def check_tier_b_cases_wellformed():
    judge = _load_module(os.path.join(ROOT, "evals", "judge.py"), "judge")
    return judge.validate()


def check_health_check_json():
    out = subprocess.run(
        [sys.executable, os.path.join(ROOT, "hooks", "health_check.py")],
        capture_output=True, text=True, timeout=15,
    ).stdout
    try:
        data = json.loads(out)
        ok = "additionalContext" in data.get("hookSpecificOutput", {})
        return ok, "valid SessionStart JSON" if ok else "missing additionalContext"
    except (json.JSONDecodeError, ValueError) as e:
        return False, f"invalid JSON: {e}"


CHECKS = {
    "palette_hueshift": check_palette_hueshift,
    "guard_decisions": check_guard_decisions,
    "linter_good": check_linter_good,
    "linter_offpalette": check_linter_offpalette,
    "linter_orphan": check_linter_orphan,
    "visual_golden_match": check_visual_golden_match,
    "visual_detects_change": check_visual_detects_change,
    "health_check_json": check_health_check_json,
    "tier_b_cases_wellformed": check_tier_b_cases_wellformed,
}


def main():
    with open(os.path.join(ROOT, "evals", "cases.json"), encoding="utf-8") as f:
        cases = {c["id"]: c for c in json.load(f)["cases"]}

    passed = 0
    covered = set()
    print("== Tier-A eval harness ==")
    for cid, fn in CHECKS.items():
        try:
            ok, detail = fn()
        except Exception as e:  # noqa: BLE001
            ok, detail = False, f"ERROR: {e}"
        mark = "PASS" if ok else "FAIL"
        print(f"  [{mark}] {cid}: {detail}")
        if ok:
            passed += 1
            covered.update(cases.get(cid, {}).get("covers", []))

    total = len(CHECKS)
    print(f"\n{passed}/{total} checks passed")
    print("Covered components:", ", ".join(sorted(covered)) or "none")
    sys.exit(0 if passed == total else 1)


if __name__ == "__main__":
    main()
