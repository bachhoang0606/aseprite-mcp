# AGENTS.md — Aseprite pixel-art discipline (Codex & any non-Claude-Code agent)

This repo is a **live-first Aseprite pixel-art system**. Claude Code enforces the
discipline below via plugin hooks; **Codex has no hooks, so follow this file + the
`pixel-art` skill explicitly.**

## Tools (MCP)
- **`aseprite-live`** (preferred) — draws **live in the open Aseprite window**. Tools:
  `live_preflight`, `live_draw_pixels`, `live_new_frame`, `live_set_cel_properties`,
  `live_set_frame_properties`, `live_new_tag`, `live_save_preview`, `live_save_filmstrip`,
  `live_list_frames`/`live_list_tags`, `live_save_sprite`/`live_save_sprite_as`,
  `export_sprite`/`export_spritesheet`, `live_ensure_layer`, `live_clear_cel`, `live_rename_layer`.
- **`aseprite`** (batch, uv/Python) — writes to a FILE via the Aseprite CLI; it does
  **not** appear in the open window. Use only for deliberate file-level ops.

## Non-negotiables (every sprite task)
1. **Preflight.** Call `live_preflight`; proceed only when `ready:true`. If false, the
   live bridge is down — Smart App Control must be **OFF**, Aseprite open with the
   `aseprite-mcp-plugin` extension. Do **not** silently draw to a file instead.
2. **Palette before pixels.** Lock a small palette; pick its size from the resolution
   (`knowledge/scale-hierarchy.json`: 8 colours @16px, 16 @32px…). Draw ONLY from it.
3. **Rig before animation.** Put limbs/shield/weapon on their own layers before animating
   (a 2-layer Body/Legs split lets the body bob while feet stay planted).
4. **Self-review before done.** Score against `rules/06` (static) and `rules/07` (animation);
   fix every must-fail.
5. **Save early & often.** `live_save_sprite_as` to a `.aseprite` immediately after opening,
   and `live_save_sprite` after each milestone — a closed **unsaved** live sprite loses ALL
   frames/layers.

## Draw-by-code, not generate
The system's core is **placing pixels programmatically** (`live_draw_pixels` with coords +
palette hex) inside the **perception loop**: draw → `live_save_preview` (upscaled, gutter) →
read the pixels → self-critique → fix. Image **generation is an opt-in escape-hatch** only
for organic-from-scratch subjects, and even then the result is disciplined into palette-locked
pixels. At 16–32px, generation is mush; code-drawing gives pixel control, palette discipline,
reproducibility, and machine-checkable quality.

## Machine gates (run via shell)
- `python tools/timing_lint.py clip.json` — per-state frame timing vs `knowledge/timing-budgets.json`.
  Feed it raw `live_list_frames` + `live_list_tags` output (it reads Aseprite's seconds/1-based shapes).
- `python tools/lint_sprite.py sprite.png --palette knowledge/palettes/<pal>.json` — off-palette / orphan / size-budget.
- `python tools/silhouette_iou.py` — cross-frame volume/proportion drift (0.80 floor).

## Gotchas learned the hard way
- Parallel `live_new_frame` copies can inherit the **active** frame's cel offset and scramble
  per-frame durations — after bulk frame creation, normalize cels (`live_set_cel_properties`)
  and set durations **last**, then verify with `live_list_frames`.
- `live_draw_pixels` makes full-canvas (origin 0,0) cels, so cel x/y offsets are absolute; erase
  with colour `#00000000`.

## Reference
Full rulebook: `rules/00..07`. Verb playbooks: `skills/*/SKILL.md`. The condensed Codex skill
`pixel-art` (in `~/.agents/skills/pixel-art/` or `.agents/skills/pixel-art/`) encodes all of this.
