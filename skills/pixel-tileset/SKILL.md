---
name: pixel-tileset
description: Build a deduplicated tileset + tilemap live in Aseprite and export it to a game engine — paint a mockup and auto-pack it (or author tiles and stamp them), see the packed tiles, validate seams, then export to Tiled/Godot/JSON. Use when the user wants tiles, a tileset, a tilemap, terrain, or an autotile/wang set.
argument-hint: "[subject e.g. grass terrain] [tile size e.g. 16] [engine tiled|godot|json]"
---

# /pixel-tileset — build & export a tileset (SPEC-003)

Turn painted tiles into a **deduplicated tileset + tilemap** that an engine can read,
without leaving the open Aseprite window. Wraps the live tilemap tools
(`live_create_tilemap_layer` → `live_stamp_tiles` / `live_pack_similar_tiles` →
`live_get_tileset` → `live_export_tileset`). Palette discipline from
`rules/01-palette-and-color.md`; seam discipline (below) is the tileset-specific gate.

## Preconditions (check FIRST, in order)
1. **Preflight.** `live_preflight` → require `ready:true`. If false, STOP and report —
   never fall back to batch/file tools (`docs/adr/0001-batch-vs-live-tools.md`).
2. **Capability gate.** `live_get_capabilities` → require `"tilemap"` in `features`
   (ADR-0005). If absent, the connected plugin is too old: STOP and tell the user to
   update/reload the Aseprite plugin — the tilemap commands reject loudly, so do NOT
   retry or work around it.
3. **Palette locked.** Tiles must be drawn from a locked palette (`/pixel-palette`).
   Off-palette tiles fragment the tileset (near-duplicate tiles that won't dedupe).

## Steps
1. **Pick the tile size** (default 16×16; 8×8 for fine terrain, 32×32 for chunky props).
   One grid for the whole set — mixing sizes defeats dedup and autotiling.
2. **Choose an authoring path:**
   - **A — Paint then pack (recommended for terrain).** Paint a seamless mockup on a
     normal layer (palette-locked; make repeating edges match so neighbours tile), then
     `live_pack_similar_tiles { tile_width: N, layer: "<mockup>", tilemap_layer: "Tilemap" }`.
     It dedupes the cells into a tileset + a tilemap layer that reconstructs the mockup
     pixel-for-pixel; read back the **cells → unique tiles** efficiency stat (high reuse =
     good, tile-able art).
   - **B — Author then stamp (for hand-placed/structured sets).**
     `live_create_tilemap_layer { name: "Tilemap", tile_width: N }`, draw each tile, then
     `live_stamp_tiles { layer: "Tilemap", tiles: [{x, y, tile_index}, …] }` — `x`/`y` are
     tile-grid **cells** (columns/rows), not pixels; `tile_index` 0 = empty.
3. **SEE the tiles.** `live_list_tilesets`, then
   `live_get_tileset { layer: "Tilemap", filename: "C:/tmp/tiles.png" }` to write a
   vision-legible packed PNG (nearest-neighbor upscaled) and confirm the tiles are what
   you intended. Also `live_save_preview` to see the laid-out tilemap on the canvas.
4. **(Optional) tag tiles** for engine terrain/collision:
   `live_set_tile_data { layer: "Tilemap", tile_index: i, data: "solid" }` (stored in the
   `.aseprite`; read back by `live_get_tileset`). Note: `live_export_tileset` does NOT emit
   this field — Tiled wangsets come from the `blob47` layout, not per-tile data.
5. **Validate seams** (deterministic gate, best for repeating/terrain tiles). Render the
   tilemap to a 1× PNG (`live_save_preview { scale: 1 }`) or use the exported sibling PNG,
   then:
   `python ${CLAUDE_PLUGIN_ROOT}/tools/seam_lint.py --strip <render>.png --tile-width N --tile-height N`
   — it asserts every horizontally adjacent tile pair shares a pixel-matching edge (exit 1
   on a mismatch). For a single check use `--pair A.png B.png --side right`. A level with
   intentional gaps will report non-seams — use `--warn-only` there and only hard-fail
   seamless terrain.
6. **Export to the engine** (writes the file + a sibling packed-tileset PNG):
   `live_export_tileset { target: "tiled"|"godot"|"json", path: "<out>", layer: "Tilemap",
   layout: "none"|"blob47"|"wang16" }`. Use `layout: "blob47"` for 47-tile terrain
   (emits a Tiled wangset). It exports the WHOLE canvas. **LDtk needs no export** — it
   reads `.aseprite` directly with hot-reload, so just `live_save_sprite`.
7. **Report** the tile size, tileset size + dedup efficiency, seam-lint result, and the
   exported file path(s).

## Definition of done
- A tilemap layer + a deduplicated tileset exist live; the packed PNG was reviewed and
  matches intent; tiles are all on the locked palette (no near-duplicate off-palette tiles).
- For seamless/terrain sets, `seam_lint.py --strip` passes (exit 0) on the rendered layout.
- An engine file (`.tsj`/`.tres`/`.json`) + its sibling PNG are written (or `.aseprite`
  saved for LDtk), and the output path is reported.

## Eval prompts (for graded testing)
- "Make a 16×16 grass terrain tileset and export for Godot" → palette locked, seamless
  grass mockup packed (cells → fewer unique tiles), packed PNG reviewed, `seam_lint --strip`
  passes, `.tres` + PNG written.
- "Build a 47-tile blob autotile set for Tiled" → tile grid authored/packed, exported with
  `layout: "blob47"` → `.tsj` carries a wangset; seams verified.
- Negative — capability: if `live_get_capabilities` lacks `"tilemap"`, STOP and tell the
  user to update the plugin; never silently no-op or batch-fallback.
- Negative — preflight: if `live_preflight` is false the skill STOPS and writes nothing to
  disk via batch tools.
