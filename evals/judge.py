#!/usr/bin/env python3
"""Tier-B LLM-judge eval tooling (checklist 9.4, skills 5.1-5.4, agents 6.2/6.4).

Tier-B cases (`evals/tier_b.json`) grade a *live* skill/agent run against a
weighted rubric using an LLM judge. The live run + judging is non-deterministic
and needs Aseprite + tokens, so it is **not** run in CI. What CI *can* do
deterministically — and what `validate()` here does — is guarantee the eval
definitions are well-formed and traceable:

  * every case maps to a real component (a `skills/<name>/SKILL.md` or
    `agents/<name>.md`) and a checklist id,
  * every rubric criterion cites a rule/source file that exists,
  * criterion weights sum to 1.0 and the pass threshold is in (0, 1].

`evals/run.py` calls `validate()` as the `tier_b_cases_wellformed` Tier-A check,
so a malformed or dangling eval definition fails the build.

Usage:
    python evals/judge.py                 # validate all cases (exit 0/1)
    python evals/judge.py --list          # list cases + components
    python evals/judge.py --emit <id>     # print the judge prompt for one case
    python evals/judge.py --emit-ab <id>  # SPEC-007 Ph2: paired persona A/B prompt
    python evals/judge.py --slope <f.json># SPEC-007 Ph2: degradation slope/regression
"""
import json
import os
import sys

# Print UTF-8 even on a cp1252 Windows console (em-dashes in rubric text).
try:
    sys.stdout.reconfigure(encoding="utf-8")
except (AttributeError, ValueError):  # pragma: no cover - older/odd stdout
    pass

ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
TIER_B = os.path.join(ROOT, "evals", "tier_b.json")

WEIGHT_EPS = 1e-6

# SPEC-007 Phase 2 — the candidate "artistic agent" persona line under A/B test.
# Adopt into a skill/agent prompt ONLY if >=3 blind A/B runs show mean delta >= +0.05
# with consistent sign — per the research caveat that this is the source's *untested*
# hypothesis, not a result.
#
# RESULT (2026-06-23): TESTED, **REJECTED — kept OUT.** 3 blind A/B runs
# (evals/BENCHMARK.md §B, evals/runs/2026-06-23/): Δ(persona−baseline) on the 0–1 scale
# = +0.43 (confounded, 1 operator drew both), −0.33 (de-confounded swordsman),
# +0.10 (de-confounded archer). Sign is NOT consistent and the two de-confounded runs
# average −0.12, so the rule rejects it. The de-confounded design (independent executor
# agents, one prompted with the line below and one not) showed the generic persona does
# not reliably help — and on the swordsman it hurt (the persona agent orphaned the sword).
# Kept here only as the tested-and-rejected control; do NOT wire it into prompts.
PERSONA_CANDIDATE = (
    "You are a meticulous pixel artist who prizes a readable silhouette and strict "
    "palette discipline: plan key poses before drawing, and keep body volume constant "
    "across frames."
)
SIL_FLOOR = 0.80  # the silhouette-IoU drift floor (matches SPEC-007 Phase 1)


def load_cases():
    with open(TIER_B, encoding="utf-8") as f:
        return json.load(f)


def _component_path(component):
    """Resolve a case `component` to the repo file that defines it."""
    if component.startswith("/"):  # a skill, e.g. "/pixel-new"
        return os.path.join(ROOT, "skills", component.lstrip("/"), "SKILL.md")
    return os.path.join(ROOT, "agents", f"{component}.md")  # an agent


def validate():
    """Return (ok, detail). Structurally validate every Tier-B case."""
    try:
        data = load_cases()
    except (OSError, json.JSONDecodeError) as e:
        return False, f"cannot load tier_b.json: {e}"

    cases = data.get("cases", [])
    if not cases:
        return False, "no cases defined"

    errors = []
    ids = set()
    required = {"id", "component", "checklist", "prompt", "pass_threshold", "rubric"}
    for case in cases:
        cid = case.get("id", "<no-id>")
        missing = required - case.keys()
        if missing:
            errors.append(f"{cid}: missing fields {sorted(missing)}")
            continue
        if cid in ids:
            errors.append(f"{cid}: duplicate id")
        ids.add(cid)

        if not os.path.isfile(_component_path(case["component"])):
            errors.append(f"{cid}: component '{case['component']}' has no file")

        thr = case["pass_threshold"]
        if not (isinstance(thr, (int, float)) and 0.0 < thr <= 1.0):
            errors.append(f"{cid}: pass_threshold {thr!r} not in (0,1]")

        if not str(case["prompt"]).strip():
            errors.append(f"{cid}: empty prompt")

        rubric = case["rubric"]
        if not rubric:
            errors.append(f"{cid}: empty rubric")
            continue

        total_w = 0.0
        for crit in rubric:
            cmiss = {"id", "weight", "rule", "desc"} - crit.keys()
            if cmiss:
                errors.append(f"{cid}/{crit.get('id','?')}: missing {sorted(cmiss)}")
                continue
            total_w += crit["weight"]
            # rule may carry an anchor (file.md#section); check the file part.
            rule_file = crit["rule"].split("#", 1)[0]
            if not os.path.isfile(os.path.join(ROOT, rule_file)):
                errors.append(f"{cid}/{crit['id']}: rule '{rule_file}' not found")
        if abs(total_w - 1.0) > WEIGHT_EPS:
            errors.append(f"{cid}: rubric weights sum to {total_w:.3f}, need 1.0")

    if errors:
        return False, f"{len(errors)} issue(s): " + "; ".join(errors[:6]) + (
            " ..." if len(errors) > 6 else ""
        )
    covered = sorted({c["checklist"] for c in cases})
    return True, f"{len(cases)} cases well-formed; covers checklist {', '.join(covered)}"


def emit_prompt(case_id):
    data = load_cases()
    case = next((c for c in data["cases"] if c["id"] == case_id), None)
    if case is None:
        ids = ", ".join(c["id"] for c in data["cases"])
        print(f"no case '{case_id}'. available: {ids}", file=sys.stderr)
        return 1

    thr = case["pass_threshold"]
    lines = [
        f"# Tier-B judge prompt — {case['id']} (checklist {case['checklist']}, {case['component']})",
        "",
        "You are an expert pixel-art reviewer acting as an automated judge.",
        "A Claude Code skill/agent was given the TASK below and produced a result",
        "in a live Aseprite session. Score the RESULT against each rubric criterion",
        "from 0.0 to 1.0. Ground every judgement in the cited rule file.",
        "",
        f"## Task given to the {case['component']}",
        f"> {case['prompt']}",
        "",
        "## Rubric",
    ]
    for crit in case["rubric"]:
        flag = "  [MUST-PASS: <0.5 fails the case]" if crit.get("must_pass") else ""
        lines.append(
            f"- **{crit['id']}** (weight {crit['weight']}, rule `{crit['rule']}`){flag}: "
            f"{crit['desc']}"
        )
    lines += [
        "",
        "## Scoring",
        "- case_score = sum(criterion_score * weight). Weights sum to 1.0.",
        f"- PASS when case_score >= {thr} AND no MUST-PASS criterion scores < 0.5.",
        "",
        "## Output (JSON)",
        '```json',
        "{",
        '  "scores": { "<criterion_id>": 0.0, "...": 0.0 },',
        '  "case_score": 0.0,',
        '  "pass": true,',
        '  "notes": "one line per criterion citing the rule"',
        "}",
        '```',
        "",
        "Append the actual live result (sprite info / layer list / exported files /",
        "the agent's plan) below this line before judging:",
        "---",
    ]
    print("\n".join(lines))
    return 0


def emit_ab(case_id):
    """SPEC-007 Phase 2 — emit a paired (with/without persona) A/B for one case.

    The persona affects the *executor* (the skill/agent run), so run the same task
    TWICE — Variant A with the persona line prepended, Variant B without — then judge
    both results BLIND with the shared rubric and record the per-variant delta.
    """
    data = load_cases()
    case = next((c for c in data["cases"] if c["id"] == case_id), None)
    if case is None:
        ids = ", ".join(c["id"] for c in data["cases"])
        print(f"no case '{case_id}'. available: {ids}", file=sys.stderr)
        return 1
    lines = [
        f"# Persona A/B — {case['id']} ({case['component']}, checklist {case['checklist']})",
        "",
        "Run the SAME task TWICE with the executor, capture both live results, then judge",
        "them BLIND (do NOT tell the judge which variant carried the persona). Adopt the",
        "persona line only if mean Δscore (A − B) >= +0.05 with consistent sign over >=3 runs.",
        "",
        "## Variant A — executor task WITH the candidate persona line",
        f"> [persona] {PERSONA_CANDIDATE}",
        f"> {case['prompt']}",
        "",
        "## Variant B — executor task WITHOUT the persona line (baseline)",
        f"> {case['prompt']}",
        "",
        "## Judge each result independently (blind) with this rubric",
    ]
    for crit in case["rubric"]:
        flag = "  [MUST-PASS: <0.5 fails]" if crit.get("must_pass") else ""
        lines.append(
            f"- **{crit['id']}** (weight {crit['weight']}, rule `{crit['rule']}`){flag}: {crit['desc']}"
        )
    lines += [
        "",
        "case_score = Σ(criterion_score × weight). Record A_score, B_score, delta = A − B",
        "as one row per A/B run in evals/RESULTS.md, and archive evidence under",
        "evals/runs/<YYYY-MM-DD>/. Decide adopt/reject only after >=3 runs.",
    ]
    print("\n".join(lines))
    return 0


def compute_slope(snapshots, iou_floor=SIL_FLOOR, linter_margin=0.10):
    """SPEC-007 Phase 2 — long-session degradation (donut test).

    `snapshots`: list of {checkpoint, linter (0..1 pass-rate), min_iou, off_palette}
    at increasing context-fill checkpoints. The three metrics are checked SEPARATELY
    against the 0%-baseline; `regressed` is True if any checkpoint breaches a margin.
    A composite-quality least-squares `slope` is reported for trend (negative = decaying).
    """
    if len(snapshots) < 2:
        return {"slope": 0.0, "regressed": False, "detail": "need >=2 snapshots"}
    base = snapshots[0]
    reasons = []
    for s in snapshots[1:]:
        cp = s.get("checkpoint", "?")
        if s["linter"] < base["linter"] - linter_margin:
            reasons.append(f"cp{cp}: linter {s['linter']:.2f} < base {base['linter']:.2f}-{linter_margin:.2f}")
        if s["min_iou"] < iou_floor:
            reasons.append(f"cp{cp}: min_iou {s['min_iou']:.2f} < floor {iou_floor:.2f}")
        if s["off_palette"] > 0 and base["off_palette"] == 0:
            reasons.append(f"cp{cp}: off_palette {s['off_palette']} (baseline 0)")
    quality = [
        s["linter"] * 0.4 + s["min_iou"] * 0.4 + (0.2 if s["off_palette"] == 0 else 0.0)
        for s in snapshots
    ]
    n = len(quality)
    xs = list(range(n))
    mx, my = sum(xs) / n, sum(quality) / n
    denom = sum((x - mx) ** 2 for x in xs)
    slope = sum((xs[i] - mx) * (quality[i] - my) for i in range(n)) / denom if denom else 0.0
    return {
        "slope": round(slope, 4),
        "regressed": bool(reasons),
        "detail": "; ".join(reasons) if reasons else f"stable (composite slope {slope:+.4f})",
    }


def main(argv):
    if "--emit-ab" in argv:
        i = argv.index("--emit-ab")
        if i + 1 >= len(argv):
            print("--emit-ab needs a case id", file=sys.stderr)
            return 2
        return emit_ab(argv[i + 1])
    if "--slope" in argv:
        i = argv.index("--slope")
        if i + 1 >= len(argv):
            print("--slope needs a snapshots JSON path", file=sys.stderr)
            return 2
        with open(argv[i + 1], encoding="utf-8") as f:
            snaps = json.load(f)
        r = compute_slope(snaps if isinstance(snaps, list) else snaps["snapshots"])
        print(json.dumps(r, indent=2))
        return 1 if r["regressed"] else 0
    if "--list" in argv:
        data = load_cases()
        for c in data["cases"]:
            print(f"{c['id']:22} {c['checklist']:>4}  {c['component']}")
        return 0
    if "--emit" in argv:
        i = argv.index("--emit")
        if i + 1 >= len(argv):
            print("--emit needs a case id", file=sys.stderr)
            return 2
        return emit_prompt(argv[i + 1])

    ok, detail = validate()
    print(("OK: " if ok else "FAIL: ") + detail)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
