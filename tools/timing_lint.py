#!/usr/bin/env python3
"""Animation timing linter — deterministic, stdlib-only checks on frame durations.

The animation rule family (`rules/04-animation.md`) was the one major ruleset still
encoded as PROSE with no machine gate, even though the data is right there in the live
API (`live_list_tags` for the tags, per-frame `duration` from `live_list_frames`). This
turns that doctrine into the same deterministic gate the palette / orphan / silhouette
rules already have (see tools/lint_sprite.py, tools/silhouette_iou.py, evals/run.py).

It only reports defects it can detect *reliably* from timing + tag structure:

  - too_fast / too_slow : a state-tagged frame's ms is outside the per-state band
                          (e.g. idle 120ms -> nervous jitter; run 200ms -> sluggish).
  - no_impact_hold      : an attack tag never holds a frame >= the hold budget
                          (the "this was a real hit, it has weight" beat).
  - uniform_timing      : an easing-expected tag (attack) where every frame is the
                          same duration -> no ease, reads robotic.  [warn]
  - loops               : a death/KO tag set to loop infinitely (repeats == 0).

It deliberately does NOT judge silhouette, anticipation poses, or the white hit-flash
(that is combat_lint / pixel-anim-review). Honest > noisy. Bands live in
knowledge/timing-budgets.json (single source of truth, also tested by evals/run.py).

Usage:
    python tools/timing_lint.py clip.json [--budgets knowledge/timing-budgets.json]
                                          [--warn-only]
    python tools/timing_lint.py --selftest

`clip.json` accepts the RAW live output — paste what `live_list_frames` and
`live_list_tags` return:
    {
      "frames": [{"frameNumber": 1, "duration": 0.12}, ...],   # duration in SECONDS, 1-based
      "tags":   [{"name": "idle", "fromFrame": 1, "toFrame": 4, "repeats": "0", "aniDir": "forward"}]
    }
or the simple hand-authored shape (ms, 0-based, inclusive from/to):
    { "frames": [120, 120, 120, 120], "tags": [{"name": "idle", "from": 0, "to": 3, "repeat": 0}] }

Exit code: 0 if no error-level findings (or --warn-only); 1 otherwise.
Always prints a JSON report to stdout.
"""
import argparse
import json
import os
import sys

DEFAULT_BUDGETS = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "knowledge",
    "timing-budgets.json",
)


def load_budgets(path=DEFAULT_BUDGETS):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def _durations(frames):
    """Simple shape: accept [120, 130] or [{'duration':120}]; return int ms list."""
    return [int(fr["duration"] if isinstance(fr, dict) else fr) for fr in frames]


def _as_repeat(rep):
    """Aseprite Tag.repeats: 0 (or '0') == infinite loop. Unknown/absent -> None."""
    if rep is None:
        return None
    try:
        return int(rep)
    except (TypeError, ValueError):
        return None


def normalize(clip):
    """Accept any of the three real shapes and return (frames_ms, tags) for lint().
    Shape is detected by structure (keys), never by value heuristics:

      1. live_list_frames/_tags : frames [{frameNumber 1-based, duration SECONDS}],
                                  tags [{name, fromFrame, toFrame, repeats}] (0=infinite).
      2. Aseprite JSON export    : frames {name: {duration MS}} (or array), tags in
         (what /pixel-export emits) `meta.frameTags` [{name, from, to, direction}] (0-based, no repeat).
      3. simple / hand-authored  : frames [ms] or [{duration: ms}], tags [{name, from, to, repeat}].
    """
    raw_frames = clip.get("frames", [])
    raw_tags = clip.get("tags")
    if raw_tags is None:  # Aseprite export keeps tags under meta.frameTags
        raw_tags = clip.get("meta", {}).get("frameTags", [])

    if isinstance(raw_frames, dict):
        # Aseprite export: dict keyed by frame name, duration already in ms, file order.
        frames_ms = [int(v.get("duration", 0)) for v in raw_frames.values()]
    elif raw_frames and isinstance(raw_frames[0], dict) and "frameNumber" in raw_frames[0]:
        # live_list_frames: 1-based frameNumber, duration in SECONDS.
        by_num = {int(fr["frameNumber"]): round(float(fr.get("duration", 0)) * 1000)
                  for fr in raw_frames}
        hi = max(by_num) if by_num else 0
        frames_ms = [by_num.get(i + 1, 0) for i in range(hi)]
    elif raw_frames and isinstance(raw_frames[0], dict):
        # Aseprite export array form: [{duration: ms}, ...].
        frames_ms = [int(fr.get("duration", 0)) for fr in raw_frames]
    else:
        frames_ms = _durations(raw_frames)

    tags = []
    for t in raw_tags:
        if "fromFrame" in t or "toFrame" in t:  # live shape (1-based + repeats)
            frm = int(t.get("fromFrame", 1)) - 1
            to = int(t.get("toFrame", t.get("fromFrame", 1))) - 1
            tags.append({"name": t.get("name", ""), "from": frm, "to": to,
                         "repeat": _as_repeat(t.get("repeats", t.get("repeat")))})
        else:  # simple OR meta.frameTags — already name/from/to, 0-based (export has no repeat)
            tags.append(t)
    return frames_ms, tags


def classify(tag_name, states):
    """First state whose `match` keyword is a substring of the lowercased tag name."""
    low = (tag_name or "").lower()
    for state, cfg in states.items():
        for kw in cfg.get("match", []):
            if kw in low:
                return state, cfg
    return None, None


def _loops(tag):
    """Only flag an explicit infinite loop (repeat == 0); absent/finite is not flagged."""
    rep = tag.get("repeat")
    if rep is None:
        return False
    try:
        return int(rep) == 0
    except (TypeError, ValueError):
        return False


def lint(frames, tags, budgets):
    """Return (findings, counts). Pure: `frames` is a 0-based ms list, `tags` use 0-based
    inclusive from/to (run normalize() first on raw live output)."""
    durs = list(frames)
    states = budgets["states"]
    findings = []

    for tag in tags:
        name = tag.get("name", "")
        state, cfg = classify(name, states)
        if cfg is None:
            continue
        lo = int(tag.get("from", 0))
        hi = int(tag.get("to", len(durs) - 1))
        rng = [(i, durs[i]) for i in range(lo, hi + 1) if 0 <= i < len(durs)]
        if not rng:
            continue

        band = cfg.get("per_frame_ms")
        floor = cfg.get("min_frame_ms")
        for idx, ms in rng:
            if band is not None:
                if ms < band[0]:
                    findings.append({"type": "too_fast", "severity": "error", "tag": name,
                                     "state": state, "frame": idx, "ms": ms, "min": band[0]})
                elif ms > band[1]:
                    findings.append({"type": "too_slow", "severity": "error", "tag": name,
                                     "state": state, "frame": idx, "ms": ms, "max": band[1]})
            elif floor is not None and ms < floor:
                findings.append({"type": "too_fast", "severity": "error", "tag": name,
                                 "state": state, "frame": idx, "ms": ms, "min": floor})

        hold = cfg.get("require_hold_ms")
        if hold is not None:
            peak = max(ms for _, ms in rng)
            if peak < hold:
                findings.append({"type": "no_impact_hold", "severity": "error", "tag": name,
                                 "state": state, "max_ms": peak, "need_hold_ms": hold})

        if cfg.get("easing_expected") and len(rng) >= 3 and len({ms for _, ms in rng}) == 1:
            findings.append({"type": "uniform_timing", "severity": "warn", "tag": name,
                             "state": state, "ms": rng[0][1]})

        if cfg.get("must_not_loop") and _loops(tag):
            findings.append({"type": "loops", "severity": "error", "tag": name, "state": state})

    counts = {}
    for fi in findings:
        counts[fi["type"]] = counts.get(fi["type"], 0) + 1
    return findings, counts


def lint_clip(clip, budgets):
    """Convenience: normalize raw live output then lint."""
    frames_ms, tags = normalize(clip)
    return lint(frames_ms, tags, budgets)


def _errors(findings):
    return [f for f in findings if f.get("severity") != "warn"]


def _selftest():
    budgets = load_budgets()
    # Simple shape: clean clip -> no errors.
    good = lint(
        frames=[200, 220, 200, 220, 60, 80, 180, 120],
        tags=[{"name": "idle", "from": 0, "to": 3, "repeat": 0},
              {"name": "attack", "from": 4, "to": 7, "repeat": 1}],
        budgets=budgets,
    )[0]
    assert _errors(good) == [], good

    # Simple shape: bad clip -> jitter + no-hold + loop flagged.
    _, counts = lint(
        frames=[120, 120, 120, 120, 80, 80, 80, 80, 100, 100],
        tags=[{"name": "idle", "from": 0, "to": 3, "repeat": 0},
              {"name": "attack_swing", "from": 4, "to": 7, "repeat": 1},
              {"name": "death", "from": 8, "to": 9, "repeat": 0}],
        budgets=budgets,
    )
    assert counts.get("too_fast", 0) >= 4 and counts.get("no_impact_hold", 0) == 1 \
        and counts.get("uniform_timing", 0) == 1 and counts.get("loops", 0) == 1, counts
    assert lint([100, 100], [{"name": "wobble", "from": 0, "to": 1}], budgets)[0] == []

    # RAW LIVE shape: seconds->ms, 1-based->0-based, fromFrame/toFrame/repeats mapping.
    fms, tgs = normalize({
        "frames": [{"frameNumber": 1, "duration": 0.12}, {"frameNumber": 2, "duration": 0.12},
                   {"frameNumber": 3, "duration": 0.12}, {"frameNumber": 4, "duration": 0.12}],
        "tags": [{"name": "idle", "fromFrame": 1, "toFrame": 4, "repeats": "0", "aniDir": "forward"}],
    })
    assert fms == [120, 120, 120, 120], fms
    assert tgs[0]["from"] == 0 and tgs[0]["to"] == 3 and tgs[0]["repeat"] == 0, tgs
    live_counts = lint(fms, tgs, budgets)[1]
    assert live_counts.get("too_fast", 0) == 4, live_counts  # 120ms idle < 150ms band
    # A well-timed live idle (0.2s) passes.
    fms2, tgs2 = normalize({
        "frames": [{"frameNumber": i + 1, "duration": 0.2} for i in range(4)],
        "tags": [{"name": "idle", "fromFrame": 1, "toFrame": 4, "repeats": "0"}],
    })
    assert _errors(lint(fms2, tgs2, budgets)[0]) == [], lint(fms2, tgs2, budgets)[0]

    # ASEPRITE JSON EXPORT shape: frames dict (ms) + meta.frameTags (0-based, no repeat).
    exp_fms, exp_tags = normalize({
        "frames": {f"sprite {i}.aseprite": {"duration": 120} for i in range(4)},
        "meta": {"frameTags": [{"name": "idle", "from": 0, "to": 3, "direction": "forward"}]},
    })
    assert exp_fms == [120, 120, 120, 120], exp_fms
    assert exp_tags[0]["from"] == 0 and exp_tags[0]["to"] == 3, exp_tags
    assert lint(exp_fms, exp_tags, budgets)[1].get("too_fast", 0) == 4
    print(json.dumps({"selftest": "ok"}))


def main(argv=None):
    ap = argparse.ArgumentParser(description="Lint animation frame timing against per-state budgets.")
    ap.add_argument("clip", nargs="?", help="clip JSON (raw live_list_frames/_tags output, or simple shape)")
    ap.add_argument("--budgets", default=DEFAULT_BUDGETS)
    ap.add_argument("--warn-only", action="store_true")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)

    if args.selftest:
        _selftest()
        return 0
    if not args.clip:
        ap.error("give a clip JSON or --selftest")

    with open(args.clip, "r", encoding="utf-8") as f:
        clip = json.load(f)
    findings, counts = lint_clip(clip, load_budgets(args.budgets))

    report = {"clip": args.clip, "ok": len(_errors(findings)) == 0,
              "counts": counts, "findings": findings[:200]}
    print(json.dumps(report, indent=2))
    return 1 if (_errors(findings) and not args.warn_only) else 0


if __name__ == "__main__":
    sys.exit(main())
