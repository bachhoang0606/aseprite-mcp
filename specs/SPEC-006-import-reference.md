# SPEC-006 — import_reference (reference image → palette-locked pixel art, live)

- Status: **Phase 1 landed (2026-06-21)** — `live_import_reference` ships a pure-Rust
  **content-aware downscale + palette snap** (`src/reference.rs`) drawn live via the
  existing `draw_pixels` path (no new plugin command, no new dependency). Phase 2 (Sobel
  grid auto-detect / imagequant auto-palette / non-PNG input) is deferred. Roadmap item
  **#6** ("import_reference") — the unlock for the hybrid generation / reference pipeline
  (Path 3/4, research §C2).
- Owner: project
- Checklist items advanced: 2.x (new live-tool surface), 9.x (deterministic image
  tests — downscale + snap math)
- Related ADRs: ADR-0005 (loud per-command degradation); reuses ADR-0004 preview render
  path conventions. No new ADR (no new return-type / protocol contract).
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  Path 3 (hybrid generation), Path 4 (reference grounding), §C2 (regrid "fake" AI pixel
  art: downscale + remap; the Sobel-profile grid detection is the harder follow-up),
  roadmap **#6**.

## Intent
The agent's hardest weakness is *inventing* organic shapes from text (§ Path 3/4). The
fix is to start from a **reference** — a photo, an illustration, an AI-generated image,
a CC0 asset — and convert it into clean, palette-locked pixel art the agent then refines.
That conversion is two deterministic steps:

1. **Downscale to the target pixel grid.** A reference is high-res (or "fake" pixel art
   that is really 64×64 rendered at 1024px). A naive bilinear shrink blurs edges and
   invents colours; pixel art needs a **content-aware** downscale that keeps hard edges
   and introduces no new colours.
2. **Snap to a curated palette.** Remap every output cell to the nearest colour in the
   sprite's locked palette (CIELAB ΔE, the real metric — `color_ops`), so the import is
   immediately on-model and the linter passes.

Done live, this drops a converted reference straight onto a layer in the open sprite, so
the agent can trace/clean over it — the "missing middle step" that makes imported or
generated assets look native (§C2).

## Inputs / Outputs
- **Inputs (`live_import_reference`):** `filename` (a **PNG** reference on disk),
  `width?`/`height?` (target grid — default the active sprite's size), `method?`
  (`"dominant"` = per-cell palette-majority, default; `"average"` = per-cell mean),
  `palette?` (explicit `#rrggbb` list to snap to; default = the active sprite palette;
  `snap:false` skips snapping), `layer?` (target layer, default `"Reference"`),
  `frame?`, and `at_x?`/`at_y?` (top-left placement on the canvas, default 0,0).
- **Outputs:** the converted pixels drawn onto the target layer in the open sprite
  (via the existing `draw_pixels` path — no new plugin command), plus a JSON summary of
  `{source size, target size, scale-down factor, method, palette size, pixels drawn,
  distinct colours}`. All image math is pure Rust (no Aseprite) → unit-testable.

## Behaviour

### Phase 1 — content-aware downscale + palette snap (this build)
Pure Rust in a new `src/reference.rs` (mirrors `preview.rs` / `gutter.rs`):
`downscale_to_grid(src: &RgbaImage, tw, th, palette: Option<&[Rgba]>, method) -> RgbaImage`.

Each output cell `(ox, oy)` maps to the source block
`x ∈ [ox·sw/tw, (ox+1)·sw/tw)`, `y ∈ [oy·sh/th, (oy+1)·sh/th)` (integer area mapping in
`u64`, each block forced to ≥1 px). Per cell:
- **`dominant` (default):** tally a vote per source pixel — a fully-transparent pixel
  votes "transparent", else it votes its **nearest palette index** (or its raw colour
  when no palette). The cell takes the majority (ties → lowest palette index, for
  determinism). This is edge-preserving (a majority vote, never an average that bleeds a
  new mixed colour) and palette-locked in one pass — the K-Centroid idea, fused with the
  remap.
- **`average`:** the mean of the cell's opaque pixels, then snapped to the palette if one
  is given. Simpler; good for soft gradients. A cell that is majority-transparent is
  output transparent under either method.

`live::import_reference` then: decode the PNG (`image`, PNG-only — see Decisions), resolve
the target dims (params or `live_get_sprite_info`), resolve the palette (explicit list /
active palette via `list_palette` / none), run the core, convert the non-transparent
output cells to a `draw_pixels` batch offset by `at_x/at_y`, and draw onto the target
layer. Bound the target long edge (≤ 256) so the batch can't explode, and refuse a
zero/oversized source clearly.

### Phase 2 — grid auto-detect + auto-palette (deferred; spec only)
- **Sobel-profile regrid (de-fake).** When the reference is *off-grid* AI pixel art
  (1024px that is "really" 64×64), detect the hidden native resolution instead of being
  told it: Sobel edge profiles per row/column → histogram-vote the dominant cell spacing
  → snap → quantize (the unfake.js method, research §C2 — corrected from "autocorrelation").
  Lets `width/height` be omitted for that class of input.
- **Auto-palette.** When no palette is given and snapping is wanted, reduce the source to
  N colours (median-cut / k-means — `tools/extract_palette.py` already does this offline;
  a native port or `imagequant` would do it live). Deferred because `imagequant` is a new
  crate dependency (weigh against the lean-deps / Windows-SAC relink cost).
- **Non-PNG input.** Enabling JPEG/WebP in the `image` crate is a feature-flag (dependency)
  change with the same SAC/relink cost; deferred — v1 documents "convert to PNG first".

### Alternative no-server-change routes (documented, not built)
For users who prefer them and have the tools, the same v1 result is reachable without
this tool: `magick ref.png -resize WxH -dither None -remap palette.png out.png` (ImageMagick),
or the pure-Lua **K-Centroid** Aseprite script via the gated `run_lua_script`
(`ASEPRITE_MCP_ALLOW_LUA=1`). `live_import_reference` is the batteries-included, no-external-
dep, deterministic-and-tested path.

### Decisions
1. **Pure-Rust deterministic core, reusing `color_ops`.** The downscale + snap is the
   `color_ops` CIELAB metric (`clamp_to_palette` / `nearest_palette_index`) over a
   majority-vote downsample — no new dependency, unit-testable without Aseprite. (The
   research's "v1 = magick/Lua skill" is offered as an alternative route, but a native tool
   is more robust and gate-free, so it is the primary deliverable.)
2. **Draw live via the existing `draw_pixels` path — no new plugin command.** A ≤256-edge
   import is one bounded batch; a dedicated `load_image_to_cel` plugin command (more
   efficient for large imports) is a Phase-2 nicety, not required.
3. **Default snap to the ACTIVE palette.** The most useful default makes the import
   immediately on-model; an explicit `palette` overrides, `snap:false` keeps source colours
   (raw downscale). No palette + no snap = a plain content-aware shrink.
4. **PNG-only input in v1** (the `image` crate is PNG-only by design); document "convert to
   PNG first" rather than pay the dependency/relink cost to add decoders now.

## Acceptance criteria
- [x] Phase 1: `downscale_to_grid` is pure-Rust unit-tested — a synthetic 2-colour image
      downscales to the expected grid; `dominant` preserves a hard edge (no invented mixed
      colour) and outputs only palette colours; a majority-transparent cell is transparent;
      `average` returns the cell mean; non-integer source:target ratios cover every cell
      (≥1 source px each, `cell_span` test); output is exactly `tw×th`. (`src/reference.rs`,
      7 tests.)
- [x] `live_import_reference` validates inputs (missing filename, bad method, zero/oversized
      target, too-large source → clear errors), resolves target dims (params or active
      sprite) + palette (explicit list / active palette / `snap:false`), and draws the
      converted cells onto the target layer via `draw_pixels`; the JSON summary reports
      source/target size, factor, method, palette size, pixels drawn, distinct colours. Pure
      helpers `validate_target_dims` / `parse_hex_palette` / `parse_palette_colors` /
      `grid_to_pixels` unit-tested (4). Schema-contract test covers `LiveImportReferenceParams`;
      crate is clippy-clean. A source-size guard (`MAX_SOURCE_EDGE`) reads dimensions before
      the full decode so a pathological PNG can't OOM.
- [x] No new dependency; `cargo test --bins` runnable locally (no SAC relink). 83 tests pass.
      **Live-verify pending an Aseprite run** (decode → downscale → `draw_pixels` onto a layer).

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** the `downscale_to_grid` table above
  (edge-preserve, palette-lock, transparency, area mapping, average mean); a golden
  small-image → grid fixture.
- **Live (Tier-B):** "import `hero_ref.png` as a 48×48 reference on the goblin palette" →
  a `Reference` layer carries a recognisable, on-palette downscale; the agent then
  traces/cleans over it and `/pixel-review` passes — graded on on-palette (linter) +
  silhouette recognisability.

## Traceability
- Module(s): `src/reference.rs` (pure downscale + snap core; reuses `color_ops`
  CIELAB), `src/live.rs` `import_reference` (decode + resolve + draw via `draw_pixels`),
  `src/server.rs` `live_import_reference` tool. No `plugin.lua` change (reuses
  `draw_pixels` / `list_palette` / `get_sprite_info`). Pairs with `/pixel-palette` (lock a
  palette first) and the future `pixel-reference-motion` skill (roadmap #7).
- Test(s): `src/reference.rs::tests` (downscale/snap/transparency/area), `live.rs`
  param-validation + summary tests; live Tier-B import-and-review run.
