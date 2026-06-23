# SPEC-009 — Constrained drawing operations (roadmap #8, Path 2/5)

- Status: **COMPLETE (2026-06-21).** All three ops landed: Phase 1 `dither_fill`, Phase 2
  `gradient_map`, and `rotsprite` rotation — the last **hand-rolled dep-free** (no `rotsprite`
  crate) so it adds nothing to the Windows-SAC link. All three are pure-Rust, eval-gated by
  `cargo test`, and draw via existing paths (no plugin change).
- Owner: project
- Checklist items advanced: 2.1 (live tool surface), 4.1 (palette-legal by construction).
- Related ADRs: ADR-0005 (loud per-command degradation). No new ADR — reuses the
  `draw_pixels` apply path (like `import_reference`/SPEC-006); no new plugin command.
- Source: research doc §D/§E (the deterministic ops LLMs do worst — "algorithms are solved"),
  roadmap **#8**.

## Intent
Some pixel-art work is *deterministic and tedious* — exactly what an LLM does worst freehand:
ordered **dithering**, **gradient mapping** to a ramp, artifact-free **rotation**. The
algorithms are textbook, so a one-call tool makes each **palette-legal by construction** and
removes a whole class of freehand errors (§D). This spec adds them as live tools; Phase 1 is
the most iconic + self-contained one, **`dither_fill_region`**.

## Inputs / Outputs
- **`live_dither_fill` (Phase 1):** `rect` (`{x,y,width,height}`) or `selection_only`; `color_a`
  + `color_b` (two `#rrggbb` — usually two adjacent ramp steps); `level` (0..1, the fraction of
  `color_b`, default 0.5); `matrix` (`bayer4`/`bayer2`/`checker`, default `bayer4`); `layer`,
  `frame`. **Output:** the dithered pixels drawn via the existing `draw_pixels` path; a JSON
  summary `{pixels, color_a, color_b, level, matrix}`. The two-colour mix is **palette-legal by
  construction** — only `color_a`/`color_b` are emitted.
- All pattern math is pure Rust (`src/dither.rs`) → unit-testable, no Aseprite, no new dependency.

## Behaviour

### Phase 1 — `dither_fill_region` (this build)
1. **Pure core `src/dither.rs`.** An **ordered (Bayer) dither**: a normalized threshold matrix
   `T(x,y) ∈ [0,1)` (Bayer 4×4 / 2×2, or a 1×1 checker) tiled over the region; a cell takes
   `color_b` when `T(x % m, y % m) < level`, else `color_a`. `level=0` → all `color_a`,
   `level=1` → all `color_b`, `level=0.5` → the classic 50% checker/Bayer blend. Returns the
   pixel list (offset by the region origin). Deterministic; only the two input colours appear.
2. **`live::dither_fill`.** Resolve the rect (params or the active selection via `get_selection`),
   validate the two colours (`color_ops::Rgba::from_hex`) and `level`, run the core, draw the
   batch onto `layer` via the existing `draw_pixels` path. Bound the region area so a huge fill
   can't explode the batch (reuse the import cap). Loudly refuse an empty rect / bad colour.
3. **`live_dither_fill` tool** (`server.rs`).

### Phase 2 — `gradient_map` (LANDED) + `rotsprite` (LANDED, dep-free)
- **`gradient_map` (LANDED)** — `color_ops::gradient_map(c, ramp)` maps a colour to the ramp
  step matching its luma (dark→light), preserving alpha; the live `live_gradient_map` builds a
  per-unique-colour map and applies it via the **SPEC-004 `get_region_colors` → `apply_color_map`**
  path (no render, no new plugin command). A StyleProfile ramp feeds straight in. Palette-legal by
  construction. Unit-tested in `color_ops`; the schema-contract test covers the tool.
- **`rotsprite` rotation (LANDED, hand-rolled dep-free)** — artifact-free rotation that introduces
  no new colours (§E). Rather than add the [`rotsprite` crate](https://docs.rs/rotsprite) (a
  Windows-SAC relink), the algorithm is hand-rolled in pure Rust (`src/rotate.rs`): **Scale2× (EPX)
  ×3 → nearest-neighbour rotate into the rotated bbox → ×8 mode-downscale**. Every stage *selects*
  an existing colour and none *blends*, so the output palette ⊆ input ∪ {transparent} —
  palette-legal by construction. Right angles (0/90/180/270) are exact rearrangements. The live
  `live_rotate` reads the flattened render (modal-free `save_preview`), rotates a region, and stamps
  the clean copy onto a NEW layer via `draw_pixels` (no plugin change). Unit-tested in `rotate.rs`
  (right-angle exactness, no-new-colours at 45°, bbox growth, solid-stays-solid, mode tie-breaking);
  the schema-contract test covers the tool.

### Decisions
1. **Palette-legal by construction.** A two-colour ordered dither can only emit its two inputs,
   so the result never needs a snap pass — the whole point of Path 2.
2. **Reuse `draw_pixels`, no new plugin command** (like SPEC-006). A Phase-2 `fill` plugin
   primitive is an efficiency nicety, not required for a bounded region.
3. **Pure deterministic core, CI-gated by `cargo test`.** The Bayer pattern is unit-tested in
   Rust (which CI runs), so the op is gated without needing a Python eval mirror.
4. **Hand-roll `rotsprite` dep-free.** The `rotsprite` crate would add a Windows-SAC relink for an
   algorithm that is ~150 lines of textbook code (Scale2× + NN-rotate + mode-downscale). Hand-rolling
   keeps the lean-deps invariant *and* gives us a unit-tested core the eval gate runs directly —
   strictly better than the crate here. (Decision reversed from the original "defer behind a dep
   decision": the dep was the cost, and we avoided it.)

## Acceptance criteria
- [ ] `src/dither.rs` is pure Rust, **unit-tested**: `level=0` → all `color_a`; `level=1` → all
      `color_b`; Bayer-4 at `level=0.5` → exactly half each in a known checker arrangement; the
      pixel list is offset by the region origin and covers every cell once.
- [ ] `live_dither_fill` validates inputs (empty/oversized rect, bad colour, `level` ∉ [0,1] →
      loud errors), resolves a rect or the active selection, and draws the dither onto the target
      layer via `draw_pixels`; returns the JSON summary. Pure helpers unit-tested; the
      schema-contract test covers the tool.
- [x] `src/rotate.rs` is pure Rust, **unit-tested**: right angles are exact rearrangements; a 45°
      rotation introduces **no colour outside `input ∪ {transparent}`**; the bbox grows ~√2 for a
      45° square; a solid square stays one colour (no AA fringe); block-mode picks the majority and
      breaks ties deterministically by first-seen. `live_rotate` resolves a rect / selection / whole
      canvas, caps the source area (the ×8 buffer), centres or places the result, and draws onto a
      NEW layer; the schema-contract test covers the tool.
- [x] No new dependency; `cargo test --bins` green (139 tests). **Live-verified 2026-06-24**
      (`evals/runs/2026-06-24/live_verify.json`): `live_rotate` 33° → palette-legal (5 colours,
      no AA fringe); `live_dither_fill` bayer4 → only the two ramp colours; `live_gradient_map`
      → output within the ramp; composite had **0 off-palette pixels**.

## Eval (how we grade it)
- **Deterministic (Rust, CI):** the `dither.rs` table above (the two endpoints + the 50% Bayer
  pattern + full coverage).
- **Live (on-demand):** "dither-shade this region from skin-mid to skin-dark at 40%" → the region
  shows a clean ordered dither in only those two palette colours; `/pixel-review` palette axis
  passes (no off-ramp strays).

## Traceability
- Module(s): `src/dither.rs` (pure Bayer core), `src/live.rs` `dither_fill` (resolve + draw),
  `src/server.rs` `live_dither_fill`. `gradient_map`: `color_ops::gradient_map`, `live.rs`
  `gradient_map`, `server.rs` `live_gradient_map`. `rotsprite`: `src/rotate.rs` (pure Scale2× /
  NN-rotate / mode-downscale core), `src/live.rs` `rotate` + `selection_bounds` +
  `region_to_raster`/`raster_to_pixels`, `src/server.rs` `live_rotate`. Reuses `color_ops::Rgba`,
  the `draw_pixels` path, `save_preview`, and `get_selection`. No `plugin.lua` change.
- Test(s): `src/dither.rs::tests` (endpoints / 50% pattern / coverage); `src/rotate.rs::tests`
  (right-angle exactness / no-new-colours / bbox growth / solid-stays-solid / mode tie-break);
  `live.rs` param-validation; the schema-contract test (all tools).
