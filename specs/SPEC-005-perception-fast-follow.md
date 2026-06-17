# SPEC-005 — Perception fast-follow (gutter + region-crop + inline image + Set-of-Mark)

- Status: Draft (2026-06-17). Roadmap item #1 ("Preview overhaul") fast-follow: the
  nearest-neighbor **upscale** already landed (`live_save_preview` + `src/preview.rs`,
  6 tests); this spec ships the **remaining three legs** of that item — a
  **coordinate gutter**, **cel-bbox region crop**, and **inline MCP-Image** return —
  plus **Set-of-Mark** numbered regions (§A line 270). Implement in phases; Phase 1
  (gutter) and the Phase-4 overlay compositor are pure Rust and land without a live
  Aseprite run.
- Owner: project
- Checklist items advanced: 1.x (perception/preview surface), 2.x (new live-tool
  options), 9.x (deterministic perception tests — gutter math, crop math, mark map)
- Related ADRs: ADR-0007 (proposed — inline-image content return + gutter/mark
  rendering conventions; see Behaviour §Decisions)
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

### Phase 2 — Region crop (cel bbox / explicit rect)
Render the budget on the **subject**, not the canvas. `crop="cel"` uses the active
cel's bounds (plugin returns `cel.bounds`); `crop="sprite"` = whole canvas (today);
`crop={x,y,w,h}` = explicit. Flow: plugin saves the faithful 1× (today's path) +
reports the crop rect → Rust crops the decoded PNG to the rect, *then*
`auto_preview_scale` picks the factor for the **crop's** long edge (so a 16×16 cel on
a 256×256 canvas fills ~1024px, not ~64px). The reported `PreviewInfo` gains
`crop_x, crop_y` so coordinates still map back exactly. Pure-Rust crop+scale is
unit-tested; the cel-bounds read is the only live piece.

### Phase 3 — Inline MCP-Image return
Add an `inline` option so `live_save_preview` (and `live_get_tileset` /
`live_save_filmstrip`, which already produce vision PNGs) can return the PNG as an
**MCP Image content block** (base64 + `image/png`) instead of only a path. This
changes the tool's return type from `String` to a content vector
(`Result<CallToolResult, _>` / `Vec<Content>`) — the first tool in the crate to emit
image content, hence the ADR. Keep the path in the text part for clients that prefer
it / for the auto-preview hook. Size-guard: skip inline (fall back to path + a note)
above a byte ceiling so a huge sheet can't blow the context budget.

### Phase 4 — Set-of-Mark numbered regions
Overlay numbered marks on regions and return a mark→region map. Region source
(`marks_from`): **slices** (named, authored — best), **layers** (one mark per visible
layer's cel bbox), or **components** (the linter's connected-component output, reused
from `tools/lint_sprite.py` / a pure-Rust CC pass). Each mark is a small numbered
badge at the region centroid in a neutral colour; the JSON returns
`[{n, region_name, bbox}]` so `pixel-critic` can say "region 3 (weapon) has a stray
pixel" and the orchestrator maps `3 → that slice/layer/cel` deterministically (§A
SoM). No SAM/ML — pixel art segments for free by slice/layer/component.

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
- [ ] Phase 1: gutter compositor is pure-Rust unit-tested — tick positions land on
      `gutter_step` source-px boundaries at the given scale; the coordinate-inversion
      identity (`preview_x → source_x`) holds for a table of (scale, step, x); the
      label colour is off-palette; oversized grids are refused with a clear error.
- [ ] Phase 2: crop+scale is unit-tested — a small cel on a big canvas crops to its
      bbox and scales so the crop's long edge ≈ target; `crop_x/crop_y` make the
      coordinate inversion exact; `crop="sprite"` reproduces today's output byte-for-
      byte (no regression).
- [ ] Phase 3: `inline=true` returns a valid `image/png` content block decodable back
      to the preview dimensions; over the byte ceiling it degrades to path + note;
      the schema-contract test covers the new param and the crate compiles clippy-clean.
- [ ] Phase 4: `marks_from=slices` on a sliced sprite returns one mark per slice with
      correct centroid + bbox and a mark→name map; badge colour is neutral; the
      mark→region inversion is exact (unit-tested on a synthetic layout).
- [ ] `live_get_capabilities` advertises the new capability
      (`features += ["perception2"]` or similar, plugin version bump); old plugins
      reject any new plugin command per-command (ADR-0005).

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** gutter tick-position + coordinate-inversion
  table; crop-bbox + scale math; SoM centroid/bbox + mark→region map; inline content
  round-trips to the right dimensions; `crop="sprite"` no-regression golden.
- **Live (Tier-B):** "there's a stray pixel near the sword" → with gutter+SoM the
  agent names "region 3, around (40,12)" and `live_draw_pixels` hits the right cell;
  graded on whether the named coordinate matches the actual defect cell (the gutter's
  whole point), logged in `evals/RESULTS.md`.

## Traceability
- Module(s): `src/preview.rs` (crop + gutter compositor; reuses `auto_preview_scale` /
  `render_preview` / nearest upscale) + a small bitmap-font/overlay helper; SoM
  centroid/CC math pure Rust (reuses `tools/lint_sprite.py` CC notion); `src/live.rs`
  live methods + `src/server.rs` `live_*` tools (Phase 3 changes one tool's return
  type to image content); `plugin.lua` gains a `cel.bounds` read for Phase 2 and a
  slice/layer enumeration for Phase 4 (both read-only). Pairs with `live_frame_diff`
  (PR #19) and `pixel-critic` for the see→locate→fix loop.
- Test(s): `src/preview.rs::tests` (gutter, crop, inversion), new SoM tests; live
  Tier-B coordinate-naming run judged on named-coord-matches-defect.
