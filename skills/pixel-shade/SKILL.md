---
name: pixel-shade
description: Shade a layer with hue-shifted ramps and a single light direction, live in Aseprite — add core shadow, highlight, selective outline and clean anti-aliasing while avoiding pillow-shading, banding and jaggies. Use when the user wants to render volume/light on flat art.
argument-hint: "[layer] [light dir e.g. top-left] [material/ramp]"
---

# /pixel-shade — render form with light

Turn flat colors into readable volume per `rules/01` (color) and `rules/02`
(shading, outlining, AA, banding). Grounded in
`knowledge/references/pixel-art-sources.md` (Aseprite shading ink, Lospec
hue-shift, Derek Yu / Pixel Parmesan on banding & AA).

## Steps
1. **Preflight.** `live_preflight` → require ready.
2. **Confirm palette/ramp.** Identify the material's ramp (`/pixel-palette`); never
   pick ad-hoc colors. If missing, build it first.
3. **Solo the layer** (`live_set_layer_visibility` others off) so you shade one
   clean shape (`rules/05` §4). Read its silhouette/base.
4. **Set ONE light direction** (default top-left, slightly above) and keep it for
   the whole sprite (`rules/00` §4).
5. **Shade in order** (`rules/00` §6): base → **core shadow** on planes away from
   light → **highlight** sparingly on planes toward it. Use `live_draw_pixels` /
   `live_use_tool` with ramp colors only.
   - Cast shadows: harder edge, darker step. Form shadows: step along the ramp.
   - Highlights are the smallest area; don't over-light (no "wet" look).
6. **Selective outline** (`rules/02` §1): 1-px, color it by darkening+hue-shifting
   the adjacent fill (not flat black) unless the style demands black.
7. **Clean up** (`rules/02` §4–6): remove jaggies/orphan pixels; ensure no
   **banding** (don't run a shadow step parallel-adjacent to the outline); hand-AA
   only key inside curves with in-ramp colors; no 1-px-thick features (chunky rule).
8. **Un-solo, judge at 100%.** Verify the light reads and nothing is muddy.

## Avoid (auto-fail)
Pillow-shading (halo from outline inward, no light dir), banding, accidental AA,
value-only ramps (no hue-shift), off-palette colors.

## Definition of done
Passes `rules/06` sections B (color), C (form/light), D (linework) for the layer.

## Eval prompts
- "Shade the Head with top-left light using the skin ramp" → cool-shifted shadow
  lower-right, warm highlight upper-left, 1-px hue-shifted outline, no banding.
- "This sprite looks puffy/blurry" → diagnose pillow-shading, re-shade to a single
  light direction.
