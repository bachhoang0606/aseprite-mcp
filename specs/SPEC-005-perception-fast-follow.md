# SPEC-005 — Perception fast-follow (gutter + region-crop + inline image + Set-of-Mark)

- Status: **All phases landed (2026-06-20)** — Phase 1: the gutter compositor
  (`src/gutter.rs`) is wired onto `live_save_preview` via the pure `live::finish_preview`
  (on by default, exact coordinate inversion). Phase 2: region crop (`crop`:
  `"sprite"`/`"cel"`/`{x,y,width,height}`) — crop-then-scale + gutter labels in
  absolute sprite coords; offline-verified, and **`crop="cel"` live-checked 2026-06-24** (executes +
  reads the cel bbox live; drawn cels are full-canvas so it equals sprite-crop — use explicit-rect
  crop for drawn content). Phase 3: `inline:true` returns the PNG as an MCP `image/png` content block
  (ADR-0007), path always present, byte-ceiling degrade. Phase 4: `marks_from`
  (`"slices"`/`"layers"`/`"components"`) overlays numbered Set-of-Mark badges + a
  `marks:[{n,region,bbox}]` map (`src/marks.rs`, pure CC + badge compositor). Phase 5:
  plugin bumped to 0.3.2 advertising `perception2`. Roadmap item #1
  ("Preview overhaul") fast-follow: the nearest-neighbor **upscale** already landed
  (`live_save_preview` + `src/preview.rs`); this spec ships the **remaining three
  legs** of that item — a **coordinate gutter**, **cel-bbox region crop**, and
  **inline MCP-Image** return — plus **Set-of-Mark** numbered regions (§A line 270).
  Implement in phases; Phase 1 (gutter) and the Phase-4 overlay compositor are pure
  Rust and land without a live Aseprite run.
- Owner: project
- Checklist items advanced: 1.x (perception/preview surface), 2.x (new live-tool
  options), 9.x (deterministic perception tests — gutter math, crop math, mark map)
- Related ADRs: [ADR-0007](../docs/adr/0007-inline-image-content.md) (Accepted
  2026-06-20 — inline-image content return: opt-in, path-always-present, byte-ceiling
  degrade, hand-rolled base64)
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  §A (VLMs-are-Blind: in-grid text labels ~double grid-geometry accuracy; AdaZoom /
  MEGA-GUI ~1000px grounding → crop the cel bbox first; SketchAgent coordinate
  margins; Set-of-Mark beats free-form coordinates; "one token = one pixel"),
  roadmap item **#1** ("nearest-neighbor upscale to ~1024px, **labeled 8-px gutter,
  return image inline (MCP Image content)**" — upscale done, "gutter / inline-image /
  region-crop are the fast-follow").

## Intent
Perception is the #1 lever (§A): every other capability multiplies after the agent
can *see and locate* its own work. The upscale fixed raw legibility, but three
documented gaps remain:

1. **No coordinates.** The agent can see a pixel is wrong but cannot name *which*
   (x,y) to fix — VLMs are blind to grid geometry, and **in-grid numeric labels
   roughly double** row/col accuracy (§A "VLMs are Blind"). Without a gutter the
   agent guesses coordinates and `live_draw_pixels` edits the wrong cell.
2. **The wrong thing fills the budget.** On a large or mostly-empty canvas the
   *subject* occupies a few hundred px even after upscale; grounding is most accurate
   when the **target** fills ~1000px (§A AdaZoom/MEGA-GUI) → crop to the cel bbox
   first, then upscale the crop.
3. **The image is out-of-band.** `live_save_preview` returns a file *path*; a vision
   client must be told to open it, and non-Claude-Code clients (Cursor/Codex via the
   same MCP server) often won't. Returning the PNG as an **inline MCP Image content
   block** puts the pixels in the model's context directly (roadmap #1 verbatim).

Plus **Set-of-Mark** (§A line 270): overlay *numbered* marks on regions (slices /
linter connected-components / layers) and let the critic say "region 3 has a stray
pixel"; the server maps mark→region deterministically, sidestepping the VLM's
coordinate weakness entirely.

## Inputs / Outputs
- **Inputs:** options on the preview surface — `gutter?: bool` (default on for
  sprites ≤ a size cap), `gutter_step?` (source-px between ticks, default 8),
  `crop?: "cel" | "sprite" | {x,y,w,h}` (default `sprite` = today's behaviour),
  `inline?: bool` (return the PNG as image content vs. a path), and for Set-of-Mark a
  `marks_from?: "slices" | "components" | "layers"`. The chosen integer upscale and
  crop origin are reported so preview pixels map back to **exact** sprite (x,y).
- **Outputs:** an upscaled PNG with an optional labelled gutter / numbered marks,
  returned **inline** (MCP Image content) or as a path; plus a JSON sidecar of
  `{source size, crop origin, scale, gutter_step, marks: [{n, region, bbox}]}` so the
  orchestrator can translate any mark or (x,y) the critic names back to a real
  layer/cel/coordinate. All image math is pure Rust (no Aseprite) → unit-testable.

## Behaviour

Implement in **phases** (each independently shippable):

### Phase 1 — Coordinate gutter (`src/preview.rs` / new `gutter` compositor)
Pure Rust, fully unit-tested (mirrors `preview.rs` / `ascii_view.rs`): given the
upscaled RGBA buffer + the integer `scale` + `gutter_step`, composite a margin with
**chunky** numeric ticks every `gutter_step` source-px along the top and left (§A:
use 8-px guides, never 1-px hairlines). Labels are source-space coordinates (0, 8,
16…), drawn with a tiny built-in bitmap font (no font dependency) in a neutral colour
chosen to avoid collision with sprite pixels (§A ClaudePlaysPokemon: a red marker on
red pixels confused the model → pick the gutter/label colour off the sprite's own
palette, or use a fixed neutral on a separate margin band so it never overlaps art).
Because the scale is integer and the crop origin known, `preview_x → source_x =
crop_x + (preview_x - gutter_w) / scale` is **exact**. Refuse a gutter when the label
density would be unreadable (cap like `ascii_view`'s 64-edge) and say so.

**Phase 1 — implemented (2026-06-20).** Wired onto `live_save_preview`:
`save_preview` renders to an in-memory buffer (`preview::render_preview_buffer`) and
hands it to the pure `live::finish_preview`, which composites the gutter and writes
the PNG. Decisions made during wiring:
- *Default-on is legibility-gated, not raw-size-gated.* "Default on for sprites ≤ a
  size cap" is implemented as "default on whenever the tick spacing is legible at the
  chosen scale"; the `render_with_gutter` floor *is* the cap. This is exact (the floor
  already accounts for label-box width/height so multi-digit labels can't overlap) and
  needs no separately-tuned size constant. `gutter:true` makes an illegible request a
  loud `gutter_unreadable` refusal; the default degrades to a plain preview with a
  `gutter_skipped` note and `gutter_applied:false`.
- *Sidecar contract.* The result reports `gutter_applied` (bool), and when applied a
  `gutter:{left_w, top_h, step, image:{w,h}}` object; `preview:{w,h}` stays the bare
  upscaled art, `gutter.image` is the on-disk (gutter'd) size. Inversion: when applied,
  `source = (preview − {left_w,top_h}) / scale`; else `source = preview / scale`.
- *Label colour* is steered off the sprite's own sampled colours
  (`gutter::sprite_palette`, distinct opaque colours, one sample per source cell).
- *Deferred (carried by review 2026-06-20):* (a) `live_get_capabilities` capability
  advertisement is held until Phases 2–4 land — Phase 1 adds **no plugin command**, so
  there is nothing version-gated to advertise (see Acceptance gate below); (b)
  `render_with_gutter` re-derives source dims as `pw/scale` rather than taking them
  from `PreviewInfo` — exact in Phase 1 (`pw = source·scale`), but **Phase 2 must pass
  the crop's true source dims in** once a non-evenly-divisible crop is possible.

### Phase 2 — Region crop (cel bbox / explicit rect)
Render the budget on the **subject**, not the canvas. `crop="cel"` uses the active
cel's bounds (plugin returns `cel.bounds`); `crop="sprite"` = whole canvas (today);
`crop={x,y,w,h}` = explicit. Flow: plugin saves the faithful 1× (today's path) +
reports the crop rect → Rust crops the decoded PNG to the rect, *then*
`auto_preview_scale` picks the factor for the **crop's** long edge (so a 16×16 cel on
a 256×256 canvas fills ~1024px, not ~64px). The reported `PreviewInfo` gains
`crop_x, crop_y` so coordinates still map back exactly. Pure-Rust crop+scale is
unit-tested; the cel-bounds read is the only live piece.

**Phase 2 — implemented (2026-06-20).** `crop?: "sprite" | "cel" | {x,y,width,height}`
on `LiveSavePreviewParams`. Flow: `save_preview` calls the plugin once (the response
now carries the active cel's `bounds`), then `resolve_crop_plan` validates the selector
and `render_preview_buffer(src, scale, crop)` clamps the rect, crops the decoded RGBA,
and auto-scales on the **crop's** long edge; `PreviewInfo` gains `crop_x/crop_y`. The
gutter draws labels in **absolute** sprite coords via `gutter::render_with_gutter_at`
(`origin = crop`), so the agent reads the real (x,y) with no arithmetic, and the sidecar
adds `crop:{x,y}`. Decisions:
- *`crop="cel"` resolves from the plugin's reported `cel.bounds`* (new read-only field in
  `handle_save_preview`); a negative cel origin is clamped to the canvas. Absent bounds —
  an empty active layer/frame, or an **old plugin** that predates the field — is a loud
  `cel_bounds_unavailable` degrade (ADR-0005), never a silent guess. This resolves the
  Phase-1 deferral (b): `render_with_gutter_at` still derives src dims as `pw/scale`, but
  that stays exact because the crop is a whole-pixel rect upscaled by an integer factor.
- *`crop="sprite"` / full-canvas rect short-circuits to the uncropped buffer*, so the
  default path is byte-for-byte today's output (no-regression unit test).
- *Explicit-rect validation is pure* (`resolve_crop_plan` / `rect_to_crop` /
  `cel_crop_from_response`) and unit-tested (modes, negative/zero rects, cel clamp, old-
  plugin degrade); the cel-bounds *read* is the only live piece, pending a plugin-reload
  verify.
- *Capability advertisement (`perception2`) still deferred to Phases 3–4* — Phase 2's
  plugin change is an additive read-only field with built-in old-plugin degradation, not
  a new command, so nothing yet requires a version gate.

### Phase 3 — Inline MCP-Image return
Add an `inline` option so `live_save_preview` (and `live_get_tileset` /
`live_save_filmstrip`, which already produce vision PNGs) can return the PNG as an
**MCP Image content block** (base64 + `image/png`) instead of only a path. This
changes the tool's return type from `String` to a content vector
(`Result<CallToolResult, _>` / `Vec<Content>`) — the first tool in the crate to emit
image content, hence the ADR. Keep the path in the text part for clients that prefer
it / for the auto-preview hook. Size-guard: skip inline (fall back to path + a note)
above a byte ceiling so a huge sheet can't blow the context budget.

**Phase 3 — implemented (2026-06-20, [ADR-0007](../docs/adr/0007-inline-image-content.md)).**
`inline?: bool` on `LiveSavePreviewParams`. `live_save_preview` now returns
`Result<CallToolResult, McpError>` (the first image-emitting tool); the no-inline wire
shape (one text block with the JSON sidecar) is byte-identical to before, so the
auto-preview hook and non-vision clients are unaffected. Decisions:
- *Pure assembly seam.* `LiveBridge::save_preview` still returns the JSON string; the
  image concern lives at the server boundary (`build_preview_call_result`), and the
  file-read + size-guard + encode is the pure `preview::read_inline_png` →
  `InlinePng::{Ready(base64), TooLarge(bytes)}`, unit-tested without the bridge.
- *Byte-ceiling degrade.* Over `INLINE_MAX_BYTES` (1 MiB PNG ≈ ~1.4 MiB base64) the
  result appends a text note (size + path) instead of the image; a read error degrades
  the same way. The path is always present — never a silent truncation.
- *Base64 is hand-rolled* (`preview::base64_encode`, pinned to RFC 4648 vectors), not a
  new dependency — keeps the dep tree lean and avoids a Windows-SAC relink block on the
  test binary (adding a crate to `Cargo.toml` trips os error 4551; a code-only rebuild
  does not).
- *Deferred:* `live_get_tileset` / `live_save_filmstrip` can reuse `read_inline_png` +
  `build_preview_call_result` verbatim, but the acceptance scope is `live_save_preview`.

### Phase 4 — Set-of-Mark numbered regions
Overlay numbered marks on regions and return a mark→region map. Region source
(`marks_from`): **slices** (named, authored — best), **layers** (one mark per visible
layer's cel bbox), or **components** (the linter's connected-component output, reused
from `tools/lint_sprite.py` / a pure-Rust CC pass). Each mark is a small numbered
badge at the region centroid in a neutral colour; the JSON returns
`[{n, region_name, bbox}]` so `pixel-critic` can say "region 3 (weapon) has a stray
pixel" and the orchestrator maps `3 → that slice/layer/cel` deterministically (§A
SoM). No SAM/ML — pixel art segments for free by slice/layer/component.

**Phase 4 — implemented (2026-06-20).** `marks_from?: "slices"|"layers"|"components"` on
`LiveSavePreviewParams`. New pure module `src/marks.rs`: `connected_components` (4-connected
flood fill over opaque pixels, mirroring `tools/lint_sprite.py`'s opacity + 4-neighbour
notion — the orphan check is the size-1 case), `assign_marks` (numbers regions 1..N; the
inverse is `marks[n-1]`), and `draw_badge` (a numbered badge over a neutral backing box,
clamped on-canvas), reusing the one shared bitmap font from `gutter.rs`. Decisions:
- *No new plugin command.* `slices` reuses `list_slices` (has `bounds`); `layers` reuses
  `list_cels` (position + image size = cel bbox) ∩ `list_layers` (visibility); `components`
  is pure Rust on the rendered buffer. Pure parse helpers `parse_slice_regions` /
  `parse_layer_regions` / `visible_layer_names` are unit-tested.
- *Effective visibility, not the local flag.* `visible_layer_names` ANDs each layer's
  `isVisible` with its ancestor groups' (a layer inside a hidden group is not rendered, so
  it is not marked) and returns `Option` — `None` (visibility unparseable) declines to
  filter, `Some(∅)` (everything hidden) genuinely emits no layer marks. Duplicate layer
  names get disambiguated output names (`Body`, `Body #2`) so the map stays 1:1.
- *Marks compose after the gutter, in absolute source coords.* `finish_preview` filters
  regions to the crop window THEN numbers them (so every emitted mark has a visible badge
  and numbering is contiguous — a slice outside a cel crop gets neither badge nor orphan
  number), draws each badge at `band + (centroid − crop)·scale`, and returns
  `marks:[{n, region, bbox(source)}]` (present even when empty: "requested, none found").
  Components run CC at SOURCE resolution — reconstructed from the bare pre-gutter buffer by
  sampling one px per upscale block (`downsample_by_scale`, exact), so CC never touches the
  up-to-67M-px upscaled buffer and bboxes are source-space directly.
- *Badge colour + count.* Badges reuse `gutter::pick_label_color` (off-palette) over a
  `BAND_BG` box so they read on any art (§A red-on-red); a `MAX_MARKS` (64) cap keeps the
  largest regions and reports the true total in `marks_truncated` so a noisy sprite can't
  bury the art in overlapping badges.

**Phase 5 — capability advertisement (2026-06-20).** Plugin bumped `0.3.1 → 0.3.2` and
`FEATURES += "perception2"`. Across all four phases the *only* plugin change is the Phase-2
`cel.bounds` field in `save_preview`; the gutter / crop-math / inline / marks features are
server-side and need no plugin support. So `perception2` advertises exactly "this plugin
reports `cel.bounds`, so `crop="cel"` works" — a client checks the one flag; everything else
degrades loudly on an old plugin (ADR-0005) rather than being gated.

### Decisions (candidate ADR-0007)
1. **Inline image is opt-in, path always present.** Default stays path-returning (the
   auto-preview hook and non-vision clients rely on it); `inline=true` adds an image
   content block. Above a byte ceiling, inline silently degrades to path + note —
   never silently truncate or blow context.
2. **Integer-scale + known crop origin = exact coordinate inversion.** The gutter and
   any (x,y) the critic names invert to real sprite coords with integer math; never
   ship a non-integer scale on the labelled path.
3. **Neutral, palette-aware annotation colour.** Gutter labels and SoM badges pick a
   colour absent from (or maximally distant in LAB from) the sprite's palette, or live
   in a separate margin band, so annotations never read as art (§A red-on-red).
4. **Marks come from existing structure** (slice/layer/linter-CC), not a new
   segmenter — deterministic, explainable, and round-trippable to a real object.

### Out of scope (future)
- **Before/after checkerboard composite** (§A "VLM resolution curse") — pairs with
  `live_frame_diff` (PR #19) and the critic loop; a small follow-up, not this spec.
- **Per-model A/B of gutter vs. plain preview** (§A SketchAgent "let the eval harness
  pick the winner per model") — belongs in the eval roadmap item #9, not here.
- **Onion-skin / multi-cel composites** — animation perception, separate spec.

## Acceptance criteria
- [x] Phase 1: gutter compositor is pure-Rust unit-tested — tick positions land on
      `gutter_step` source-px boundaries at the given scale; the coordinate-inversion
      identity (`preview_x → source_x`) holds for a table of (scale, step, x); the
      label colour is off-palette; oversized grids are refused with a clear error.
      **Wiring (2026-06-20):** `live_save_preview` gains `gutter`/`gutter_step`; the
      pure `live::finish_preview` composites-or-degrades and is unit-tested (default
      legible draw, default degrade, explicit require success + refusal, `gutter:false`
      bare, fully-transparent art, write-failure); the legibility floor also rejects
      multi-digit label overlap. 81 unit tests pass; clippy adds no new lints.
- [x] Phase 2: crop+scale is unit-tested — a small region on a big canvas crops to its
      rect and scales so the crop's long edge ≈ target; `crop_x/crop_y` + absolute-coord
      gutter labels make the inversion exact; `crop="sprite"`/full-rect reproduces today's
      output byte-for-byte (no-regression test). `crop` modes + rect validation + cel-
      bounds parsing/clamp/old-plugin degrade are unit-tested (`resolve_crop_plan`,
      `rect_to_crop`, `cel_crop_from_response`, `clamp_crop`). 87 unit tests pass; clippy
      adds no new lints. **`crop="cel"` live-checked 2026-06-24** (`evals/runs/2026-06-24/live_verify.json`):
      the cel-bbox read works live; drawn cels are full-canvas so cel-crop == sprite-crop (documented).
- [x] Phase 3: `inline=true` returns a valid `image/png` content block — `read_inline_png`
      base64s the PNG (`base64_encode` pinned to RFC 4648 vectors; the round-trip test
      decodes back to the preview dimensions); over `INLINE_MAX_BYTES` it degrades to
      `TooLarge` → path + note; the schema-contract test covers `inline` (in
      `LiveSavePreviewParams`) and the crate is clippy-clean. `live_save_preview` returns
      `Result<CallToolResult, McpError>` ([ADR-0007](../docs/adr/0007-inline-image-content.md)).
      89 unit tests pass.
- [x] Phase 4: `marks_from` returns one mark per region (`slices`/`layers`/`components`)
      with correct centroid + bbox and a mark→name map; badge colour is neutral
      (`pick_label_color` over `BAND_BG`); the mark→region inversion is exact. Unit-tested
      on synthetic layouts: `connected_components` (disjoint blobs, 4-connectivity, L-merge,
      empty), `assign_marks` numbering + inversion, `draw_badge` in-bounds + clamp, the
      `parse_slice_regions`/`parse_layer_regions`/`buffer_rect_to_source` helpers, and
      `finish_preview` marks-in-JSON for both components and crop-window-filtered regions.
      No new plugin command (reuses `list_slices`/`list_cels`/`list_layers`). 103 unit tests
      pass; clippy adds no new lints. **Live-verify pending a plugin reload.**
- [x] `live_get_capabilities` advertises the new capability — plugin bumped `0.3.1 → 0.3.2`,
      `FEATURES += "perception2"`. Across all phases the only plugin change is the Phase-2
      `cel.bounds` field, so `perception2` means "`crop="cel"` works"; the rest is server-
      side and degrades loudly on an old plugin (ADR-0005), with nothing new to gate.

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** gutter tick-position + coordinate-inversion
  table; crop-bbox + scale math; SoM centroid/bbox + mark→region map; inline content
  round-trips to the right dimensions; `crop="sprite"` no-regression golden.
- **Live (Tier-B):** "there's a stray pixel near the sword" → with gutter+SoM the
  agent names "region 3, around (40,12)" and `live_draw_pixels` hits the right cell;
  graded on whether the named coordinate matches the actual defect cell (the gutter's
  whole point), logged in `evals/RESULTS.md`.

## Traceability
- Module(s): `src/preview.rs` (crop + nearest upscale + inline-PNG helper), `src/gutter.rs`
  (gutter compositor + shared bitmap font), `src/marks.rs` (SoM centroid/CC math, reuses
  `tools/lint_sprite.py`'s opacity + 4-neighbour CC notion); `src/live.rs` live methods +
  `src/server.rs` `live_*` tools (Phase 3 changes `live_save_preview`'s return type to image
  content). `plugin.lua` gains exactly one read-only field across SPEC-005 — the active
  cel's `bounds` in `save_preview` (Phase 2, for `crop="cel"`); **Phase-4 marks add NO new
  plugin command** (they reuse the existing `list_slices` / `list_cels` / `list_layers`
  reads). Pairs with `live_frame_diff` and `pixel-critic` for the see→locate→fix loop.
- Test(s): `src/preview.rs::tests` (scale/crop/inline), `src/gutter.rs::tests` (gutter,
  inversion, label floor), `src/marks.rs::tests` (CC, numbering, badge), `src/live.rs::tests`
  (`finish_preview` gutter/crop/marks wiring, region/visibility parsing); live Tier-B
  coordinate-naming run judged on named-coord-matches-defect.
