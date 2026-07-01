---
name: pixel-review
description: Critique a pixel-art sprite against the project rulebook and return a scored, actionable report — silhouette, palette, light/form, linework, proportion/view, rig, animation. Use when the user wants quality feedback or a go/no-go before shipping.
argument-hint: "[layer or whole sprite] [view: side|front|3-4]"
---

# /pixel-review — scored critique vs the rules

Apply the rubric in `rules/06-review-checklist.md` (which encodes the cross-source
quality criteria in `knowledge/references/pixel-art-sources.md`). This is also the
self-review every other skill must pass before declaring done.

## Steps
1. **Preflight** (`live_preflight` ready) so you inspect the *live* sprite.
2. **Gather evidence**: `live_get_sprite_info`, `live_list_layers`, `live_list_tags`,
   `live_list_palette`, and pixel data / per-layer soloing as needed
   (`live_set_layer_visibility`). Judge at **100%**.
3. **Score each section** of `rules/06`:
   A silhouette/readability · B palette/color · C form/light · D linework ·
   E proportion/view (incl. true 3/4 asymmetry) · F rig/layers · G animation ·
   H output. Mark each line pass / weak / fail.
   - **If animated**, do section G as the scored rubric in `rules/07-animation-review.md`,
     grounded by the deterministic gates (don't eyeball what a tool can decide):
     `python tools/timing_lint.py clip.json` (per-state timing; export the clip from
     `live_list_tags` + `live_list_frames`), `python tools/silhouette_iou.py` (volume
     drift), and `python tools/lint_sprite.py` per frame. Cite the tool finding in the
     report.
4. **Diagnose with location**: name the *region/layer/frame* and the *specific*
   defect using the right term — pillow-shading, banding, jaggies, orphan pixels,
   value-only ramp, off-palette stray, tangent, 1-px limb, skating, weapon-stub.
5. **Report** in this shape:
   - **Verdict:** pass / needs-work + headline reason. Any failing **must** item
     caps the verdict at needs-work.
   - **Findings:** only weak/failing lines, each with what + where + concrete fix.
   - **Score /10.**
   - **Top 3 fixes** ordered by impact (and which skill to run for each, e.g.
     "`/pixel-shade` the Head — pillow-shaded").

## Definition of done
A reproducible scored report tied to `rules/06`, with at least the top 3 fixes
actionable by a specific `/pixel-*` skill.

## Eval prompts
- "Review this goblin" → per-section scores, named defects with fixes, /10, top-3.
- "Is this 3/4 view correct?" → checks asymmetry, planar shading, off-center
  features, staggered feet; flags "front view nudged sideways" if applicable.
- "Why does this look muddy?" → points to value-only ramps / banding / too many
  similar colors with the exact regions.
