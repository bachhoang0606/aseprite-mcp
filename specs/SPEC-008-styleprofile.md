# SPEC-008 — StyleProfile pipeline (reference → machine-checkable style contract, roadmap #11)

- Status: **Draft (2026-06-21)** — design + Phase 1 deterministic core; Phase 2 (grid
  auto-detect + live tools) deferred.
- Owner: project
- Checklist items advanced: **9.4** (new objective eval axis — ramp-lint), 4.1 (palette/ramp
  discipline made machine-checkable), supports 2.x (future live tool).
- Related ADRs: reuses the eval two-tier split (`evals/README.md`, SPEC-007) and the
  `knowledge/palettes/<name>.json` `ramps` format; no new return-type/protocol contract → no
  new ADR.
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  §G ("Reference → style profile"), Path 4 (style grounding), roadmap **#11**. Builds on
  `tools/extract_palette.py` (the extract step already exists) and `rules/01-palette-and-color.md`.

## Intent
"Make a goblin matching my hero sheet" should be a **deterministic, lintable** task, not
vibes (§G). Turn a reference into a machine-checkable **`StyleProfile`** contract —
`{grid, palette, ramps, outline_policy, light_dir, heads_tall, frame_counts}` — derived by
mostly-deterministic native steps, that rig-builder / animation-director consume as **hard
constraints** and the linter / pixel-critic check against. The keystone is **ramp-lint**: a
new *objective scoring axis* (value-monotonic, per-step hue-shift, mid-peaked saturation, no
max-sat+max-value corner — `rules/01` + SLYNYRD) that plugs straight into the SPEC-007 eval
harness. It's the 2D analog of Figma's "named tokens, not hex".

## Inputs / Outputs
- **Inputs:** a reference PNG (a single sprite or a sheet); for Phase 1 it is taken at native
  resolution (grid auto-detect is Phase 2); optional `--grid WxH` / `--colors N` hints.
- **Outputs:** a **StyleProfile JSON** (extends the `knowledge/palettes/*.json` shape) +
  a **ramp-lint report** (per-ramp score + findings). All math is pure Python (stdlib, reuses
  `pixelpng.py` + `extract_palette.py`) → unit-testable, no Aseprite, **no new dependency**.

## Behaviour

### Phase 1 — deterministic core (pure Python, this build)
1. **`tools/ramp_lint.py` — the objective ramp-quality axis.** A ramp = an ordered dark→light
   hex list. Score (0..1) from codifiable rules, each emitting a finding when it fails:
   - **value_monotonic** (must-pass): luma strictly increases dark→light (`rules/01` §2).
   - **hue_shift**: hue rotates across the ramp with the right *direction* — darker steps
     cooler (toward blue), lighter warmer (toward orange/yellow); circular hue spread ≥ ~10°
     (`rules/01` §3).
   - **mid_peaked_sat**: saturation peaks in the middle steps, not at an endpoint (SLYNYRD).
   - **no_max_corner**: no step sits at both near-max saturation *and* near-max value (the
     muddy/garish corner — SLYNYRD).
   - **length**: 3–5 steps (`rules/01` §2; 2 = flat, >5 = wasteful).
   A ramp **passes** when score ≥ 0.7 and `value_monotonic` holds. Pure stdlib (HSV/luma from
   hex). CLI `python tools/ramp_lint.py palette.json [--ramp skin]` + `--selftest`.
2. **Ramp-sort.** Group a flat extracted palette into ramps: cluster colours by hue family,
   order each cluster by luma (dark→light), label by role heuristically (greenish→`skin`,
   brownish→`leather`, grey→`metal`, near-black single→`outline`). Deterministic (no RNG).
3. **`tools/style_profile.py` — derive the profile.** From a reference PNG, assemble:
   - `palette` — `extract_palette.py` (frequency by default).
   - `ramps` — ramp-sort of the palette (each `{role, colors[], length, lint}` with its
     ramp-lint score).
   - `light_dir` — compare mean luma of the top-left vs bottom-right opaque quadrants →
     `"top-left"` / `"top-right"` / … (§G).
   - `heads_tall` — silhouette bounding height ÷ the top colour-cluster (head) height.
   - `outline_policy` — sample the silhouette boundary: one dominant dark colour → `"uniform
     #hex"`; none → `"none"`; else `"selective"` (§G).
   - `grid` / `origin` / `frame_counts` — accepted from `--grid` or omitted (Phase 2 fills
     them from auto-detect).
   Serialize to a StyleProfile JSON (the palette-JSON shape + the profile fields), so it round-
   trips with the existing palette loaders.
4. **Eval integration.** A Tier-A check `ramp_lint_quality` (in `evals/run.py`): the
   `goblin-default` ramps score above the floor, and a synthetic value-only (no hue-shift)
   ramp is flagged below it — making ramp quality a deterministic, CI-gated axis (SPEC-007 9.4).

### Phase 2 — deferred (grid auto-detect + live)
- **Normalize / grid auto-detect** — Sobel edge-profile row/col histogram → median cell
  spacing → per-cell mode colour (proper-pixel-art / unfake.js, §C2/§G); fills `grid`/`origin`/
  `frame_counts` and lets the reference be any scaled/dithered image. Shared with SPEC-006
  Phase 2 (same regrid); carries the `imageproc`/decoder dependency cost, so deferred.
- **Live tools** — `live_extract_style_profile` (derive from the open sprite or a `Reference`
  layer) and feeding the profile to rig-builder / animation-director as hard constraints.

### Decisions
1. **Reuse what exists.** Palette extraction is `extract_palette.py`; the ramp format is the
   `knowledge/palettes/*.json` `ramps:{role:[hex]}` shape; the lint axis plugs into the SPEC-007
   harness. SPEC-008 adds ramp-sort, ramp-lint, and the profile assembler — not a parallel stack.
2. **Deterministic, stdlib, CI-gated first.** Phase 1 is pure Python (no Aseprite, no LLM, no
   new dep) so the whole pipeline is unit-testable and the ramp-lint axis is a hard gate; the
   live tools + Sobel grid auto-detect (which carry real cost) are Phase 2.
3. **Ramp-lint rules are the project's own** (`rules/01`) + SLYNYRD, not invented — and
   `value_monotonic` is the single must-pass (a non-monotonic "ramp" isn't a ramp).
4. **Role labels are heuristic + overridable.** Ramp-sort guesses a role from hue; the profile
   JSON is editable, so a wrong guess is a one-line fix, never load-bearing.

## Acceptance criteria
- [ ] `tools/ramp_lint.py` is pure-Python, stdlib-only, **unit-tested** (`--selftest`): a known
      hue-shifted ramp scores ≥ 0.7 and passes; a grayscale value-only ramp fails `hue_shift`;
      a 2-step ramp fails `length`; a #ffffff-at-max-sat ramp fails `no_max_corner`; a
      non-monotonic ramp fails the `value_monotonic` must-pass.
- [ ] `tools/style_profile.py` derives a StyleProfile JSON from a reference PNG with
      `palette`, `ramps:[{role, colors, length, lint}]`, `light_dir`, `heads_tall`,
      `outline_policy`; reuses `extract_palette.py`; unit-tested on a synthetic reference with a
      known light direction + head proportion.
- [ ] `evals/run.py` gains `ramp_lint_quality` (registered in `CHECKS`, mapped in `cases.json`):
      the `goblin-default` ramps pass and a value-only ramp is flagged → CI gates ramp quality.
- [ ] All Tier-A checks pass (13 → 14); no new dependency; runnable with no Aseprite.

## Eval (how we grade it)
**Tier-A (deterministic, CI):** the `ramp_lint` `--selftest` table above; the `ramp_lint_quality`
gate (good ramps pass, a value-only ramp fails); `style_profile` derivation unit tests.
**Tier-B (on-demand):** "match this hero sheet" — derive a StyleProfile from a reference, draw a
new sprite, and check it against the profile (palette ⊆ profile palette, ramps lint-pass, light
direction consistent) — a deterministic axis the judge can cite.

## Traceability
- Module(s): `tools/ramp_lint.py` (the objective axis), `tools/style_profile.py` (assembler +
  ramp-sort), `evals/run.py` `ramp_lint_quality` + `evals/cases.json`. Reuses
  `tools/extract_palette.py`, `tools/pixelpng.py`, `knowledge/palettes/*.json`, `rules/01`.
- Test(s): `ramp_lint --selftest` + `style_profile` unit tests; the new `run.py` check.
