# SPEC-004 — Constrained / semantic drawing (palette-snap + adjust-pixels)

- Status: Draft (2026-06-14). **Phases 1–4 implemented.** Phase 1 = `src/color_ops.rs`
  (pure CIELAB/CIEDE2000 + semantic ops + colour-map), 11 unit tests green. Phases
  2–4 = live tools `live_palette_snap` / `live_adjust_pixels` / `live_snap_colors`
  (`src/live.rs` + `src/server.rs`) + plugin handlers `get_region_colors` /
  `apply_color_map` (RGB only, v1); compile-green + Lua-parse-clean. **Live E2E
  pending an Aseprite run** (`[~]` items below).
- Owner: project
- Checklist items advanced: 2.x (new live tool surface), 7.3 (snap makes off-ramp
  colors impossible → the sprite linter goes from catcher to non-issue), 9.4
  (deterministic color-op tests)
- Related ADRs: ADR-0006 (proposed — CIELAB metric + per-color-map architecture +
  hue-shift convention; see Behaviour §Decisions)
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  §B (real-LAB `palette_snap`), §D (Magic Pencil semantic paint ops; pixel-mcp's
  fake LAB / dead 8-light-dir), roadmap item #4 (Path 2 — constrained drawing).

## Intent
The #1 hand-drawing failure is the agent picking colors *by eye* — off-palette
strokes, value-only ramps, muddy shades it computed slightly wrong. This feature
makes every colour operation **legal by construction**: a **real CIELAB palette
snap**, plus **intent ops** (darken / lighten / hue-shift / colorize) that bake in
the project's hue-shift rule and clamp to the palette. It implements **Path 2**
(constrained drawing): it turns the sprite linter from a catcher into a non-issue,
and it *honestly* surpasses the competitor **pixel-mcp**, whose "LAB snapping" is
plain RGBA squared-euclidean in its own generated Lua and whose "8 light
directions" compute a light vector that is never used (§D). The win: the agent
stops doing arithmetic on hex and instead expresses *intent* — "shade this darker"
— and the tool guarantees a palette-legal, rule-compliant result.

## Inputs / Outputs
- **Inputs:** new `live_*` tool params — target scope (`layer?`, `frame?`,
  `selection_only?`), op enum + `amount` for adjust, optional `hue` (colorize),
  optional `clamp_to_palette`. The active sprite's palette is read live
  (`live_list_palette`). A pure helper also accepts a bare list of hex colours.
- **Outputs:** live in-place pixel edits in the open Aseprite UI — undoable, **one
  `app.transaction` per call** — plus a JSON report of the colour→colour mapping
  applied and the pixel/colour counts. All colour math is pure Rust (no Aseprite),
  so every number is unit-testable.

## Behaviour

Implement in **phases** (each independently shippable):

### Phase 1 — Pure colour-ops core (`src/color_ops.rs`)
Deterministic, no Aseprite, fully unit-tested (mirrors `autotile.rs` /
`tileset_export.rs` / `ascii_view.rs`):
- sRGB ↔ CIELAB conversion (D65); ΔE distance — **CIEDE2000 preferred** (the
  perceptual metric "real LAB" means), CIE76 acceptable for v1.
- `nearest_palette_index(rgba, palette) -> usize` by minimum ΔE — the **real LAB
  snap**. Alpha is preserved; a fully-transparent pixel passes through untouched.
- Semantic per-colour ops, each `rgba -> rgba`:
  - `darken(c, amount)` / `lighten(c, amount)` — shift HSV value by `amount` **and**
    rotate hue per the project's hue-shift rule (`rules/` palette/shading): darken →
    toward blue/purple (cooler), lighten → toward yellow/orange (warmer). The result
    is a hue-shifted *ramp step*, not a flat value change.
  - `hue_shift(c, degrees)` — rotate hue.
  - `colorize(c, target_hue)` — set hue to target, keep value (Magic Pencil: average
    saturation).
- `clamp_to_palette(result, palette)` — snap any op result to the nearest LAB
  palette colour → legal by construction.
- `build_color_map(unique_colors, transform, palette) -> [(from, to)]` — the
  orchestration primitive. **Both tools are per-colour transforms** (a given input
  colour always maps to one output), so only a region's *unique* colours need cross
  the wire, never per-pixel data.

### Phase 2 — `live_palette_snap`
Snap every off-palette colour in the target region to its nearest LAB palette
colour. Flow: plugin `get_region_colors(layer, frame, selection_only)` returns the
region's unique colours + the palette → Rust `nearest_palette_index` builds the map
→ plugin `apply_color_map` replaces each colour inside one `app.transaction`.
Returns the mapping + counts (`"7 off-palette colours snapped, 312 pixels
recoloured"`). **Idempotent** — a snapped sprite snaps to itself.

### Phase 3 — `live_adjust_pixels`
`op ∈ {darken, lighten, hue_shift, colorize}`, `amount`, optional `hue` (colorize),
optional `clamp_to_palette` (default **true**). Same get-colours → build-map →
apply-map flow over the region's unique colours, scoped to layer / frame /
selection. Shading **by intent**, palette-legal — directly attacks the #1 failure
(agent hand-computing slightly-wrong shades) and bakes the hue-shift rule into the
tool so ramps come out cool-shadow / warm-highlight automatically.

### Phase 4 — snap-on-draw helper (`live_snap_colors`)
A lightweight **pure** helper (no live edit): given a list of hex colours, return
each snapped to the active palette — so the agent can legalize a stroke's colours
**before** `live_draw_pixels`. Makes "every stroke legal by construction" (§B) a
one-call habit, and works for non-vision clients too.

### Decisions (candidate ADR-0006)
1. **Colour metric is CIELAB ΔE** (CIEDE2000 preferred, CIE76 acceptable v1), **not
   RGBA euclidean** — the honest version of the competitor's claimed feature (§D).
2. **Per-colour-map architecture:** tools fetch only a region's *unique* colours,
   compute a colour→colour map in pure Rust, and apply it in one plugin replace
   pass. Keeps the math testable, the wire payload tiny (no per-pixel transfer), and
   reuses replace-colour plumbing. (Avoids needing a live full-pixel readback.)
3. **Hue-shift convention** encoded from the project rules: darken → cooler,
   lighten → warmer, so semantic ops produce rule-compliant ramps by default.
4. **`clamp_to_palette` defaults ON** for adjust ops (legal-by-construction); pass
   `false` to free-shade an open/expanding palette.

### Out of scope (future)
- **Real light-direction shading** (the other half of §D's pixel-mcp critique):
  needs form/surface-normal estimation and pairs naturally with the SPEC-011-style
  `StyleProfile.light_dir`. Tracked as a follow-up, not this spec.
- New palette *generation* (palette-smith already covers that).

## Acceptance criteria
- [x] Phase 1: sRGB↔LAB, ΔE, `nearest_palette_index`, and each op are unit-tested
      (`src/color_ops.rs::tests`, 11 cases) with **no Aseprite** — incl. CIEDE2000
      validated vs the Sharma reference pairs, a brute-forced **"LAB nearest ≠ RGBA
      nearest"** proof, and the darken-cools / lighten-warms direction. CI-green.
- [~] Phase 2: `live_palette_snap` recolours an off-palette region to palette-only,
      in one undo step, and is idempotent — verified by `tools/lint_sprite.py`
      reporting **0 off-palette** afterwards. *Code-complete (plugin
      `get_region_colors`/`apply_color_map` + Rust); **live E2E pending**.*
- [~] Phase 3: `live_adjust_pixels(op=darken)` on a flat fill yields a darker,
      hue-shifted, **palette-legal** colour; `clamp_to_palette=false` yields the raw
      shade (off-palette allowed). *Code-complete; **live E2E pending**.*
- [~] Phase 4: `live_snap_colors` returns palette-legal hex for arbitrary inputs.
      *Code-complete; **live E2E pending**.*
- [x] `live_get_capabilities` advertises the new capability
      (`features += ["color_ops"]`, plugin 0.3.0); the new param structs are in the
      schema-contract test and the crate compiles clean (clippy-clean).

## Eval (how we grade it)
- **Deterministic (Tier-A, no Aseprite):** colour-ops table tests — the LAB-vs-RGBA
  divergence fixture; darken/lighten hue direction; `clamp_to_palette` idempotency;
  transparent pass-through.
- **Live (Tier-B):** "shade this flat goblin: darken the shadow side, lighten the
  rim" → judged on palette-legality (linter 0 off-ramp) + hue-shift presence
  (shadow cooler, highlight warmer); logged in `evals/RESULTS.md`.

## Traceability
- Module(s): new pure `src/color_ops.rs` (LAB/ΔE, snap, semantic ops, color-map);
  `src/live.rs` live methods + `src/server.rs` `live_*` tools; `plugin.lua` handlers
  `get_region_colors` + `apply_color_map`. Reuses `live_list_palette` and
  replace-colour plumbing; pairs with `tools/lint_sprite.py` (palette check) as the
  objective gate.
- Test(s): `src/color_ops.rs::tests` (Tier-A table tests); live Tier-B shading run
  judged on linter-legality + hue direction.
