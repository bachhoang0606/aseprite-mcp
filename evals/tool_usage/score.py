#!/usr/bin/env python3
"""Tool-USAGE correctness scorer (stdlib-only).

Given the tool (fixed) and its description (FULL vs TRIMMED), did the model emit the CORRECT
call? This is the measurement the tool-SELECTION harness does NOT do, and it's what de-risks
trimming a tool description: trimming only saves tokens if it doesn't degrade correct usage.

  - call cases   : score param-correctness (required params present + right values; deep match).
  - compute case : the model must DERIVE a value from the description (save_preview's gutter
                   inversion) -> tests whether trimming removed load-bearing how-to.

Reports per condition usage_accuracy + param_recall, and the token saving of the trim.
Run with --selftest for the math; --selections <file> to score real agent output.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TOK_PER_CHAR = 0.25  # rough tokens-per-char for description text


def load(name):
    return json.load(open(os.path.join(HERE, name), encoding="utf-8"))


def _match(gold, got):
    """Deep match: dict -> all keys match; float -> tolerance; else equality (case-insensitive str)."""
    if isinstance(gold, dict):
        return isinstance(got, dict) and all(_match(v, got.get(k)) for k, v in gold.items())
    if isinstance(gold, float):
        return isinstance(got, (int, float)) and abs(float(got) - gold) <= 1e-6
    if isinstance(gold, bool):
        return got is gold or got == gold
    if isinstance(gold, str):
        return isinstance(got, str) and got.strip().lower() == gold.strip().lower()
    return got == gold


def score_case(case, params):
    """Return (fully_correct: bool, partial_credit: 0..1)."""
    params = params or {}
    if case["type"] == "compute":
        tol = case.get("tol", 0)
        oks = [abs((params.get(k, 1e9)) - v) <= tol for k, v in case["expect"].items()]
        return all(oks), sum(oks) / len(oks)
    req = case["required"]
    hits = sum(1 for k, v in req.items() if _match(v, params.get(k)))
    forbidden = any(f in params for f in case.get("forbid", []))
    return (hits == len(req) and not forbidden), hits / len(req)


def score(descriptions, cases, selections):
    by = {c["id"]: c for c in cases["cases"]}
    out = {"n_cases": len(by)}
    for cond in ("full", "trimmed"):
        sel = selections[cond]
        full_ok, partial, misses = 0, [], []
        for cid, c in by.items():
            ok, pc = score_case(c, sel.get(cid, {}).get("params"))
            full_ok += ok
            partial.append(pc)
            if not ok:
                misses.append({"id": cid, "tool": c["tool"], "got": sel.get(cid, {}).get("params")})
        out[cond] = {
            "usage_accuracy": round(full_ok / len(by), 3),
            "param_recall": round(sum(partial) / len(by), 3),
            "misses": misses,
        }
    # Token cost of the descriptions under test (when their schemas are loaded).
    tf = round(sum(d["full_chars"] for d in descriptions.values()) * TOK_PER_CHAR)
    tt = round(sum(d["trimmed_chars"] for d in descriptions.values()) * TOK_PER_CHAR)
    out["description_tokens"] = {"full": tf, "trimmed": tt, "saving": tf - tt}
    out["verdict"] = {
        "usage_accuracy_delta_trimmed_minus_full":
            round(out["trimmed"]["usage_accuracy"] - out["full"]["usage_accuracy"], 3),
        "token_saving_from_trim": tf - tt,
        "trim_is_safe": out["trimmed"]["usage_accuracy"] >= out["full"]["usage_accuracy"],
    }
    return out


def selftest():
    desc, cases = load("descriptions.json"), load("cases.json")
    by = {c["id"]: c for c in cases["cases"]}
    # FULL: the model emits every correct call (incl. the inversion compute). TRIMMED: same, EXCEPT
    # the compute case is wrong (the inversion formula was trimmed out) -> a usage regression.
    full, trimmed = {}, {}
    for cid, c in by.items():
        if c["type"] == "compute":
            full[cid] = {"params": dict(c["expect"])}
            trimmed[cid] = {"params": {"source_x": 296, "source_y": 168}}  # forgot to subtract gutter/scale
        else:
            full[cid] = {"params": dict(c["required"])}
            trimmed[cid] = {"params": dict(c["required"])}
    r = score(desc, cases, {"full": full, "trimmed": trimmed})
    assert r["full"]["usage_accuracy"] == 1.0, r["full"]
    assert r["trimmed"]["usage_accuracy"] < 1.0, "trimmed must regress on the compute case"
    assert r["verdict"]["token_saving_from_trim"] > 0, "trim must save tokens"
    assert r["verdict"]["trim_is_safe"] is False, "this synthetic trim is NOT safe (compute regressed)"
    print("selftest OK:", json.dumps({"full": r["full"]["usage_accuracy"],
                                       "trimmed": r["trimmed"]["usage_accuracy"],
                                       "verdict": r["verdict"]}, indent=2))
    return 0


def main(argv):
    if "--selftest" in argv:
        return selftest()
    if "--selections" in argv:
        sel = json.load(open(argv[argv.index("--selections") + 1], encoding="utf-8"))
        r = score(load("descriptions.json"), load("cases.json"), sel)
        print(json.dumps(r, indent=2))
        if "--out" in argv:
            json.dump(r, open(argv[argv.index("--out") + 1], "w"), indent=2)
        return 0
    print(__doc__)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
