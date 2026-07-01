---
name: pixel-new
description: Start a NEW pixel-art sprite from scratch in Aseprite — pick canvas size, lock a palette, set up rigged layers, draw the base pose. Use when creating a new character / icon / tile / sprite. Not for editing an existing sprite (use $pixel-animate / $pixel-shade) or generic images.
---

# pixel-new — scaffold a sprite (live, draw-by-code)

Shared discipline (preflight, palette-first, save-early) is in `AGENTS.md` / `$pixel-art`.

## Steps
1. **Preflight** — `live_preflight`; if not `ready:true`, run `$pixel-doctor` (don't draw to a file).
2. **Seed the canvas** — Aseprite `NewFile` hangs (modal), so: write a blank transparent PNG at
   the target size, `live_open_sprite <abs path>.png`, then **`live_save_sprite_as` to `.aseprite`
   immediately** (and save after each milestone — a closed unsaved live sprite loses everything).
3. **Size → palette** — pick size (16/24/32/48/64…) and a palette sized to it (≈8 colours @16px,
   16 @32px, 24 @64px). Lock it first (`$pixel-palette`); draw only from those hex.
4. **Rig layers** — semantic PascalCase layers, parts that move on their own layer (e.g. `Body`,
   `Legs`, `Shield`, `Weapon`) so animation is clean later. `live_ensure_layer` / `live_rename_layer`.
5. **Draw the base pose by code** — `live_draw_pixels {layer, pixels:[{x,y,color}]}` (full-canvas
   cel, origin 0,0). Bold readable silhouette first, then outline (dark, NOT pure `#000000`),
   then a light-from-one-direction ramp.
6. **See & fix** — `live_save_preview inline:true gutter:true`; read pixels, remove orphans/off-ramp, repeat.
7. **Save.**

## Done
A readable base sprite on rigged, semantically-named layers, all pixels on the locked palette,
saved to `.aseprite`. Hand off to `$pixel-animate` for motion, `$pixel-review` for a scored check.
