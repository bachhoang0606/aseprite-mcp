---
name: pixel-palette
description: Build, load, or optimize an Aseprite palette and colour ramps — hue-shifted 3–5 step ramps sized to the sprite resolution. Use when choosing colours, locking a palette before drawing, or fixing muddy / off-palette / value-only art. Not for shading a specific layer (use $pixel-shade).
---

# pixel-palette — lock a disciplined palette

## Rules that make a palette read
- **Size to resolution:** ~8 colours @≤16px, 16 @32px, 24 @64px. Fewer, more deliberate.
- **Ramps of 3–5 steps** per material (skin/steel/cloth…), steps distinct at 100%.
- **Hue-shift the ramp:** cool/blue-shifted shadows, warm/yellow-shifted highlights — NOT a single
  hue getting darker (that reads muddy/value-only).
- **No pure black shadow / pure white highlight** killing the hue (use a dark tinted colour for outline).

## Steps
1. Pick/lock the palette (from Lospec, a reference, or authored). Draw ONLY from it.
2. Save it as JSON for linting, e.g. `{ "colors":[...], "ramps": { "steel":[dark..light] } }`.
3. **Verify:** `python tools/lint_sprite.py <sprite>.png --palette <pal>.json` (from the aseprite-mcp
   repo) → zero `off_palette`. Ramp check: `python tools/ramp_lint.py`.

## Done
A saved palette + named ramps; the art snaps to it with no off-palette strays and no value-only ramp.
