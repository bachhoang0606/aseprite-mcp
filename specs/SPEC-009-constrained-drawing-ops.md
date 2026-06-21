# SPEC-009 — Constrained drawing operations (roadmap #8, Path 2/5)

- Status: **Draft (2026-06-21)** — design + Phase 1 (`dither_fill_region`). Phase 2
  (`gradient_map`, `rotsprite` rotation) deferred.
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

### Phase 2 — deferred
- **`gradient_map`** — map each pixel's luma to a position along a ramp (the StyleProfile ramps
  feed straight in); pure Rust, no new dep. A natural follow-up.
- **`rotsprite` rotation** — artifact-free rotation that introduces no new colours
  ([`rotsprite` crate](https://docs.rs/rotsprite), §E). Carries a **new crate dependency**
  (Windows-SAC relink cost), so deferred behind a deliberate dep decision.

### Decisions
1. **Palette-legal by construction.** A two-colour ordered dither can only emit its two inputs,
   so the result never needs a snap pass — the whole point of Path 2.
2. **Reuse `draw_pixels`, no new plugin command** (like SPEC-006). A Phase-2 `fill` plugin
   primitive is an efficiency nicety, not required for a bounded region.
3. **Pure deterministic core, CI-gated by `cargo test`.** The Bayer pattern is unit-tested in
   Rust (which CI runs), so the op is gated without needing a Python eval mirror.
4. **Defer the dep-carrying op (`rotsprite`).** Dithering + gradient-map are stdlib-pure; only
   rotation wants a crate, so it waits behind a lean-deps decision.

## Acceptance criteria
- [ ] `src/dither.rs` is pure Rust, **unit-tested**: `level=0` → all `color_a`; `level=1` → all
      `color_b`; Bayer-4 at `level=0.5` → exactly half each in a known checker arrangement; the
      pixel list is offset by the region origin and covers every cell once.
- [ ] `live_dither_fill` validates inputs (empty/oversized rect, bad colour, `level` ∉ [0,1] →
      loud errors), resolves a rect or the active selection, and draws the dither onto the target
      layer via `draw_pixels`; returns the JSON summary. Pure helpers unit-tested; the
      schema-contract test covers the tool.
- [ ] No new dependency; `cargo test --bins` green. (Live-verify of the draw is the
      already-proven `draw_pixels` path.)

## Eval (how we grade it)
- **Deterministic (Rust, CI):** the `dither.rs` table above (the two endpoints + the 50% Bayer
  pattern + full coverage).
- **Live (on-demand):** "dither-shade this region from skin-mid to skin-dark at 40%" → the region
  shows a clean ordered dither in only those two palette colours; `/pixel-review` palette axis
  passes (no off-ramp strays).

## Traceability
- Module(s): `src/dither.rs` (pure Bayer core), `src/live.rs` `dither_fill` (resolve + draw),
  `src/server.rs` `live_dither_fill`. Reuses `color_ops::Rgba`, the `draw_pixels` path, and
  `get_selection`. No `plugin.lua` change.
- Test(s): `src/dither.rs::tests` (endpoints / 50% pattern / coverage); `live.rs` param-validation.
