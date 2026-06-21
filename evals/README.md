# Evals — quality gates for skills/agents/hooks

Checklist 9.4. Two tiers:

## Tier A — automatable (CI, deterministic, no Aseprite/LLM)
`python evals/run.py` runs graded checks and reports per-component coverage:
- `palette_hueshift` — the goblin-default skin ramp is genuinely hue-shifted
  (hue spread across the ramp) and value-monotonic, not value-only.
- `guard_decisions` — the batch-draw guard blocks batch drawing **and
  destructive** (`clear_/remove_/delete_`) tools and allows `live_*` / export.
- `linter_good/offpalette/orphan` — the sprite linter (`tools/lint_sprite.py`)
  passes a clean sprite and flags off-palette / orphan pixels.
- `visual_golden_match` / `visual_detects_change` — the pixel-diff matches the
  golden and detects a changed sprite.
- `silhouette_iou_stable` / `silhouette_iou_detects_drift` — the silhouette-IoU
  animation-drift gate (`tools/silhouette_iou.py`, SPEC-007 Phase 1): a clean walk
  strip stays above the 0.80 IoU floor, and a deliberately-ballooned frame is
  **caught** below it — the cross-frame proportion drift SwordsBench names as the #1
  animation failure. Goldens under `evals/fixtures/` (regenerate: `make_fixtures.py`).
- `health_check_json` — the SessionStart hook emits valid context JSON.
- `tier_b_cases_wellformed` — every Tier-B case (below) is structurally valid:
  maps to a real component + checklist id, cites existing rule files, and its
  rubric weights sum to 1.0 (via `evals/judge.py:validate()`).

Mapping of each case → the skill/agent/hook it grades lives in `cases.json`.
Exit code is non-zero if any check fails (wired into `.github/workflows/quality.yml`).

## Tier B — LLM-judged, live (manual / on-demand)
Running a skill/agent prompt through Claude and grading the live Aseprite result
against `rules/` requires Aseprite + tokens and is non-deterministic, so the
*live run* is **not** in CI. The eval definitions, however, are concrete: each
case in [`tier_b.json`](tier_b.json) is a weighted rubric tied to a checklist
item, and CI structurally validates them (the `tier_b_cases_wellformed` check
above) so they can't silently rot.

| Case | Checklist | Component |
|------|-----------|-----------|
| `tb_pixel_new` | 5.1 | `/pixel-new` |
| `tb_pixel_shade` | 5.2 | `/pixel-shade` |
| `tb_pixel_animate` | 5.3 | `/pixel-animate` |
| `tb_pixel_export` | 5.4 | `/pixel-export` |
| `tb_animation_director` | 6.2 | `animation-director` |
| `tb_rig_builder` | 6.4 | `rig-builder` |
| `tb_swords_static` | 5.1 | `/pixel-new` |
| `tb_swords_walk` | 5.3 | `/pixel-animate` |

### Running one live
1. Connect Aseprite (`live_preflight` ready).
2. `python evals/judge.py --emit tb_pixel_animate` → prints the judge prompt
   (task + weighted rubric + scoring rule).
3. Run the component's prompt live; capture the result (sprite info / layer list
   / exported files / the agent's plan).
4. Paste the result under the prompt's `---` line and have a judge model (or
   `pixel-critic`) emit the JSON score.
5. Record the outcome in [`RESULTS.md`](RESULTS.md) **and archive the evidence**
   under `evals/runs/<YYYY-MM-DD>/` — the judge prompt(s), the judge's raw JSON
   scores, and the rendered PNGs/tool outputs that were scored. RESULTS.md rows
   must be independently re-derivable from the archive, not trust-me logs.

A case **passes** when `case_score >= pass_threshold` AND no `must_pass`
criterion scores below 0.5. List cases with `python evals/judge.py --list`.

## Adding a check
- **Tier A:** add a function to `run.py` returning `(ok, detail)`, register it in
  `CHECKS`, and add a `cases.json` entry mapping it to the component(s) it covers.
- **Tier B:** add a case object to `tier_b.json` (id, component, checklist,
  prompt, pass_threshold, rubric with weights summing to 1.0); `judge.py` and CI
  validate it automatically.
