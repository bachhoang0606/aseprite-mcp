# Changelog

## Unreleased

### Added
- **SPEC-005 Phase 4 — `live_save_preview` Set-of-Mark numbered regions (`marks_from`).**
  Overlay numbered badges on regions and return a `marks:[{n, region, bbox}]` map so the
  critic can say "region 3 has a stray pixel" and the orchestrator maps `3 → that
  slice/layer/blob` — no fragile free-form coordinates (research §A SoM). `marks_from`:
  `"slices"` (one per named slice), `"layers"` (one per visible layer's cel at the active
  frame), or `"components"` (one per 4-connected opaque blob). New pure `src/marks.rs`:
  `connected_components` (iterative flood fill mirroring `tools/lint_sprite.py`'s opacity +
  4-neighbour notion), `assign_marks` (numbers 1..N; inverse is `marks[n-1]`), `draw_badge`
  (numbered badge over a neutral box, clamped on-canvas), reusing the one shared bitmap font
  from `gutter.rs`. No new plugin command — `slices`/`layers` reuse `list_slices`/`list_cels`
  ∩ `list_layers`; `components` is pure Rust. Layer visibility honours **effective** group
  visibility (a layer in a hidden group isn't marked) and disambiguates duplicate layer
  names (`Body`, `Body #2`); `components` runs CC at source resolution (reconstructed from
  the upscaled buffer, so it never touches the up-to-67M-px buffer); a `MAX_MARKS` cap keeps
  the largest regions and reports the total in `marks_truncated`. `finish_preview` filters
  regions to the crop window then numbers them (every mark has a visible badge, contiguous
  numbering) and draws each at `band + (centroid − crop)·scale`, returning `marks` even when
  empty ("requested, none found"). Unit-tested (CC disjoint/L-merge/empty, mark numbering +
  inversion, badge bounds/clamp, slice/layer parse, group-visibility, duplicate names,
  crop-window filter under a non-zero crop, marks-over-an-applied-gutter-band, truncation).
  112 unit tests pass; clippy adds no new lints.
- **SPEC-005 Phase 5 — plugin `0.3.2` advertises `perception2`.** The only plugin change
  across SPEC-005 is the Phase-2 `cel.bounds` report in `save_preview`, so the new
  `perception2` feature flag means "`crop="cel"` works"; the gutter / crop-math / inline /
  marks features are server-side and degrade loudly on an old plugin rather than being gated.
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
- **SPEC-005 Phase 1 — `src/gutter.rs`: coordinate gutter compositor (Perception
  fast-follow, research Path 1 §A).** A pure-Rust margin band that labels the
  upscaled preview with **chunky numeric ticks** every 8 source-px along the top and
  left — VLMs are blind to grid geometry, but in-grid numeric labels roughly double
  row/col accuracy ([VLMs are Blind]). A built-in 3×5 bitmap font (no font dep); the
  label colour is the candidate **maximally distant in CIELAB ΔE** from the sprite
  palette *and* the band (reuses `color_ops`), so labels never read as art; and
  because the upscale factor is integer, any (x,y) the agent reads off the gutter
  **inverts back to an exact source coordinate** for `live_draw_pixels`. Refuses a
  tick density below the legibility floor (`step × scale < 24px`). **7 unit tests**
  (inversion identity, off-palette pick, density refusal, byte-faithful art blit).
  See SPEC-005 / research §A.
- **SPEC-005 Phase 1 — gutter wired onto `live_save_preview` (on by default).** The
  preview is upscaled to an in-memory buffer (`preview::render_preview_buffer`), then —
  whenever the tick spacing is legible at the chosen scale — composited with the
  coordinate gutter before the PNG is written. New `gutter` / `gutter_step` options:
  `gutter:false` suppresses it, `gutter:true` requires it (loud `gutter_unreadable`
  refusal if illegible), and the default quietly degrades to a plain preview with a
  `gutter_skipped` note. The result JSON gains `gutter_applied` plus a `gutter`
  `{left_w, top_h, step, image}` sidecar so any (x,y) read off the preview inverts
  exactly (`source = (preview − {left_w,top_h}) / scale`). The legibility floor now
  also rejects spacings where multi-digit labels would overlap, and the label colour
  is steered off the sprite's own sampled colours (`gutter::sprite_palette`). Pure
  helpers `live::finish_preview` + `gutter::sprite_palette` unit-tested (transparent
  art, explicit-require success/refusal, default degrade, write-failure, label-overlap
  refusal). No plugin change — works with any connected plugin version.
- **SPEC-005 Phase 2 — `live_save_preview` region crop (`crop`).** Focus the upscale
  budget on the subject: `crop:"sprite"` (whole canvas, default), `crop:"cel"` (the
  active cel's bbox — a 16×16 cel on a 256×256 canvas now fills ~1024px instead of
  ~64px), or `crop:{x,y,width,height}`. `render_preview_buffer` clamps the rect, crops
  the decoded RGBA, then auto-scales on the **crop's** long edge; `PreviewInfo` gains
  `crop_x/crop_y` and the sidecar a `crop:{x,y}`. The gutter draws labels in **absolute**
  sprite coordinates (`gutter::render_with_gutter_at`, origin = crop), so the agent reads
  the real (x,y) with no arithmetic. `crop:"cel"` resolves from a new read-only `cel`
  bounds field the plugin reports in `save_preview`; an empty cel or an old plugin is a
  loud `cel_bounds_unavailable` degrade (never a silent guess). Pure crop/validation
  helpers unit-tested (`clamp_crop`, `resolve_crop_plan`, `rect_to_crop`,
  `cel_crop_from_response`, crop-then-scale, full-crop no-regression, absolute-label
  origin). 87 unit tests pass; clippy adds no new lints. (Live-verify of `crop:"cel"`
  pending a plugin reload.)
- **SPEC-005 Phase 3 — `live_save_preview` optional inline image return (`inline`,
  [ADR-0007](docs/adr/0007-inline-image-content.md)).** `inline:true` returns the PNG as
  an MCP `image/png` content block (base64) so a vision client sees the pixels directly,
  not just a path — the first tool in the crate to emit image content (`live_save_preview`
  now returns `Result<CallToolResult, McpError>`). The path is always present too, so the
  auto-preview hook and non-vision clients are unchanged (the no-inline wire shape is
  byte-identical). A preview over the 1 MiB ceiling degrades to path + a text note rather
  than blowing the context budget. Pure `preview::read_inline_png` → `InlinePng::{Ready,
  TooLarge}` + a hand-rolled RFC 4648 `base64_encode` (no new dependency); unit-tested
  (known-vector encode, round-trip decode to dimensions, size-guard). 89 unit tests pass;
  clippy adds no new lints.

  [VLMs are Blind]: https://arxiv.org/abs/2407.06581
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
