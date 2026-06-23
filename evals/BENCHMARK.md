# Capability & persona benchmark (SPEC-007 Phase 2)

Live, on-demand measurements (need Aseprite + tokens + a judge, so **not** in CI — the
*tooling* is CI-verified, the *runs* are recorded here). Every row must be re-derivable
from archived evidence under `evals/runs/<YYYY-MM-DD>/` (the executor prompt, the live
result, the judge's raw JSON). See [README](README.md#phase-2--live-on-demand-measurements).

## A. Cross-path capability benchmark
"How much does each capability path actually move output on *our* stack?" Run a
SwordsBench case (e.g. `tb_swords_walk`) twice per path — once **with** the path's step,
once **without** — and score both blind. Toggled as manual run-variants (no code flags):

| Path | Step toggled | Case | with | without | Δ (with − without) | Date / evidence |
|------|--------------|------|-----:|--------:|-------------------:|-----------------|
| 1 Perception | the `live_save_preview` see-step in the draw→see→fix loop | 16×16 sword (scoped) | outline present, reads at 100% | **no outline**, weak silhouette | **silhouette outline 0→39/39 boundary cells** (spatial defects 0→0) | 2026-06-22 · [runs/2026-06-22/](runs/2026-06-22/) |
| 2 Constrained colour | `live_palette_snap` (CIELAB) | 12px mixed-palette bench (scoped) | **100% on-pal** | 25% on-pal | **+75pp** (9 violations → 0) | 2026-06-21 · [runs/2026-06-21/](runs/2026-06-21/) |

A positive Δ = the path measurably improves the result on this server.

> **Run 2 (2026-06-22, Path 1).** A 16×16 side-view sword drawn live (Aseprite 1.3.17.2 / plugin
> 0.3.2). **Without** the see-step (blind, from a fixed coordinate plan): the result was objectively
> clean — 1 connected component, 0 orphans, 0 off-palette (careful coordinate planning is itself a
> form of verification) — BUT the silhouette had **no outline** (the cool-dark outline colour was
> never placed), so it reads weakly at 100%. **With** the see-step: `live_save_preview` (16× upscale
> + gutter + inline + components Set-of-Mark) made the missing outline obvious; adding a 1px
> hue-shifted dark outline (rules/02) brought silhouette boundary coverage from 0/39 → 39/39 and the
> sprite now reads clearly. So perception's *measured* contribution on this scoped case was a
> **readability fix**, not error-correction. Evidence: [`runs/2026-06-22/`](runs/2026-06-22/)
> (`path1_{without,with,with_clean}.png` + `path1_perception.json`). **Caveats (honest):** N=1,
> single agent drew + assessed (no independent blind judge), and the objective spatial-defect metric
> was uninformative because the blind draw was clean — perception's headline error-CATCHING value
> (orphans / asymmetry / off-by-one) is better shown on a complex/freehand sprite or with an
> independent judge, which stays the open Path-1 measurement.

> **Run 1 (2026-06-21, Path 2).** On a locked 3-colour palette, 12 pixels were drawn (3
> on-palette + 9 off-palette). Metric = off-palette violations (objective, no LLM judge).
> **Without** `live_palette_snap`: 9/12 off-palette (25% on-palette). **With**: 0/12
> (100%). Δ = **9 violations eliminated**. The snap used real CIELAB ΔE (e.g. `#808080`→red,
> `#ff8800`→red), not RGB. Evidence: [`runs/2026-06-21/`](runs/2026-06-21/)
> (`path2_{before,after}.png` + `path2_constrained_colour.json`). Scoped single example —
> the full SwordsBench-with-judge cross-path runs remain on-demand.

## B. Persona A/B
Tests the candidate "artistic agent" persona line (`judge.PERSONA_CANDIDATE`). Emit with
`python evals/judge.py --emit-ab <case>`; run Variant A (with persona) + Variant B
(without), judge **blind**. **Adopt the line only if mean Δ (A − B) ≥ +0.05 with a
consistent sign over ≥3 runs** — otherwise it stays out (the source's claim is an
*untested hypothesis*, §D).

| Run | Date | Case | A_score (persona) | B_score (baseline) | Δ (A − B) | Evidence |
|----:|------|------|------------------:|-------------------:|----------:|----------|
| 1 | — | — | — | — | — | _pending_ |
| 2 | — | — | — | — | — | _pending_ |
| 3 | — | — | — | — | — | _pending_ |

**Mean Δ:** _pending_ · **Decision:** _pending (adopt iff mean Δ ≥ +0.05, consistent sign)_

## C. Long-session degradation (donut test)
Snapshot an objective quality vector (linter pass-rate, min silhouette-IoU, off-palette
count) at context-fill checkpoints during a long task, then
`python evals/judge.py --slope snapshots.json`. **Regression** = any checkpoint below its
0%-baseline margin (linter −0.10, min IoU < 0.80, or off-palette > 0 from a 0 baseline).

| Date | Task | Checkpoints (linter / minIoU / offpal) | Slope | Regressed | Evidence |
|------|------|----------------------------------------|------:|:---------:|----------|
| — | — | — | — | — | _pending first run_ |
