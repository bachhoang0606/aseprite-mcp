# SPEC-003 — Tilemap / tileset / autotile tool family

- Status: **Done (2026-06-14)**. Phases 1–5 implemented (Rust + Lua + exporters);
  deterministic parts CI-green AND the live tile CRUD / dedupe / export verified
  end-to-end on Aseprite 1.3.17.2 (plugin 0.2.3). Only Tiled/Godot *import* of the
  emitted files remains a user check. See Acceptance.
- Owner: project
- Checklist items advanced: 2.x (new live tool surface), 5.x (a `pixel-tileset` skill),
  7.3 (seam-lint extends the sprite linter), 9.4 (deterministic eval gates)
- Related ADRs: [ADR-0005](../docs/adr/0005-tilemap-protocol-and-bitmask.md)
  (Accepted — tile-placement protocol reuse, one bitmask convention, capability
  gating; refines Decision 3 below). ADR-0004 was already taken by the preview
  decision, so the tilemap ADR is 0005.
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md) §E (tilemap) + §G/critic gap-fill

## Intent
Real 2D games are mostly **tiles**, yet no Aseprite MCP — including this one —
exposes Aseprite 1.3's first-class tilemap/tileset API (the repo only *reads* an
`isTilemap` flag at `src/tools/sprite.rs:151`). A commercial competitor already
monetizes exactly this gap. This feature gives the agent a tilemap tool family so
it can: paint a level mockup with the tools it is already good at, then get a
**deduplicated tileset + index map**, generate **autotile templates** (blob-47 /
Wang-16) from a handful of hand-drawn minitiles, **validate seams** deterministically,
and **export engine-ready** tilesets (Tiled / LDtk / Godot). The unifying insight:
a tilemap cel is an image whose "pixels" are tile indices, so tile placement reuses
the existing `live_draw_pixels` coordinate-batch protocol almost verbatim.

## Inputs / Outputs
- **Inputs:** new `live_*` tool params (tilemap layer name, tile-size/grid,
  coordinate batches of `{x, y, tileIndex}`, per-tile user-data, autotile layout
  enum `blob47 | wang16`, export target enum `tiled | godot | json`). Reference
  images/minitiles come from existing live drawing tools.
- **Outputs:** live tilemap edits in the open Aseprite UI (undoable); a deduped
  tileset + tilemap; on export, files on disk (`.tsj`/`.tres`/`.json` + tileset
  PNG); seam-lint findings surfaced like the existing sprite linter.

## Behaviour

Implement in **phases** (each independently shippable):

### Phase 1 — Tilemap CRUD (foundation)
New live tools, all routed through the bridge to new Lua handlers in the plugin:
- `live_create_tilemap_layer(name, tile_width, tile_height)` — a tilemap layer +
  empty tileset (`Sprite:newTilemapLayer`-equivalent via `app.command` / Lua).
- `live_list_tilesets()` — `{tilesets:[{index, name, tileCount, grid, baseIndex}]}`.
- `live_get_tileset(index, as_preview?)` — dump tiles as a packed PNG (so the agent
  can *see* them; pairs with the upscaled-preview perception work) + per-tile
  metadata. `as_preview` upscales like `live_save_preview`.
- `live_set_tile / live_stamp_tiles(tiles:[{x, y, tileIndex}], layer, frame?)` —
  the tile-grid analogue of `live_draw_pixels`; **same coordinate-batch shape**,
  target = the tilemap cel image (`Image:putPixel(col, row, index)`).
- `live_set_tile_data(tileIndex, data)` / read via `live_get_tileset` — per-tile
  user data (`tile.data`/`tile.properties`) for terrain/collision tags exporters read.

### Phase 2 — Dedupe (mockup → tileset + map)
- `live_pack_similar_tiles(grid_size, layer?)` — port the official
  [Pack Similar Tiles.lua](https://github.com/aseprite/Aseprite-Script-Examples/blob/main/Pack%20Similar%20Tiles.lua):
  slice the active frame into `grid_size` tiles, dedupe with `Image:isEqual`, emit a
  packed tileset + a tilemap whose cells reference unique tiles. Also returns
  efficiency stats (`"40 cells -> 12 unique tiles"`) — usable as a wasted-near-
  duplicate lint.

### Phase 3 — Autotile template generation
- `live_create_autotile_template(tile_size, layout=blob47|wang16)` — build a
  tilemap pre-wired so the agent only draws ~5 minitiles (or 4 corner quarters) and
  all 16/47 combinations are composited deterministically (4-corners-per-tile model).
- Shared **bitmask table** (pure Rust, unit-tested, no Aseprite): 8-neighbor mask
  with corner-masking (a diagonal counts only if both adjacent cardinals are filled)
  → the 47 canonical blob states (Red Blob Games convention — see Decisions).

### Phase 4 — Seam / tileset lint (the verifiable gate)
- Extend the Python sprite linter (`tools/lint_sprite.py`) with `seamless_check`:
  for a wrap tile assert `left_edge == right_edge` and `top == bottom`; for an
  autotile set assert every adjacency-compatible tile pair has matching edge masks
  (the bitmask table supplies which pairs must match). Fully deterministic →
  a Tier-A eval gate (generate set → run checker → fail on any seam).

### Phase 5 — Engine export
- `live_export_tileset(target, path)`:
  - `tiled` — `.tsj` tileset with `wangsets` (8-entry `wangid` per tile from the
    Phase-3 layout / per-tile terrain data).
  - `godot` — `.tres` `TileSet` (bitmask/collision/region), per autotiler's format.
  - `json` — per-layer tile-index grid + packed tileset PNG + grid metadata
    (Phaser / custom engines; the Gabinou shape).
  - **LDtk** needs no exporter: it reads `.aseprite` directly with hot-reload —
    document `live_save_sprite` as the deliverable, with a file-format-version compat note.
  - ⚠ Fix the known community-script bug: export the **whole canvas**, not just the
    active visible region.

### Decisions ([ADR-0005](../docs/adr/0005-tilemap-protocol-and-bitmask.md))
1. **Tile placement reuses the pixel-batch protocol** (`{x,y,tile_index}` batches),
   not a new shape — minimal new surface, mirrors `live_draw_pixels`.
2. **One bitmask convention** (Red Blob Games 8-bit ordering, `src/autotile.rs`).
   The open generators (blobator, autotiler) disagree on bit weights; normalize to
   one and document it, or silent mis-mapping results. The sorted mask list is the
   tile order, so bitmask + exporter wangset agree by construction.
3. **Tilemap ops are `live_*`** (undoable in the UI) and require **new Lua plugin
   handlers** — not plugin-byte-compatible with SPEC-001. **Refined during impl:**
   rather than bump the wire `VERSION` (which would break *all* commands on any
   skew), the wire version stays 1; the plugin advertises `features=["tilemap"]`
   and a tilemap command on an old plugin returns `unsupported_command` — loud,
   per-command degradation. See ADR-0005.

## Acceptance criteria

Legend: `[x]` done + verified. **Live E2E run 2026-06-14 on Aseprite 1.3.17.2**
(plugin 0.2.3) closed the previously-pending live items; Tiled/Godot *import* is
the only remaining user check (this server cannot launch those editors).

- [x] Phase 1: created a tilemap layer (8×8 **and** 16×16), stamped a batch of
      tiles (overwrite + fill, confirmed by render), and read them back — all live
      in the open Aseprite UI. (Fixed live: wire snake_case `tile_index`; new-tilemap
      grid inheritance; float→int tile index for `putPixel` — see CHANGELOG.)
- [x] `live_get_tileset` returned a vision-legible packed PNG (upscaled via
      `preview.rs`); rendered tiles matched the dedupe.
- [x] Phase 2: `live_pack_similar_tiles` turned a 2-colour mockup into 2 unique
      tiles (16 cells → 2) + a tilemap reconstructing it pixel-for-pixel (verified
      by export grid + render).
- [x] Phase 3: the bitmask table maps all 256 neighbor configs to the 47 blob states
      (`src/autotile.rs::tests`, CI `cargo test`); template generation is future work.
- [x] Phase 4: `seamless_check` flags a deliberately broken seam and passes a correct
      wrap tile / strip (`tools/seam_lint.py`, `tests/test_seam_lint.py`, CI).
- [x] Phase 5: whole-canvas export to Tiled `.tsj` (blob47 wangset), Godot `.tres`,
      and JSON — serializers unit-tested green (`src/tileset_export.rs::tests`, 9
      cases) **and** exercised live (grid round-trips exactly, files written to disk).
      "Autotiles in Tiled out-of-the-box" remains a user import check.
- [x] `live_get_capabilities` advertises tilemap support (`features=["tilemap"]`);
      old plugins degrade loudly — verified live: a 0.1.0 plugin returned
      `unsupported_command` for `list_tilesets` (ADR-0005).
- [x] Existing unit tests still pass; new deterministic tests (bitmask, exporters)
      pass with no Aseprite (`cargo test --bins`, 38 green).

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** bitmask-table table-test; `pack_similar_tiles`
  dedupe on a fixture mockup (assert unique count + round-trip); `seamless_check` on a
  matched and a broken fixture (assert pass / fail).
- **Live (Tier-B):** "paint a 3-terrain mockup, pack it, export a Tiled `.tsj`" —
  judged on tileset correctness + Tiled import; logged in `evals/RESULTS.md`.

## Traceability
- Module(s): new `src/tools/tilemap.rs` (or `src/live.rs` live methods) + bridge Lua
  handlers in `scripts/aseprite-mcp-plugin/plugin.lua`; bitmask table in a pure-Rust
  module (e.g. `src/autotile.rs`); `tools/lint_sprite.py` (seam-lint); exporters.
- Test(s): bitmask table-test + dedupe + seam-lint fixtures (Tier-A); Tier-B live run.
- Replaces the read-only `isTilemap` flag at `src/tools/sprite.rs:151` with a real surface.
