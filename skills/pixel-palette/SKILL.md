---
name: pixel-palette
description: Set, load, or optimize an Aseprite sprite's palette with hue-shifted ramps, live. Use when the user wants to choose colors, apply a known palette (PICO-8, Game Boy, NES, goblin-default), build ramps, or reduce/clean an existing palette.
argument-hint: "[load <name> | build <materials> | optimize]"
---

# /pixel-palette — lock a disciplined palette

Color discipline is the biggest lever on quality (`rules/01-palette-and-color.md`).
This skill makes the palette explicit and ramp-based **before** drawing.

## Modes
### load <name>
1. `live_preflight` → require ready.
2. Read `knowledge/palettes/<name>.json` (`pico-8`, `game-boy-dmg`, `nes`,
   `goblin-default`).
3. Size the palette: `live_resize_palette` to the color count, then
   `live_set_palette_color` for each index in load order.
4. Confirm with `live_list_palette`. Report the ramps available for shading.

### build <materials>
1. Preflight. For each material (skin, cloth, metal…), construct a **3–5 step
   hue-shifted ramp** (`rules/01` §2–3): dark step shifted cool, highlight shifted
   warm; mid carries most saturation; avoid pure black/white endpoints.
2. Write the colors with `live_set_palette_color`; keep total within the size
   budget (≤8 tiny / ≤16 character / ≤32 detailed).
3. Save the set to `knowledge/palettes/<name>.json` (with `source` + `ramps`) if
   the user wants it reusable.

### optimize
1. Preflight. Pull current colors (`live_list_palette`) and, if needed, pixel data.
2. Flag problems per `rules/01` §6: off-ramp strays, near-duplicate colors (each
   color must have its own identity), value steps too close, count over budget,
   pure-black/white endpoints.
3. Propose a reduced, ramped palette; on approval, remap via `live_set_palette_color`
   (and re-shade affected layers with `/pixel-shade` if hues moved).

## Definition of done
- Palette is small, ramp-organized, hue-shifted, within budget; every ramp step
  is distinct at 100%; no off-ramp colors remain.

## Eval prompts
- "Load pico-8" → 16 colors set in order, ramps reported.
- "Build a hue-shifted green skin ramp + brown leather ramp" → dark steps cooler,
  highlights warmer, ≤ size budget.
- "Optimize this 40-color sprite" → flags near-dupes/strays, proposes ≤16 ramped.
