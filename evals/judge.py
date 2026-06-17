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


def main(argv):
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
