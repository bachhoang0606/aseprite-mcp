# Live Tools

Live tools operate on the currently running Aseprite UI session through the installed Lua plugin. They do not open Aseprite in batch mode and should not close/reopen Aseprite during normal editing.

All live tools use the `live_` prefix. Batch/file tools keep their existing names.

## Core Workflow

1. Start the Rust MCP server.
2. Open Aseprite with the MCP plugin installed.
3. **Preflight:** call `live_preflight` and proceed only when `ready == true`. If `ready == false`,
   stop and surface the `directive`/`remediation` to the user — do **not** fall back to batch/file
   drawing tools (they edit disk, not the open window). `live_status.connected` carries the same flag.
4. Use `live_get_capabilities` to verify plugin compatibility.
5. Edit the active sprite with live tools.
6. Save with `live_save_sprite`.

> Guard enforcement: live mutating tools refuse loudly with `code: "live_not_connected"` and
> `details.doNotFallBackToBatch: true` when the plugin is disconnected, instead of timing out silently.

## Tool Groups

- Preflight/status: `live_preflight` (connectivity guard — call before any edit), `live_status`, `live_get_capabilities`.
- Session/sprite: `live_list_sprites`, `live_open_sprite`, `live_activate_sprite`, `live_get_active_site`, `live_get_sprite_info`, `live_set_sprite_properties`, `live_save_sprite`, `live_save_sprite_as`, `live_save_copy_as`, `live_save_preview`, `live_ascii_view`, `live_close_sprite`, `live_resize_canvas`.
  - `live_save_preview` writes a **vision-legible** PNG: it saves a faithful 1× copy, then nearest-neighbor upscales it (in the Rust server, so the live document is untouched) so the sprite's long edge lands near ~1024px — raw 1× previews of 16–64px sprites are below the resolution a vision model can read. Optional integer `scale`; otherwise auto (capped 16×). Returns source/scale/preview sizes. This is what the auto-preview hook calls; use it (not `live_save_copy_as`) whenever an agent needs to *see* its own work.
  - `live_ascii_view` returns the active frame as a **text grid** — one glyph per pixel (`.` = transparent) with tens/units column rulers, row labels, and a glyph→`#rrggbb` legend. LLMs read a one-token-per-cell grid far more reliably than a small image, so use it to *verify* exact pixel values/positions, count cells, or check work on non-vision clients (complement to `live_save_preview`). Active frame, visible layers; refuses sprites > 64×64 (crop first).
  - `live_save_filmstrip` composites **every animation frame** into one near-square row-major grid (gray gaps between cells), nearest-neighbor upscaled. The Claude API only reads the *first* frame of an animated GIF, so this is the way to **review animation** (walk/attack cycle) in a single image — check timing and cross-frame proportion drift. Restores the user's active frame. Optional `scale`; returns frames/cols/rows + strip size.
- Layer: `live_list_layers`, `live_ensure_layer`, `live_set_active_layer`, `live_rename_layer`, `live_create_group_layer`, `live_set_layer_visibility`, `live_set_layer_properties`, `live_delete_layer`.
- Frame/cel: `live_list_frames`, `live_ensure_frames`, `live_set_active_frame`, `live_set_frame_properties`, `live_new_empty_frame`, `live_new_frame`, `live_delete_frame`, `live_list_cels`, `live_new_cel`, `live_set_cel_properties`, `live_delete_cel`, `live_clear_cel`.
- Drawing: `live_draw_pixels`, `live_use_tool`.
- Tags/slices: `live_list_tags`, `live_new_tag`, `live_set_tag_properties`, `live_delete_tag`, `live_list_slices`, `live_new_slice`, `live_set_slice_properties`, `live_delete_slice`.
- Selection/palette: `live_get_selection`, `live_set_selection`, `live_list_palette`, `live_set_palette_color`, `live_resize_palette`.
- Advanced privileged tool: `live_run_app_command`.
- Tilemap (SPEC-003, needs a plugin with the `tilemap` capability): `live_create_tilemap_layer`, `live_list_tilesets`, `live_get_tileset`, `live_stamp_tiles`, `live_set_tile_data`, `live_pack_similar_tiles`, `live_export_tileset`.
  - A tilemap cel is an image whose "pixels" are tile indices, so `live_stamp_tiles` mirrors `live_draw_pixels` with `{x, y, tile_index}` batches (x/y are tile-grid cells, not pixels). `live_get_tileset` with a `filename` returns a vision-legible upscaled packed PNG of the tiles (same path as `live_save_preview`). `live_pack_similar_tiles` deduplicates a painted mockup into a tileset + reconstructing tilemap. `live_export_tileset` writes a Tiled `.tsj` (with a blob47 wangset when `layout=blob47`), Godot `.tres`, or JSON, plus a sibling packed PNG, exporting the whole canvas. LDtk needs no exporter — it reads `.aseprite` directly; use `live_save_sprite`.
  - Check `live_get_capabilities().features` for `"tilemap"` first; on an older plugin these tools return `unsupported_command` (loud, per-command — ADR-0005).
- Colour ops (SPEC-004, needs the `color_ops` capability; **RGB sprites only**): `live_palette_snap`, `live_adjust_pixels`, `live_snap_colors`.
  - These make colour operations **legal by construction** using a real **CIELAB ΔE** snap (not RGBA). `live_palette_snap` recolours a layer/selection's off-palette colours to the nearest palette colour. `live_adjust_pixels` shades by **intent** — `op = darken|lighten|hue_shift|colorize|snap`; darken/lighten apply the project hue-shift rule (shadows cool toward blue, highlights warm toward orange) and `clamp_to_palette` (default true) keeps the result palette-legal. `live_snap_colors` snaps a hex list to the active palette **without editing** — use it to legalise a stroke's colours before `live_draw_pixels`. All operate on a region's *unique* colours (small wire payload), applied in one undoable pass. Pass `selection_only` to limit to the active selection.
  - Check `live_get_capabilities().features` for `"color_ops"`; older plugins return `unsupported_command` (ADR-0005).

See `docs/live-command-matrix.md` for coverage and smoke cases.

