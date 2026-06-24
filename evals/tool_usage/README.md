# Tool-USAGE correctness harness

The complement to `evals/tool_select/`. That one measures **which tool** the model picks; this one
measures **whether it USES the tool correctly** — emits the right call (params/values) — under a
**full** vs **trimmed** tool-level description.

## Why it exists
"Trim the oversized tool descriptions to save tokens" is only worth doing if the trim **doesn't
degrade correct usage**. The token saving is trivially measurable; the *quality* effect is not — and
some verbose descriptions carry **load-bearing** how-to (e.g. `live_save_preview`'s coordinate
gutter-inversion formula `source_x = (preview_x − left_w)/scale`). Trimming that out would quietly
break the perception loop. This harness measures that risk before any trim ships.

## Design
- **`descriptions.json`** — for each candidate tool, the real **`full`** tool-level description
  (extracted from `src/server.rs`) and an authored **`trimmed`** rewrite that keeps the params but
  drops the verbose prose (incl., deliberately, `save_preview`'s inversion formula). Built by
  `_build_descriptions.py`.
- **`cases.json`** — usage tasks with a gold call:
  - `type:call` — gold `required` params/values (deep-matched, partial credit).
  - `type:compute` — the model must DERIVE a value from the description (the `save_preview`
    inversion) → directly tests whether trimming removed load-bearing detail.
- **agents** (`_wf_usage.js`) emit the call for each case under full vs trimmed. They get the param
  signature (names+types, constant) + the tool-level description (the variable) — **no param-level
  docs**, so this is a *conservative / worst-case* estimate of trim risk (real clients also send
  param docs, which would cover some cases the tool-level prose does).
- **`score.py`** (stdlib, `--selftest`, CI-gated): per-condition `usage_accuracy` + `param_recall`,
  and the description token saving. `trim_is_safe` = trimmed usage ≥ full usage.

## Run
```
python evals/tool_usage/_extract_descriptions.py   # full descriptions from server.rs
python evals/tool_usage/_build_descriptions.py      # -> descriptions.json (full + trimmed)
python evals/tool_usage/score.py --selftest         # validate the scorer
python evals/tool_usage/_gen_workflow.py             # -> _wf_usage.js
# launch _wf_usage.js (Workflow) -> reshape output -> runs/<date>/selections.json
python evals/tool_usage/score.py --selections runs/<date>/selections.json --out runs/<date>/results.json
```

## Reading it
- If `usage_accuracy(trimmed) ≈ usage_accuracy(full)` → the trim is **safe**; take the token saving.
- If trimmed regresses (esp. the `compute` case) → that tool's verbosity is **load-bearing**; do NOT
  trim it (or move the detail somewhere still loaded when used). A safe trim candidate is a tool whose
  description is long but **not** load-bearing.
