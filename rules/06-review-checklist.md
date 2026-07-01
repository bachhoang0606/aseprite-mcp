# 06 — Sprite review checklist

> The pass/fail rubric used for self-review before declaring a sprite done, and
> by `/pixel-review` and the `pixel-critic` agent. Score each line; a sprite is
> "done" when every **must** passes and the weighted feel is ≥ 9/10.

## A. Silhouette & readability (must)
- [ ] Recognizable as a flat silhouette; distinct from other characters.
- [ ] Extremities break the outline (head shape, weapon, hands/feet readable).
- [ ] No tangents (edges barely touching); depth reads clearly.

## B. Palette & color (must)
- [ ] All pixels are from the locked palette — **no stray off-ramp colors**.
- [ ] Each material is a 3–5 step ramp; steps distinct at 100%.
- [ ] Ramps are **hue-shifted** (cool shadows, warm highlights), not value-only.
- [ ] No pure black shadow / pure white highlight killing hue.
- [ ] Color count within budget for the size.

## C. Form & light (must)
- [ ] One consistent light direction across the whole sprite.
- [ ] Shadow on planes away from light, highlight sparse on planes toward it.
- [ ] **No pillow-shading** (no bright halo center / direction-less shading).
- [ ] Focal point (face/weapon) has the strongest contrast.

## D. Linework & cleanliness (must)
- [ ] Outline is 1 px, hue-shifted (not flat black) unless style intends it.
- [ ] **No banding** (shadow not running parallel-adjacent to the outline steps).
- [ ] No stray single pixels / jaggies; curves have consistent run lengths.
- [ ] AA (if any) is hand-placed with ramp colors, not over-applied; no fringe.
- [ ] Dithering (if any) is regular and purposeful, not noisy or on faces.

## E. Proportion & view (must)
- [ ] Proportions match the size budget (stylized where small).
- [ ] If 3/4: asymmetric, planar-shaded (front/side/top), features off-center,
      feet staggered in depth — not a "front view nudged sideways".

## F. Rig & layers (must, if rigged)
- [ ] Each layer is a clean, complete shape when soloed (hide others to check).
- [ ] Anatomy on correct layers (chin→Head, shoulders→Body, weapon→hand layer).
- [ ] Semantic PascalCase + L/R naming; scratch/AI-draft layers removed.

## G. Animation (must, if animated)
- [ ] Big actions have anticipation; arcs are curved; eases read.
- [ ] Body bobs in walk (low at Down, high at Pass); arms counter-swing legs.
- [ ] Loose parts (cloth/ears/sack) overlap/lag by a frame.
- [ ] Volume preserved frame-to-frame; weapon stays a readable big shape.
- [ ] Tags correct (`idle`/`walk`/`attack`); loops cleanly at target speed.
> This is the quick pass. For a scored animation review (timing/volume/feel,
> gated by `tools/timing_lint.py` + `tools/silhouette_iou.py`), use `07-animation-review.md`.

## H. Output (must, if exporting)
- [ ] Exports cleanly (PNG/GIF/spritesheet) with correct metadata.
- [ ] Looks right at **100%** (not just zoomed in).

## Scoring & report shape
For a review, output:
1. **Verdict:** pass / needs-work, with the headline reason.
2. **Per-section findings:** only the failing/weak lines, each with *what* and
   *where* (region/layer/frame) and a concrete fix.
3. **Score** out of 10 (and which **must** items, if any, fail — any failing
   *must* caps the verdict at needs-work).
4. **Top 3 fixes** ordered by impact.

> Tie-back: this mirrors the project quality bar — every layer must still read
> as a beautiful, meaningful image when soloed on its own — and the per-layer
> soloing review the user asked for.
