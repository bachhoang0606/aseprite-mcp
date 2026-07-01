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
import timing_lint  # noqa: E402
import export_guard  # noqa: E402

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


# ---- silhouette-IoU animation-drift gate (SPEC-007 Phase 1) ----
SIL_FIXTURES = os.path.join(ROOT, "evals", "fixtures")
SIL_FLOOR = 0.80
SIL_FRAME_W = 24


def _sil_min(name):
    sil = _load_module(os.path.join(ROOT, "tools", "silhouette_iou.py"), "sil")
    w, h, px = read_png(os.path.join(SIL_FIXTURES, name))
    return sil.series(sil.strip_masks(w, h, px, SIL_FRAME_W))["min"]


def check_silhouette_iou_stable():
    m = _sil_min("walk_stable.png")
    return m >= SIL_FLOOR, f"min IoU={m:.3f} (need >= {SIL_FLOOR})"


def check_silhouette_iou_detects_drift():
    m = _sil_min("walk_drift.png")
    return m < SIL_FLOOR, f"min IoU={m:.3f} (want < {SIL_FLOOR}, drift detected)"


def check_tier_b_cases_wellformed():
    judge = _load_module(os.path.join(ROOT, "evals", "judge.py"), "judge")
    return judge.validate()


def check_degradation_slope_math():
    """SPEC-007 Phase 2: the donut-test degradation helper flags a decaying session
    and passes a stable one (deterministic — exercises judge.compute_slope)."""
    judge = _load_module(os.path.join(ROOT, "evals", "judge.py"), "judge")
    stable = [{"checkpoint": p, "linter": 1.0, "min_iou": 0.9, "off_palette": 0} for p in (0, 20, 40, 60)]
    decaying = [
        {"checkpoint": 0, "linter": 1.0, "min_iou": 0.90, "off_palette": 0},
        {"checkpoint": 40, "linter": 0.70, "min_iou": 0.60, "off_palette": 3},
    ]
    s_ok = judge.compute_slope(stable)["regressed"] is False
    d_ok = judge.compute_slope(decaying)["regressed"] is True
    return s_ok and d_ok, f"stable_no_regress={s_ok}, decaying_regress={d_ok}"


def check_tool_select_scorer():
    """Tool-surface measurement: the scorer's token model + accuracy math is sound
    (deterministic — exercises evals/tool_select/score.py --selftest)."""
    import contextlib
    import io
    score = _load_module(os.path.join(ROOT, "evals", "tool_select", "score.py"), "tsscore")
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            rc = score.selftest()
        return rc == 0, "score.py selftest passed"
    except AssertionError as e:
        return False, f"score.py selftest FAILED: {e}"


def check_tool_usage_scorer():
    """Tool-usage correctness: the scorer detects a quality-regressing trim and prices the
    token saving (deterministic — exercises evals/tool_usage/score.py --selftest)."""
    import contextlib
    import io
    score = _load_module(os.path.join(ROOT, "evals", "tool_usage", "score.py"), "tuscore")
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            rc = score.selftest()
        return rc == 0, "score.py selftest passed"
    except AssertionError as e:
        return False, f"score.py selftest FAILED: {e}"


def check_ramp_lint_quality():
    """SPEC-008: the project's own ramps lint well (rules/01 calibration) and a
    value-only grey ramp is flagged — making ramp quality a deterministic axis."""
    rl = _load_module(os.path.join(ROOT, "tools", "ramp_lint.py"), "ramp_lint")
    with open(PALETTE, encoding="utf-8") as f:
        ramps = json.load(f)["ramps"]
    good_ok = all(r["pass"] for r in rl.lint_palette_ramps(ramps).values())
    bad_flagged = not rl.lint_ramp(["#222222", "#555555", "#888888", "#bbbbbb", "#eeeeee"])["pass"]
    return good_ok and bad_flagged, f"goblin ramps pass={good_ok}, value-only flagged={bad_flagged}"


def check_regrid_detects_scale():
    """SPEC-008 Phase 2: the grid auto-detect recovers a 4×-upscale's native cell size
    (de-fake) and leaves genuinely native art at cell 1."""
    rg = _load_module(os.path.join(ROOT, "tools", "regrid.py"), "regrid")
    pal = [(200, 40, 40, 255), (40, 200, 60, 255), (50, 60, 220, 255), (230, 210, 40, 255), (0, 0, 0, 0)]
    base = [pal[((i * 1103515245 + 12345) >> 4) % 5] for i in range(64)]
    native = rg.detect_grid(base, 8, 8)["cell_w"]
    scaled = rg.detect_grid(rg._upscale(base, 8, 8, 4), 32, 32)["cell_w"]
    return native == 1 and scaled == 4, f"native_cell={native}, 4x_cell={scaled}"


def check_timing_lint_good():
    """A1: a well-timed clip (in-band idle, attack with a real >=150ms hold + easing)
    produces zero error-level timing findings against knowledge/timing-budgets.json."""
    budgets = timing_lint.load_budgets()
    findings, _ = timing_lint.lint(
        frames=[200, 220, 200, 220, 60, 80, 180, 120],
        tags=[{"name": "idle", "from": 0, "to": 3, "repeat": 0},
              {"name": "attack", "from": 4, "to": 7, "repeat": 1}],
        budgets=budgets,
    )
    errs = timing_lint._errors(findings)
    return len(errs) == 0, f"{len(errs)} error findings (want 0)"


def check_timing_lint_detects():
    """A1: a jittery idle (too fast), a held-less uniform attack, and an infinitely
    looping death are all flagged — the rules/04 doctrine as a deterministic gate."""
    budgets = timing_lint.load_budgets()
    _, counts = timing_lint.lint(
        frames=[120, 120, 120, 120, 80, 80, 80, 80, 100, 100],
        tags=[{"name": "idle", "from": 0, "to": 3, "repeat": 0},
              {"name": "attack_swing", "from": 4, "to": 7, "repeat": 1},
              {"name": "death", "from": 8, "to": 9, "repeat": 0}],
        budgets=budgets,
    )
    ok = (counts.get("too_fast", 0) >= 4 and counts.get("no_impact_hold", 0) == 1
          and counts.get("loops", 0) == 1)
    return ok, f"too_fast={counts.get('too_fast', 0)}, no_hold={counts.get('no_impact_hold', 0)}, loops={counts.get('loops', 0)}"


def check_timing_lint_live_shape():
    """A1: the linter ingests the RAW live output shape — live_list_frames duration in
    SECONDS + 1-based frameNumber, live_list_tags fromFrame/toFrame/repeats — not just
    the hand-authored ms shape. Proves the data-contract against the plugin, not vibes."""
    budgets = timing_lint.load_budgets()
    # A 0.12s (=120ms) idle is below the 150ms floor -> 4 too_fast; a 0.2s idle passes.
    jitter = timing_lint.lint_clip({
        "frames": [{"frameNumber": i + 1, "duration": 0.12} for i in range(4)],
        "tags": [{"name": "idle", "fromFrame": 1, "toFrame": 4, "repeats": "0", "aniDir": "forward"}],
    }, budgets)[1]
    clean = timing_lint._errors(timing_lint.lint_clip({
        "frames": [{"frameNumber": i + 1, "duration": 0.2} for i in range(4)],
        "tags": [{"name": "idle", "fromFrame": 1, "toFrame": 4, "repeats": "0"}],
    }, budgets)[0])
    ok = jitter.get("too_fast", 0) == 4 and len(clean) == 0
    return ok, f"live 0.12s idle too_fast={jitter.get('too_fast', 0)} (want 4), 0.2s idle errors={len(clean)} (want 0)"


def check_linter_autobudget():
    """A4: the colour budget is derived from canvas size (knowledge/scale-hierarchy.json)
    — small sprites get a tight cap — and an over-cap sprite is flagged via that budget."""
    tiers = lint_sprite.load_scale_hierarchy()
    table = {s: lint_sprite.derive_budget(s, tiers)[0] for s in (16, 24, 32, 64, 128, 300)}
    monotonic = all(table[a] <= table[b] for a, b in zip([16, 24, 32, 64, 128], [24, 32, 64, 128, 300]))
    tight_small = table[16] <= 8 and table[300] >= table[16]
    # A 9-wide, 1-tall strip of 9 distinct colours: longer side 9 -> budget 8 -> over budget.
    px = [(i * 25 % 256, (i * 60) % 256, (i * 90) % 256, 255) for i in range(9)]
    budget = lint_sprite.derive_budget(9, tiers)[0]
    _, counts, _ = lint_sprite.lint(9, 1, px, palette=None, budget=budget)
    flagged = counts.get("over_budget", 0) == 1
    ok = monotonic and tight_small and flagged
    return ok, f"sizes->budget={table}, over_budget_flagged={flagged}"


def check_export_guard():
    """A3: the export guard passes a clean integer scale and rejects a fractional /
    nonpositive one (the pixel-grid-destroying export mistake)."""
    clean = len(export_guard._errors(export_guard.guard(4, 32, 32, "indexed")[0])) == 0
    frac = export_guard.guard(1.5)[1].get("non_integer_scale") == 1
    nonpos = export_guard.guard(0)[1].get("nonpositive_scale") == 1
    ok = clean and frac and nonpos
    return ok, f"clean4x={clean}, rejects_1.5x={frac}, rejects_0x={nonpos}"


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
    "silhouette_iou_stable": check_silhouette_iou_stable,
    "silhouette_iou_detects_drift": check_silhouette_iou_detects_drift,
    "health_check_json": check_health_check_json,
    "tier_b_cases_wellformed": check_tier_b_cases_wellformed,
    "degradation_slope_math": check_degradation_slope_math,
    "ramp_lint_quality": check_ramp_lint_quality,
    "regrid_detects_scale": check_regrid_detects_scale,
    "tool_select_scorer": check_tool_select_scorer,
    "tool_usage_scorer": check_tool_usage_scorer,
    "timing_lint_good": check_timing_lint_good,
    "timing_lint_detects": check_timing_lint_detects,
    "timing_lint_live_shape": check_timing_lint_live_shape,
    "linter_autobudget": check_linter_autobudget,
    "export_guard": check_export_guard,
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
