# Changelog

## Unreleased

### Added
- **`live_frame_diff` — pixel-level diff of two frames as a text grid (Perception
  fast-follow, research Path 1).** Renders `from_frame` and `to_frame` (modal-free
  `save_preview`, 1×) and emits a one-glyph-per-cell grid: `.` = unchanged, `-` =
  erased (became transparent), otherwise the glyph of the **new** colour at that cell
  (with a glyph→`#rrggbb` legend) plus a changed-cell count. Lets the agent see
  EXACTLY what an edit changed, or where two animation frames differ at the pixel
  level (the verify half of the draw→see→fix loop). Validates frames in range and that
  they differ; restores the user's active frame. The pixels→diff transform is pure
  Rust in `src/ascii_view.rs::diff_to_ascii` (4 unit tests); refuses grids > 64×64
  (crop first). Live-verified on a 6-frame sprite (frame 1→3 = 131 changed cells,
  correct grid + palette legend).
- **SPEC-004 Phases 2–4 — live constrained/semantic colour tools (Path 2).** Three
  new `live_*` tools that make every colour operation legal by construction:
  `live_palette_snap` (snap a layer/selection's off-palette colours to the nearest
  CIELAB palette colour), `live_adjust_pixels` (shade by INTENT —
  darken/lighten/hue_shift/colorize, with darken/lighten applying the project
  hue-shift rule and `clamp_to_palette` on by default), and `live_snap_colors`
  (snap a hex list to the active palette WITHOUT editing — legalise a stroke before
  `live_draw_pixels`). The colour math is the pure `color_ops` core; the tools fetch
  a region's *unique* colours (new plugin `get_region_colors`), build a colour→colour
  map in Rust, and apply it in one undoable pass (new plugin `apply_color_map`,
  clone→mutate→reassign). RGB sprites only (v1). Plugin advertises
  `features += ["color_ops"]` (v0.3.0); old plugins reject the commands per-command
  (ADR-0005). Live E2E pending an Aseprite run.
- **SPEC-004 Phase 1 — `src/color_ops.rs`: pure constrained/semantic colour core
  (Path 2).** Real **CIELAB + CIEDE2000** palette snapping (validated against the
  Sharma reference pairs) — the honest version of the competitor pixel-mcp's
  claimed-but-RGBA "LAB snap"; plus intent ops `darken`/`lighten` (value shift **+**
  the project hue-shift rule: shadows cool toward blue, highlights warm toward
  orange), `hue_shift`, `colorize`, `clamp_to_palette`, and `build_color_map` (the
  per-unique-colour map the live tools will apply). Pure Rust, **11 unit tests**
  incl. a brute-forced "LAB nearest ≠ RGBA nearest" proof and the darken-cools /
  lighten-warms direction. Live tools (`live_palette_snap`, `live_adjust_pixels`,
  `live_snap_colors`) are SPEC-004 Phases 2–4. See SPEC-004 / research §B,§D.
- **`live_save_filmstrip` — review animation in one image (Perception
  fast-follow, research Path 1).** Composites every frame into a near-square
  row-major grid (gray gaps between cells), nearest-neighbor upscaled toward
  ~1024px. The Claude API only reads the *first* frame of an animated GIF, so a
  strip is the only way to review a walk/attack cycle and the #1 animation failure
  (cross-frame proportion drift). Renders each frame via the modal-free
  `save_preview` and restores the user's active frame; the frames→grid compositor
  is pure Rust in `src/filmstrip.rs` (5 unit tests). Live-verified on a 6-frame
  sprite (3×2 grid, 1040×702).
- **`live_ascii_view` — text-grid readback of the active frame (Perception
  fast-follow, research Path 1).** One glyph per pixel (`.` = transparent) with
  tens/units column rulers, row labels, and a glyph→`#rrggbb` legend. LLMs read a
  one-token-per-cell grid far more reliably than a small sprite image (Text2Space),
  so this is the agent's exact, token-space view for VERIFYING pixel values /
  positions — and it works for non-vision clients. Reuses the modal-free
  `save_preview` 1× render; the pixels→text transform is pure Rust in
  `src/ascii_view.rs` (4 unit tests). Refuses sprites > 64×64 (crop first).
- **SPEC-003 tilemap / tileset / autotile tool family (Phases 1, 2, 5).** Seven new
  live tools: `live_create_tilemap_layer`, `live_list_tilesets`, `live_get_tileset`
  (with a vision-legible upscaled packed PNG), `live_stamp_tiles` (the tile-grid
  analogue of `live_draw_pixels`), `live_set_tile_data`, `live_pack_similar_tiles`
  (dedupe a painted mockup into a tileset + reconstructing tilemap), and
  `live_export_tileset` (Tiled `.tsj` with a blob47 wangset, Godot `.tres`, or JSON
  + a sibling packed PNG; whole-canvas). Engine-format serializers are pure Rust in
  `src/tileset_export.rs` (9 unit tests, reuses the Phase-3 `autotile` blob47 order);
  tile CRUD/dedupe are new `plugin.lua` handlers. The wire protocol stays v1; the
  plugin advertises `features=["tilemap"]` and old plugins reject the new commands
  loudly per-command (ADR-0005). `scripts/smoke/tilemap-selftest.lua` exercises the
  Aseprite-side primitives for the live E2E check. Joins the already-landed Phase 3
  (`src/autotile.rs` blob-47 bitmask) and Phase 4 (`tools/seam_lint.py`).
  Live-verified end-to-end on Aseprite 1.3.17.2: paint mockup → `pack_similar_tiles`
  (16 cells → 2 unique tiles, pixel-faithful) → `get_tileset` vision preview →
  `stamp_tiles` (overwrite + fill cells, confirmed by render) → `set_tile_data` →
  export Tiled/Godot/JSON + blob47 wangset (grid round-trips exactly). **Three bugs
  the live run surfaced and fixed** (plugin 0.2.3): (1) `stamp_tiles` sent the nested
  `LiveTile.tile_index` as snake_case on the wire — now remapped to `tileIndex`;
  (2) `create_tilemap_layer`/`pack_similar_tiles` anchor onto a non-tilemap layer
  before `NewLayer{tilemap=true}` so the new tileset takes the requested tile size
  instead of inheriting the active tilemap's grid; (3) JSON numbers decode to Lua
  floats and `Image:putPixel` writes a float tile index as the empty tile 0 — the
  stamp/rebuild path now `math.floor`-coerces tile indices to integers.
- `live_save_preview` tool + auto-preview hook rewired to it: saves a faithful 1×
  copy, then nearest-neighbor upscales it in the Rust server (live document
  untouched) so the sprite's long edge lands near ~1024px. Raw 1× previews of
  16–64px sprites are below the resolution a vision model can read reliably, so
  this is the perception half of the agent's see→fix loop (research doc Path 1).
  Pure-Rust image math in `src/preview.rs` (new `image` png-only dep), 6 unit tests.

### Fixed
- **Adversarial-audit follow-ups** (no behaviour-changing bugs were found; these
  harden error reporting + a documented contract):
  - Colour ops: `selection_only=true` with **no active selection** now returns an
    `empty_selection` error instead of silently recolouring the whole layer; a
    group/tilemap target now returns a clear `not_an_image_layer` error instead of
    a confusing "0 colours changed" no-op (plugin returns an `imageLayer` flag);
    `adjust_pixels(op=snap)` now requires a palette even when `clamp_to_palette=false`.
  - `live_ascii_view`: the size cap is now a true **per-edge 64×64** check (was a
    4096-*cell* area cap, which let a 256×16 sprite through and produced an
    unreadable 256-glyph row) — matches the documented "64×64" contract.
  - `live_set_tile_data` description corrected: tile user-data is stored in the
    `.aseprite` file and read back by `live_get_tileset`, but is **not** emitted by
    `live_export_tileset` (Tiled wangsets come from the blob47 layout).
  - Removed dead `get_or_create_tilemap_cel` (obsolete after the stamp rewrite).

## v0.1.0 — 2026-06-10

First tagged release: the Claude Code pixel-art plugin for Aseprite — live MCP
drawing, encoded rules, `/pixel-*` skills, review/rig/palette/animation agents,
live-first hooks, 3-OS CI quality gates. Checklist v18 ≈95.8/100.

### Added
- Standalone `aseprite-live-bridge` singleton (decoupled WS bridge, ports 9876/9877)
  so MCP server restarts never drop the Aseprite plugin connection (SPEC-001, ADR-0002).
- Full live tool surface (`live_*`) for sprite, layer, frame, cel, drawing, tag,
  slice, selection, palette, and app-command workflows, with `live_preflight` guard.
- Claude Code plugin packaging: `.claude-plugin/plugin.json`, marketplace manifest,
  `mcp/aseprite-live.json`, install/uninstall scripts, QUICKSTART + ARCHITECTURE docs.
- Pixel-art expertise pack: `rules/` rulebook, `knowledge/` palettes/glossary/references,
  `/pixel-*` skills, and pixel-critic / palette-smith / rig-builder / animation-director agents.
- Hooks: live-first batch-draw guard, session health check, palette-lint on save,
  auto-preview export (`mcp_tool` PostToolUse).
- Quality gates in CI (3-OS): Rust unit + schema-contract tests, sprite linter,
  visual-regression golden diff, Tier-A eval harness, hook contract tests,
  install verification, packaging manifest validation.
- Security: `ASEPRITE_MCP_ALLOW_LUA` opt-in gate for `run_lua_script` (SPEC-002,
  ADR-0003), loopback-only bridge binding test, `SECURITY.md`.
- `export_spritesheet` now emits `meta.frameTags` in the JSON data by default
  (`--list-tags`), with opt-in `list_layers` / `list_slices` (closes Tier-B 5.4 gap).
- MIT `LICENSE` file; Dependabot config for cargo + GitHub Actions.

### Changed
- Hardened Lua plugin error shape and self-healing reconnect (no Aseprite restart needed).
- Live protocol capabilities reporting and namespaced request IDs.

### Fixed
- Tool JSON-Schema validity (`params` boolean-schema regression) with contract tests.
- Spurious `live_timeout` during unfocused Aseprite periods.
