---
name: pixel-art
description: Orchestrate a COMPLETE pixel-art sprite/character end-to-end in Aseprite (scaffold → palette → draw → animate → review → export) via the aseprite-live MCP, or any pixel/sprite/Aseprite task that does NOT map to a specific verb. For focused steps prefer the granular skills $pixel-new, $pixel-palette, $pixel-animate, $pixel-review, $pixel-doctor. Do NOT use for generic raster/photo editing.
---

# Pixel-art collaborator (Aseprite, live-first)

Port of the `aseprite-pixel-art` Claude Code plugin into a Codex skill. It draws
**live in the open Aseprite window** via the `aseprite-live` MCP server, with palette
discipline + a perception review loop. Full rulebook: `rules/00..07` in the
`bachhoang0606/aseprite-mcp` repo; this skill condenses it.

## Core loop (do this for every task)
1. **Preflight** — `live_preflight`; only proceed when `ready:true`. If false: Aseprite
   open + `aseprite-mcp-plugin` extension enabled + **Smart App Control OFF** (else the
   bridge is blocked). Never silently draw to a file as a "workaround".
2. **Palette first** — lock a small palette sized by resolution (8 colours @≤16px, 16
   @32px, 24 @64px). Draw ONLY from it (hex `#rrggbb`).
3. **New canvas** — `live_open_sprite` a blank seed PNG (Aseprite `NewFile` hangs modal),
   then **immediately `live_save_sprite_as` to `.aseprite`** and save after each milestone.
4. **Draw by code** — place pixels with `live_draw_pixels` (coords + palette hex). Rig
   parts on their own layers (e.g. Body + Legs) before animating.
5. **See your work** — `live_save_preview inline:true gutter:true` (nearest-upscaled). Read
   the actual pixels; fix orphans/off-ramp/wrong-light. Repeat draw→preview→fix.
6. **Self-review** — score `rules/06` (static) + `rules/07` (animation). Fix must-fails.

## Verb playbook (act as each `/pixel-*` command)
- **pixel-new** — scaffold: size + palette + rigged layers, base pose. Draw by code.
- **pixel-palette** — build/optimize a palette: 3–5 step **hue-shifted** ramps (cool
  shadows/warm highlights), no pure black/white; save `knowledge/palettes/<x>.json`.
- **pixel-shade** — apply a ramp to a layer: one light direction, no pillow-shading, no banding.
- **pixel-animate** — frames + tags. Idle: bob the Body cel `y` via `live_set_cel_properties`
  (feet planted on a separate Legs layer). Walk: alternate leg poses (clear Legs cel + redraw)
  + body bob. Attack: anticipation → strike → **impact HOLD ≥150ms** → recovery. Set durations
  LAST, then verify with `live_list_frames` (parallel frame-copies can scramble them).
- **pixel-tileset** — dedup tiles + seam-match edges.
- **pixel-export** — `export_sprite` (GIF, integer scale) / `export_spritesheet` (+ JSON
  `meta.frameTags`). Integer scale only; remind engine: Nearest filter.
- **pixel-review** — scored critique vs `rules/06`+`07`; run the machine gates below and cite them.
- **pixel-reference-motion** — rotoscope a video/GIF: import per-frame ref on a locked layer, trace clean.
- **pixel-asset** — find CC0 palette/asset (Lospec), import with provenance.
- **pixel-generate** — OPT-IN only for organic-from-scratch subjects: gate (usually draw
  directly), then cheapest generator (Codex `$imagegen` if apt), then discipline into palette pixels.
- **pixel-doctor** — diagnose the live bridge: `live_preflight`; check port 9876 listener,
  Aseprite extension enabled, SAC off, one non-orphan `aseprite_mcp.exe`.

## Tool cheatsheet (aseprite-live MCP)
`live_preflight` · `live_open_sprite` · `live_save_sprite_as`/`live_save_sprite` ·
`live_ensure_layer`/`live_rename_layer` · `live_draw_pixels {layer,frame,pixels:[{x,y,color}]}`
(erase = `#00000000`; cels are full-canvas origin 0,0) · `live_new_frame {source_frame}` ·
`live_set_cel_properties {layer,frame,x,y}` (partial update keeps the other axis) ·
`live_set_frame_properties {frame,duration}` (seconds) · `live_new_tag {name,from_frame,to_frame,repeats}`
· `live_clear_cel` · `live_list_frames`/`live_list_tags` · `live_save_preview`/`live_save_filmstrip`
· `export_sprite`/`export_spritesheet`.

## Machine gates (shell — run them, don't eyeball)
- `python tools/timing_lint.py clip.json` — feed raw `live_list_frames`+`live_list_tags`
  (it handles Aseprite seconds + 1-based + `fromFrame/toFrame`). Flags too-fast/slow,
  no-impact-hold, uniform timing, looping death.
- `python tools/lint_sprite.py sprite.png --palette knowledge/palettes/<pal>.json` — off-palette / orphan / size-budget.
- `python tools/silhouette_iou.py` — cross-frame volume drift (0.80 floor).

## Definition of done
A reproducible sprite/animation drawn from a locked palette, previewed at scale, passing
`rules/06`/`07` and the machine gates, saved to `.aseprite`, exported with metadata.
