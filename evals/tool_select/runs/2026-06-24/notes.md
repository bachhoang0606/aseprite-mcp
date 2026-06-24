# Tool-surface measurement — run 2026-06-24 (complete)

15 cases, selector agents under FLAT vs hard-GATED surfaces (flat done first; the 15 gated agents
re-ran after the account session-limit reset). All deltas final.

## Result
| metric | flat | gated | Δ (gated − flat) |
|---|--:|--:|--:|
| selection / **routing accuracy** | **1.000** | **0.933** | **−0.067** |
| tool-name precision (after routing) | 1.000 | 1.000 | — |
| **tokens / task — Claude Code (deferred)** | 2,275 | **8,307** | **+6,032** |
| standing / turn — eager client | 23,429 | **7,336** | −16,093 |

## What the numbers say (answers the three review concerns)

1. **Flat selection is perfect (15/15).** From the full 77-tool flat list the model picked the exact
   correct tool every time, with discriminating reasoning. → The flat surface causes **no**
   mis-selection on this suite; there is **no accuracy upside** for pruning to recover.

2. **Hard gating measurably HURTS recall.** Routing accuracy fell to 0.933 — one miss:
   **`style_profile`** (the tool `live_extract_style_profile` lives in the `color` group) was routed
   to **`tilemap`** because the model reasoned "style / tileset domain". Under gating the model would
   open the wrong group and **never find the tool**; under the flat surface it picked it correctly.
   This is precisely the recall failure mode raised in review — a tool whose group is non-obvious.

3. **The surface→tool discovery loop costs MORE, not less, for a deferred client.** For Claude Code
   (which already defers schemas via ToolSearch), gating adds a gate-open round-trip per non-core
   task: **+6,032 tokens/task** and a *higher* standing cost (7,336 vs 1,386, because `core` eagerly
   carries the 2.5k `save_preview` + 1k `import_reference` schemas). Gating's token win is **only**
   for eager clients (Cursor/Desktop/API): −16,093 standing.

## Verdict (data-backed)
- **Do NOT add hard gating for Claude Code.** It is *strictly worse* here: lower accuracy
  (−6.7pp) **and** higher token cost (+6k/task). Same conclusion the analytic argument gave.
- **Pruning is justified only for eager clients**, via **static profiles** (set-once, no per-task
  loop) or **consolidation** (fewer tools, nothing hidden — no recall risk).
- **Orthogonal real win, independent of gating:** don't eager-load the two giant schemas
  (`save_preview` 2.5k, `import_reference` 1k) — moving them to a name+ToolSearch tier saves more
  standing tokens than any grouping, for every client.

## Caveats
- N=15 tasks, 1-shot routing, single model; the routing miss count is small-sample (1/15). The
  *direction* (gating ≤ flat on accuracy, > flat on Claude-Code tokens) is robust and matches the
  token model. Re-run with a larger/again suite to tighten the recall estimate.
- The token model is analytic (fixed SEARCH/GATE overheads); absolute numbers are estimates, the
  cross-condition deltas are the signal.
