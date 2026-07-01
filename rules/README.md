# Pixel-art rulebook

Encoded pixel-art expertise the agent **must follow** when drawing, shading,
animating, or reviewing sprites in Aseprite. These rules are the source of the
quality bar — when a sprite looks "lem nhem" (muddy/noisy), it is almost always
a violation of one of the rules below.

> Scope: applies to all `live_*` drawing and to `/pixel-*` skills and the
> `pixel-critic` agent. Checklist pillar **4. Domain rules**.

## How the agent applies these
1. **Before drawing** — read `05-layers-and-rig.md` (set up layers) and
   `01-palette-and-color.md` (lock a palette first; never improvise colors).
2. **While drawing** — obey `00-core-principles.md` and `02-shading-outlining-aa.md`.
3. **For motion** — follow `04-animation.md`.
4. **Before declaring done** — self-review against `06-review-checklist.md`
   (the same rubric `pixel-critic` and `/pixel-review` use). For **animated**
   sprites also score `07-animation-review.md` (timing/volume/feel), whose
   deterministic axes are gated by `tools/timing_lint.py` + `tools/silhouette_iou.py`.

## Files
| File | Covers | Checklist |
|------|--------|-----------|
| [`00-core-principles.md`](00-core-principles.md) | Resolution, intentional pixels, readability | 4.1–4.3 |
| [`01-palette-and-color.md`](01-palette-and-color.md) | Palette discipline, ramps, hue-shifting | 4.1 |
| [`02-shading-outlining-aa.md`](02-shading-outlining-aa.md) | Selective outlining, anti-aliasing, banding, dithering | 4.2 |
| [`03-proportions-silhouette-3-4-view.md`](03-proportions-silhouette-3-4-view.md) | Proportions, silhouette, 3/4 view | 4.3 |
| [`04-animation.md`](04-animation.md) | Timing, easing, anticipation, walk/idle/attack | 4.4 |
| [`05-layers-and-rig.md`](05-layers-and-rig.md) | Layer/rig conventions, naming | 4.5 |
| [`06-review-checklist.md`](06-review-checklist.md) | The pass/fail self-review rubric (static sprite) | 4.x, 5.6, 6.1 |
| [`07-animation-review.md`](07-animation-review.md) | Scored animation review (timing/volume/feel), tool-gated | 4.4, 5.6 |

## Golden rule
**Every pixel is a decision.** If you cannot say *why* a pixel is that color in
that place (form, light, outline, or readability), it is noise — remove it.
"Lem nhem" = unintentional pixels. The fix is never "more pixels"; it is fewer,
more deliberate ones.
