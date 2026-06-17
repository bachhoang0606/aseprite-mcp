# ADR-0005 — Tilemap tool family: protocol reuse, capability gating, one bitmask convention

- Status: Accepted (2026-06-14)
- Context: [SPEC-003](../../specs/SPEC-003-tilemap-tool-family.md) (tilemap / tileset / autotile)
- Supersedes the "candidate ADR-0004" note in SPEC-003 (ADR-0004 was already taken
  by the modal-free preview decision; the tilemap decisions are ADR-0005).

## Context

SPEC-003 adds a tilemap/tileset/autotile tool family. Three structural decisions
were load-bearing enough to record, and one of them (the version/compat story)
was refined away from the spec's first draft during implementation.

## Decision

1. **Tile placement reuses the pixel-batch protocol shape.** A tilemap cel is an
   image whose "pixels" are tile indices (`Image:putPixel(col,row,index)`), so
   `live_stamp_tiles` takes `{x, y, tile_index}` batches — the tile-grid analogue
   of `live_draw_pixels` — rather than inventing a new wire shape. `x`/`y` are
   tile-grid cells (columns/rows), not pixels.

2. **One bitmask convention (Red Blob Games 8-bit, corner-masked).** Implemented
   in `src/autotile.rs`: edges in the low nibble (N=1,E=2,S=4,W=8), corners in the
   high nibble (NE=16,SE=32,SW=64,NW=128); a corner only counts when both its
   adjacent cardinals are filled. That collapses 256 raw configs to the 47 blob
   states, and the *sorted* mask list (`blob47_masks()`) **is** the tile order, so
   the bitmask, a future template generator, and the Tiled wangset exporter agree
   by construction. The open generators (blobator, autotiler) disagree on bit
   weights; we normalize to this one and document it so there is no silent
   mis-mapping. The Tiled `wangid` mapping lives in
   `src/tileset_export.rs::blob47_wangid` (`[top, topright, right, bottomright,
   bottom, bottomleft, left, topleft]`, 1 where the edge/corner is filled).

3. **Capability gating via a feature flag — NOT a breaking wire-version bump.**
   SPEC-003's draft said "bump the plugin protocol/version and gate via
   `live_get_capabilities`." Implementation refined this: the wire `VERSION` stays
   **1**. `handle_command` strict-rejects any frame whose `version` ≠ `VERSION`, so
   bumping it would make an *old* plugin reject **every** command against a new
   server (and vice-versa) — catastrophic, not graceful. Instead:
   - The plugin advertises `features = ["tilemap"]` in `get_capabilities` and bumps
     the cosmetic `PLUGIN_VERSION` to `0.2.0`.
   - A tilemap command sent to an **old** plugin hits the existing unknown-command
     path and returns `unsupported_command` — loud, **per-command** degradation,
     while every pre-existing command keeps working across plugin builds.

   This satisfies the spec's acceptance ("`live_get_capabilities` advertises
   tilemap support; old plugins degrade loudly") more surgically than a version
   bump would.

## Consequences

- New tilemap commands require new Lua handlers in `plugin.lua` (not
  byte-compatible with SPEC-001 plugins) — but old commands are unaffected, so a
  user on an old extension only loses *tilemap*, with a clear error, until they
  reinstall.
- The engine-format rules are pure Rust (`src/tileset_export.rs`), so Tiled/Godot/
  JSON serialization is unit-tested without Aseprite; only the data-fetch
  (`export_tilemap`) and the tile CRUD are live-bridge dependent.
- Clients should check `live_get_capabilities().features` for `"tilemap"` before
  offering tilemap workflows.

## Alternatives considered

- **Breaking version bump (spec draft):** rejected — see Decision 3; it breaks all
  commands on any version skew, not just the new family.
- **A separate tilemap wire protocol / second socket:** rejected — unnecessary
  surface; the batch shape + feature flag is enough.
