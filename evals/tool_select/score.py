#!/usr/bin/env python3
"""Tool-surface measurement scorer (stdlib-only).

Answers three questions with numbers, comparing a FLAT 77-tool surface against a
hard-GATED (core + workflow-profile) surface:

  1. selection accuracy / recall  -- does gating make the right tool harder to find?
  2. token cost per client type   -- does the surface->tool discovery loop actually save tokens?
  3. where (if anywhere) gating wins.

Selection data comes from selector agents (selections.json); the token cost is an
analytic model so the comparison is reproducible. Run with --selftest for the math check.

    python score.py --selftest
    python score.py --selections runs/<date>/selections.json [--out results.json]
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SEARCH_OVERHEAD = 350   # a deferred-client ToolSearch round-trip (call + result)
GATE_OVERHEAD = 350     # an enable_toolset round-trip (call + result + tools/list_changed)


def load(name):
    return json.load(open(os.path.join(HERE, name), encoding="utf-8"))


# ---- token model -------------------------------------------------------------
def task_tokens(condition, correct_tool, opened_profile, surface):
    """Effective tokens to REACH `correct_tool` under a surface condition.

    flat_eager     : every tool's full schema is loaded standing (eager clients: Cursor/Desktop/API).
    flat_deferred  : only names stand; one ToolSearch loads the picked tool's schema (Claude Code today).
    gated_dynamic  : core schemas + profile names stand; a non-core tool needs a gate-open then a search.
    """
    W = surface["weights"]
    core = set(surface["core"])
    profiles = surface["profiles"]
    nl = surface["name_listing_per_tool"]
    w = W.get(correct_tool, 200)
    names_all = nl * len(W)
    core_standing = sum(W[t] for t in core) + nl * len(profiles)  # core schemas + the few profile names

    if condition == "flat_eager":
        return sum(W.values())                      # standing, task-independent
    if condition == "flat_deferred":
        return names_all + SEARCH_OVERHEAD + w
    if condition == "gated_dynamic":
        if correct_tool in core:
            return core_standing
        prof_tools = profiles.get(opened_profile, [])
        return core_standing + GATE_OVERHEAD + nl * len(prof_tools) + SEARCH_OVERHEAD + w
    raise ValueError(condition)


def standing_cost(condition, surface):
    """Task-independent tokens the client pays EVERY turn just to carry the surface."""
    W = surface["weights"]; nl = surface["name_listing_per_tool"]
    if condition == "flat_eager":
        return sum(W.values())
    if condition == "flat_deferred":
        return nl * len(W)
    if condition == "gated_dynamic":
        return sum(W[t] for t in surface["core"]) + nl * len(surface["profiles"])
    raise ValueError(condition)


# ---- scoring -----------------------------------------------------------------
def score(surface, cases, selections):
    by_id = {c["id"]: c for c in cases["cases"]}
    t2p = surface["tool_to_profile"]
    out = {"n_cases": len(by_id), "flat": {}, "gated": {}}

    # FLAT: the agent saw all 77; success = first pick is a correct tool.
    flat = selections["flat"]
    flat_hits, flat_tok_e, flat_tok_d = 0, [], []
    flat_misses = []
    for cid, c in by_id.items():
        pick = flat[cid]["chosen_tool"]
        ok = pick in c["correct_tools"]
        flat_hits += ok
        if not ok:
            flat_misses.append({"id": cid, "picked": pick, "want": c["correct_tools"]})
        flat_tok_e.append(task_tokens("flat_eager", c["correct_tools"][0], None, surface))
        flat_tok_d.append(task_tokens("flat_deferred", pick if ok else c["correct_tools"][0], None, surface))
    out["flat"] = {
        "selection_accuracy": round(flat_hits / len(by_id), 3),
        "misses": flat_misses,
        "avg_tokens_eager_client": round(sum(flat_tok_e) / len(by_id)),
        "avg_tokens_deferred_client": round(sum(flat_tok_d) / len(by_id)),
    }

    # GATED may be pending (e.g. a session-limit retry) — score flat + token model regardless.
    gated_sel = selections.get("gated") or {}
    if not gated_sel or not any(v.get("chosen_tool") or v.get("open_profile") for v in gated_sel.values()):
        out["gated"] = {"status": "pending (no selections yet)"}
        out["standing_cost_per_turn"] = {
            "flat_eager": standing_cost("flat_eager", surface),
            "flat_deferred": standing_cost("flat_deferred", surface),
            "gated_dynamic": standing_cost("gated_dynamic", surface),
        }
        return out

    # GATED: the agent saw core + profile names; it must ROUTE to the right group (open it).
    # Opening the right group IS the recall test -- after opening it would see the group's tools
    # and pick correctly. So `reached` = routed to the right place; tool-name guess is a bonus stat.
    gated = selections["gated"]
    g_hits, g_name_hits, g_tok, g_misses = 0, 0, [], []
    for cid, c in by_id.items():
        sel = gated[cid]
        opened = sel.get("open_profile")
        pick = sel.get("chosen_tool")
        want_profile = t2p.get(c["correct_tools"][0], "core")
        in_core = want_profile == "core"
        reached = in_core or opened == want_profile          # routed to where the tool lives
        g_hits += reached
        g_name_hits += pick in c["correct_tools"]
        if not reached:
            g_misses.append({"id": cid, "opened": opened, "want_profile": want_profile})
        g_tok.append(task_tokens("gated_dynamic", c["correct_tools"][0], opened or want_profile, surface))
    out["gated"] = {
        "routing_accuracy": round(g_hits / len(by_id), 3),
        "tool_name_precision": round(g_name_hits / len(by_id), 3),
        "misses": g_misses,
        "avg_tokens_deferred_client": round(sum(g_tok) / len(by_id)),
    }

    out["standing_cost_per_turn"] = {
        "flat_eager": standing_cost("flat_eager", surface),
        "flat_deferred": standing_cost("flat_deferred", surface),
        "gated_dynamic": standing_cost("gated_dynamic", surface),
    }
    # The headline deltas (what the decision hinges on)
    fe = out["flat"]["avg_tokens_eager_client"]
    ge_standing = out["standing_cost_per_turn"]["gated_dynamic"]
    out["verdict"] = {
        "eager_client_token_delta_gated_minus_flat": ge_standing - fe,
        "deferred_client_token_delta_gated_minus_flat":
            out["gated"]["avg_tokens_deferred_client"] - out["flat"]["avg_tokens_deferred_client"],
        "accuracy_delta_gated_routing_minus_flat":
            round(out["gated"]["routing_accuracy"] - out["flat"]["selection_accuracy"], 3),
    }
    return out


def selftest():
    surface = load("surface.json")
    cases = load("cases.json")
    by = {c["id"]: c for c in cases["cases"]}
    t2p = surface["tool_to_profile"]
    # Synthetic selections: a PERFECT flat selector; a gated selector that is perfect on core
    # but only opens the right profile 70% of the time on cross-profile tasks.
    flat, gated = {}, {}
    noncore = [cid for cid, c in by.items() if t2p[c["correct_tools"][0]] != "core"]
    miss_set = set(noncore[: max(1, int(round(len(noncore) * 0.3)))])  # 30% gated recall miss
    for cid, c in by.items():
        flat[cid] = {"chosen_tool": c["correct_tools"][0]}
        wp = t2p[c["correct_tools"][0]]
        if wp == "core":
            gated[cid] = {"open_profile": None, "chosen_tool": c["correct_tools"][0]}
        elif cid in miss_set:
            gated[cid] = {"open_profile": "io_escape", "chosen_tool": c["correct_tools"][0]}  # wrong profile
        else:
            gated[cid] = {"open_profile": wp, "chosen_tool": c["correct_tools"][0]}
    r = score(surface, cases, {"flat": flat, "gated": gated})
    assert r["flat"]["selection_accuracy"] == 1.0, r["flat"]
    assert r["gated"]["routing_accuracy"] < 1.0, "gated routing must drop on cross-profile misses"
    # Eager clients: gated standing must be far cheaper than flat-eager's full load.
    assert r["verdict"]["eager_client_token_delta_gated_minus_flat"] < 0, "gated should save eager clients"
    # Deferred clients (Claude Code): gated adds a gate loop -> should NOT be cheaper.
    assert r["verdict"]["deferred_client_token_delta_gated_minus_flat"] > 0, \
        "gated should cost a deferred client MORE (extra gate loop)"
    print("selftest OK:",
          json.dumps({"flat_acc": r["flat"]["selection_accuracy"],
                      "gated_routing_acc": r["gated"]["routing_accuracy"],
                      "verdict": r["verdict"],
                      "standing": r["standing_cost_per_turn"]}, indent=2))
    return 0


def main(argv):
    if "--selftest" in argv:
        return selftest()
    if "--selections" in argv:
        sel = json.load(open(argv[argv.index("--selections") + 1], encoding="utf-8"))
        r = score(load("surface.json"), load("cases.json"), sel)
        print(json.dumps(r, indent=2))
        if "--out" in argv:
            json.dump(r, open(argv[argv.index("--out") + 1], "w"), indent=2)
        return 0
    print(__doc__)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
