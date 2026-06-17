---
name: pixel-new
description: Scaffold a new pixel-art sprite live in Aseprite — choose a size budget, lock a palette with ramps, and build the standard rigged layers — so drawing starts on a clean, correct base. Use when the user wants to start a new sprite/character.
argument-hint: "[subject] [size e.g. 32 or 64] [palette e.g. pico-8|goblin-default]"
---

# /pixel-new — scaffold a rigged sprite

Create a correct starting point so quality is right from the first pixel. Applies
`rules/00-core-principles.md` (size budget), `rules/01-palette-and-color.md`
(lock palette), `rules/05-layers-and-rig.md` (rig).

## Steps
1. **Preflight.** `live_preflight` → require `ready:true`. If not, STOP and report
   (do not batch-fallback).
2. **Decide the size budget** (`rules/00` §1). Default 32×32 for a character, 64×64
   if rich detail/animation is wanted, 16×16 for an icon. Smallest size that
   carries the silhouette.
3. **Get a canvas.**
   - If a suitable sprite is already active (`live_get_sprite_info`), use it.
   - Else create one live: `live_run_app_command` `NewFile` with
     `{ "ui": false, "width": W, "height": H, "colorMode": "rgb" }`, then confirm
     with `live_get_sprite_info`.
4. **Lock the palette** via `/pixel-palette` (load `knowledge/palettes/<name>.json`
   or a custom set). Never proceed without a palette.
5. **Build the standard rig** (bottom→top) with `live_ensure_layer` /
   `live_create_group_layer`, named per `rules/05`:
   `Shadow`, `Legs` (or `LegL`/`LegR`), `Body`, `ArmL`, `ArmR`, `Head`.
   Set draw order so `Head` is top, `Shadow` bottom.
6. **Block the silhouette** on a single flat color first (`rules/00` §3) on the
   body/legs layers — do NOT detail yet. Confirm it passes the flat-silhouette
   readability test.
7. **Report** the size, palette, and layer list; hand off to drawing/`/pixel-shade`.

## Definition of done
- Active sprite at the chosen size, palette locked, all rig layers present and
  correctly ordered/named, silhouette blocked and readable at 100%.
- Each layer is a clean shape when soloed (`rules/05` §4).

## Eval prompts (for graded testing)
- "New 32×32 goblin with the goblin-default palette" → sprite 32×32, goblin-default
  loaded, 6 rig layers present, readable silhouette, no off-palette pixels.
- "Start a 16×16 coin icon, pico-8" → 16×16, pico-8 locked, minimal layers, round
  readable silhouette.
- Negative: if `live_preflight` is false the skill STOPS and never creates a file
  on disk via batch tools.
