# Tool-surface measurement harness

**Question it answers (before we build any pruning):** does grouping the ~77 flat tools into a
`core` + workflow-`profiles` surface actually help — *without* hurting the model's ability to find
the right tool, and *without* the surface→tool discovery loop costing more tokens than it saves?

This exists because tool-surface pruning has two real risks (raised in review):
1. **Recall** — hiding a tool behind a profile may make it *harder* to find (the model can't pick
   what it can't see).
2. **Token economics** — a 2-step "open the group, then call the tool" loop may cost *more* than the
   flat surface it replaces, especially for clients that already defer tool schemas (Claude Code's
   ToolSearch).

## Design (mirrors the §A with/without methodology)
- **`surface.json`** — the pruned model: a `core` set (always on, 18 tools), 7 workflow `profiles`,
  and a per-tool token weight (≈ schema size). Built from the real 77 tools by `_build_surface.py`.
- **`cases.json`** — a gold set of realistic tasks, each labelled with the correct tool(s) and the
  profile it lives in. 11 of 15 deliberately land in non-`core` profiles to stress recall.
- **Selector agents** (`_wf_select.js`, via the Workflow tool) pick a tool under two surfaces:
  - **flat** — the agent sees all 77 names and picks one.
  - **gated** — the agent sees only the 18 core tools + 7 group hints, and must *route* (open the
    right group) for a non-core task, naming the tool it expects.
- **`score.py`** — deterministic scorer (stdlib, `--selftest`):
  - **flat selection accuracy** (first pick correct) and **gated routing accuracy** (opened the
    right group = the recall test; after opening, the model would see and pick the tool).
  - an analytic **token model** per client type, pricing the discovery loops:
    - `flat_eager` — every schema loaded standing (Cursor / Desktop / plain API).
    - `flat_deferred` — names stand; one ToolSearch loads the picked tool (Claude Code today).
    - `gated_dynamic` — core schemas + group names stand; a non-core tool adds a gate-open + search.

## Run
```
python evals/tool_select/_build_surface.py     # (re)build surface.json from the 77 tools
python evals/tool_select/score.py --selftest   # validate the token-model math
python evals/tool_select/_gen_workflow.py       # regenerate the selector workflow
# launch _wf_select.js via the Workflow tool -> reshape its output into runs/<date>/selections.json
python evals/tool_select/score.py --selections runs/<date>/selections.json --out runs/<date>/results.json
```

## Reading the result
A pruning design is only worth shipping if, on this suite, **gated routing accuracy ≈ flat accuracy**
(recall not hurt) AND the token model shows a **net win for the target client**. The token model
already shows the split analytically: gating saves *eager* clients a lot of standing tokens, but for
a *deferred* client (Claude Code) it adds a gate loop on top of a mechanism that already defers — so
it tends to cost **more**, not less. The agent run fills in whether routing accuracy holds up.
