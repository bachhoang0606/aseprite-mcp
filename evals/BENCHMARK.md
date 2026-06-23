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
| 1 Perception | the `live_save_preview` see-step in the draw→see→fix loop | 24×24 goblin (blind-judged) | overall **7.0** | overall **5.67** | **+1.33 overall · 3/3 blind judges · form/light +3.33** | 2026-06-22 · [runs/2026-06-22/](runs/2026-06-22/) |
| 2 Constrained colour | `live_palette_snap` (CIELAB) | 12px mixed-palette bench (scoped) | **100% on-pal** | 25% on-pal | **+75pp** (9 violations → 0) | 2026-06-21 · [runs/2026-06-21/](runs/2026-06-21/) |
| 4 Reference grounding | import a reference (`live_import_reference`) vs invent from scratch | 24×24 apple (blind-judged) | overall **7.33** | overall **3.33** | **+4.0 overall · 3/3 blind judges · form/light +5.0, proportions +3.67** | 2026-06-22 · [runs/2026-06-22/](runs/2026-06-22/) |

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

> **Run 3 (2026-06-22, Path 1 — the stronger run that closes Run 2's caveats).** A **24×24
> front-facing goblin head** drawn live. **Without** the see-step (blind first pass): the right eye
> landed a row lower than the left (a vertical asymmetry you make without visual feedback) and the
> skin was flat. **With** the see-step: `live_save_preview` made both obvious; I aligned the eyes
> and added a hue-shifted directional ramp (light forehead / shadow jaw+right). The two clean
> renders were then scored **blind by an independent 3-judge panel** (scrambled labels, hypothesis
> withheld). Result: **3/3 judges preferred the perception version**, mean overall **5.67 → 7.0
> (+1.33)**, driven by **form/light +3.33** (flat → ramped) — and, unprompted, every judge named
> exactly what the see-step fixed (the value/hue ramp + light direction, and "mismatched eyes" in
> the blind version). Silhouette + linework tied (same base + outline, no orphans). So on a complex
> sprite, perception's value is real, externally-scored, and **error-catching + polish**, not just
> the readability fix Run 2 showed. Evidence: [`runs/2026-06-22/`](runs/2026-06-22/)
> (`cand_A.png` / `cand_B.png` + `path1b_perception.json`). Caveats: still N=1 case; the panel is 3
> LLM judges (consistent but model-correlated).

> **Run 4 (2026-06-22, Path 4 — reference grounding; also live-verifies `live_import_reference`).**
> The research names "inventing organic shapes from text" as the agent's #1 hardest weakness, so the
> see-step won't save a from-scratch organic draw — a reference will. Subject: a 24×24 apple.
> **Without** (freehand from imagination): the apple came out a near-perfect circle with **flat
> horizontal value banding** (hard light/mid/dark stripes) and an angular skirt. **With**: I generated
> an organic 48×48 apple reference and `live_import_reference`'d it to 24×24 snapped to a 6-colour
> palette (decode → content-aware dominant downscale → CIELAB snap → 231 px drawn, all on-palette —
> the tool's first live E2E verification). The two clean renders were scored **blind by an independent
> 3-judge panel**. Result: **3/3 preferred the reference version**, mean overall **3.33 → 7.33
> (+4.0)** — the largest cross-path Δ so far — driven by **form/light +5.0** (banding → directional
> round shading) and **proportions +3.67**. Unprompted, the judges named the freehand's "textbook
> banding" and "near-perfect circle" vs the reference's "organic lobed silhouette… value steps follow
> the curvature." Evidence: [`runs/2026-06-22/`](runs/2026-06-22/) (`apple_ref.png`, `cand_ref.png` /
> `cand_noref.png` + `path4_reference.json`). Caveats: N=1, model-correlated judges; the registered
> live binary predates regrid/auto_colors, so those Phase-2 import features stay unverified live.

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
| Run | Method | Case | A (persona) | B (baseline) | Δ (A − B) | Pref | Evidence |
|----:|--------|------|------------:|-------------:|----------:|------|----------|
| 1 | **confounded** (1 operator drew both) | 32×32 swordsman | overall **7.67** | overall **3.33** | **+4.33 (+0.43)** | 3/3 persona | [runs/2026-06-23/](runs/2026-06-23/) (`persona_ab1.json`) |
| 2 | **de-confounded** (independent agents) | 32×32 swordsman | overall **4.0** | overall **7.33** | **−3.33 (−0.33)** | **3/3 baseline** | [runs/2026-06-23/](runs/2026-06-23/) (`persona_ab_deconfound.json`) |
| 3 | **de-confounded** (independent agents) | 32×32 archer | overall **6.67** | overall **5.67** | **+1.0 (+0.10)** | 3/3 persona | [runs/2026-06-23/](runs/2026-06-23/) (`persona_ab_deconfound.json`) |

**Δ across 3 runs (0–1 scale):** +0.43, **−0.33**, +0.10 → **sign NOT consistent**; mean of the two *de-confounded* runs = **−0.12**.
**Decision: ✗ REJECT — the candidate persona line is NOT adopted.** The rule adopts only on a consistent-sign mean Δ ≥ +0.05 over ≥3 runs; the sign flips, and once de-confounded the effect is *negative on average*. Run 1's large +0.43 is explained as single-operator confound, not a persona effect.

> **Run 1 (2026-06-23, Persona A/B).** Case `tb_swords_static` — a 32×32 swordsman, 3/4 view,
> on a locked 12-colour palette, **both variants drawn live**. **Variant B (baseline, no persona):**
> a sincere default — flat single-value fills, no outline, a thin vertical sword, blocky symmetric
> stance. **Variant A (with the candidate persona line):** the same brief on the same palette with
> the persona doctrine applied — a planned readable 3/4 silhouette, a 1px dark outline, hue-shifted
> volume shading (upper-left light), a big diagonal blade with crossguard/grip/pommel, and separated
> shaded legs. Both verified **100% on-palette** (so the persona's palette-discipline claim was
> already met by both; the delta is silhouette/form/proportion). The two clean renders were scored
> **blind by an independent 3-judge panel** (neutral labels, persona placed *second* to counter
> first-position bias, hypothesis withheld). Result: **3/3 preferred the persona variant**, mean
> overall **3.33 → 7.67 (+4.33)**, driven by **form/light +4.67** and **silhouette +3.67** — and,
> unprompted, every judge named the doctrine's exact levers ("clean intentional dark outline",
> "value ramp", "a large diagonal blade with a crossguard/pommel that an arm visibly grips") vs the
> baseline's "flat single-fill blocks … fused legs … floating tab-arms". Evidence:
> [`runs/2026-06-23/`](runs/2026-06-23/) (`persona_A.png` / `persona_B.png` + `persona_ab1.json`).
> **Honest caveats — read before trusting the magnitude:** a *single operator drew both* and cannot
> un-know the persona, deliberately applying the doctrine to A while drawing B as a plain default, so
> this largely re-demonstrates the **Path-1 finding** (outline + value ramp beat flat) under a persona
> label rather than isolating whether the persona *string* causes better output; A was also drawn
> second (practice/order effect). The magnitude is therefore an **upper bound**. The proper de-confounded
> design — two *independent* executor agents, one given the persona line and one not, each drawing
> blind, then judged blind — stays pending, as do runs 2–3 (the decision needs **≥3** runs).

> **Runs 2–3 (2026-06-23, Persona A/B — the DE-CONFOUNDED runs that settle it).** To remove Run 1's
> single-operator confound, two *independent* cold executor agents each **designed** a sprite in
> isolation — one given the persona line, one not, neither aware of the A/B test or the other — and
> emitted a precise draw-op plan that a **deterministic rasterizer applied 1:1** (zero operator input).
> So the only difference between variants is the persona *string*. **Run 2 (swordsman): 3/3 judges
> preferred the BASELINE** (persona overall 4.0 vs 7.33, **Δ −3.33**) — the persona agent over-invested
> in body volume and left the sword a **detached bar floating beside the figure** ("a fatal silhouette
> failure for 'a humanoid holding a sword'"), while the plain agent had the figure actually grip a
> raised blade. **Run 3 (archer): 3/3 preferred the PERSONA** (6.67 vs 5.67, **Δ +1.0**) — here the
> persona agent drew a curved bow with a visible gripped string vs the plain agent's ambiguous
> floating column. All four designs were **100% on-palette** (palette wasn't the differentiator). Net:
> the sign is **inconsistent** and the de-confounded mean is **negative**, so the persona line is
> **rejected**. The decisive dimension in both runs — *does the figure actually hold the weapon* — is
> a failure mode the generic "be meticulous / readable silhouette" persona doesn't target; task-specific
> rules + the live see-step do. Evidence: [`runs/2026-06-23/`](runs/2026-06-23/) (`r2_*_x16.png`,
> `r3_*_x16.png`, `r{2,3}_{plain,persona}.json`, `persona_ab_deconfound.json`). Caveats: executors
> planned **blind** (no live preview — both equally), N=2 de-confounded cases, 3 model-correlated judges.

## C. Long-session degradation (donut test)
Snapshot an objective quality vector (linter pass-rate, min silhouette-IoU, off-palette
count) at context-fill checkpoints during a long task, then
`python evals/judge.py --slope snapshots.json`. **Regression** = any checkpoint below its
0%-baseline margin (linter −0.10, min IoU < 0.80, or off-palette > 0 from a 0 baseline).

| Date | Task | Checkpoints (linter / minIoU / offpal) | Slope | Regressed | Evidence |
|------|------|----------------------------------------|------:|:---------:|----------|
| 2026-06-23 | C1 — 8-frame goblin walk (realistic, ~1px body bob) | 1.0 / **0.73→0.66** / 0 (flat) | −0.008 | ⚠ flag¹ | [runs/2026-06-23/](runs/2026-06-23/) (`donut_c.json`) |
| 2026-06-23 | C2 — 8-frame goblin walk (body fixed, feet-only) | 1.0 / **0.97→0.90** / 0 | −0.009 | ✅ no | [runs/2026-06-23/](runs/2026-06-23/) (`donut_c.json`) |

¹ Not session degradation — a **metric false-positive** (see note). **No-decay confirmed by C2.**

> **First §C run (2026-06-23) — no degradation; one metric insight.** The "long session" is an
> executor agent generating an **8-frame 16×16 goblin walk-cycle** as op-plans in one long response
> (~230–260 ops), scored at cumulative context-fill checkpoints (25/50/75/100%) with the project's own
> `lint_sprite` + `silhouette_iou`. **In both runs the linter pass-rate held at 1.0 and off-palette at
> 0 across all 8 frames** — zero context-rot in cleanliness or palette over the long generation — and
> the composite slope was **flat** (−0.008 / −0.009). **C1** (the agent gave the goblin a ~1px body
> bob) tripped the `regressed` flag, but *only* on silhouette-IoU, and *from frame 1* (baseline IoU
> 0.733 < the 0.80 floor): that's constant animation bounciness on a tiny 16px sprite, **not** decay.
> **C2** (body held fixed, feet-only motion) gives a baseline IoU of 0.971 that clears the floor, and
> the donut gate then reports **no regression** (slope −0.009) — cleanly confirming **no quality decay
> over the long generation.** **Metric finding:** the gate checks `min_iou` against an *absolute* floor
> while the linter uses a *baseline-relative* margin, so a low-but-stable animation mis-flags;
> recommended follow-up is to judge `min_iou` relative to its own baseline too (left out of this PR to
> avoid changing tested eval logic). Evidence: [`runs/2026-06-23/`](runs/2026-06-23/) (`donut_c.json`,
> `donut_strip_x16.png`, `donut2_strip_x16.png`, `donut{,2}_snapshots.json`). Caveats: single
> long-generation proxy (output growth, not multi-turn input-context-rot), N=1 task per condition,
> rasterized op-plans (not live hand-drawn).
