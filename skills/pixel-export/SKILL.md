---
name: pixel-export
description: Export the current Aseprite sprite to game-ready output — PNG, animated GIF, or a packed spritesheet with JSON metadata. Use when the user wants to ship/preview a sprite or hand frames to a game engine.
argument-hint: "[png|gif|spritesheet] [output path] [scale]"
---

# /pixel-export — game-ready output

Produce clean deliverables and verify them at 100%. Export is the one place batch
file tools are appropriate (explicit, deterministic offline generation — see
`docs/adr/0001-batch-vs-live-tools.md`), but always source from the live sprite.

## Steps
1. **Preflight** (`live_preflight`) and **save the live sprite** first
   (`live_save_sprite`) so disk matches what's on screen.
2. Confirm content with `live_get_sprite_info` / `live_list_tags` (frames + tags).
3. **Export by target:**
   - **PNG (single frame / flattened):** `live_save_copy_as` to `name.png`
     (Aseprite infers format from extension), or batch `export_sprite`.
   - **Animated GIF:** export with the animation tag; verify loop/timing.
   - **Spritesheet + JSON:** batch `export_spritesheet` → packed sheet plus a
     JSON frame map (engine-ready; includes `meta.frameTags` by default so engines
     can key animations by tag). Choose a sane sheet layout (rows by tag).
4. **Scale:** export at 1× for the canonical asset; offer an upscaled preview
   (integer scale only — 2×/3×/4×, never fractional) for sharing.
5. **Verify**: open/inspect the output; check transparency, frame order, and that
   it reads at 100%. Report the file path(s) and dimensions.

## Definition of done
Output exists at the requested path, correct format/frames/tags, transparent where
expected, integer-scaled, verified to match the live sprite.

## Eval prompts
- "Export this as a PNG at 4×" → `name.png`, integer 4× preview + 1× canonical,
  transparent bg.
- "Give me a spritesheet + JSON for the walk + attack tags" → packed sheet + JSON
  frame map covering both tags.
