# SPEC-012 — Import animation (free Path-3 hybrid-generation backend)

- Status: **Draft (2026-06-24).**
- Owner: project
- Checklist items advanced: 2.x (new live-tool surface), 9.x (deterministic image tests).
- Related ADRs: ADR-0005 (loud per-command degradation); reuses ADR-0004 preview render
  path + SPEC-006 import core. No new ADR (no new return-type / protocol contract).
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  **Path 3 — hybrid generation** (the *free* default route: "user-supplied video" → frames →
  regrid/quantize/clean), §C.

## Intent
Path 3 ("hybrid generation") is the only route to genuinely **organic** characters/creatures:
a model supplies the organic base, the agent supplies pixel-art discipline. The **free /
offline** variant of that — the one that honours the lean-deps + no-paid-API invariants — is
to let the user bring a **generated animation** (from *any* source: Veo / a diffusion GIF
re-exported to frames / a video they ran through `ffmpeg`) and convert it into a clean,
**palette-locked Aseprite animation** the agent can trace and refine. SPEC-006
(`live_import_reference`) already does this for **one** still; SPEC-012 does it for a **multi-frame
sequence**, with **one consistent palette across all frames** (so the animation doesn't colour-flicker)
— the missing piece that turns "I have an organic motion" into an editable sprite animation.

**No generation happens in-tool** (that is the *paid* path, deferred): the organic frames are
user-supplied. This is purely the deterministic ingest+convert step.

## Inputs / Outputs
Two source modes (mutually exclusive), both **PNG-only** (no new dependency — the `image` crate
stays png-only; GIF/video decoders are a separate dep decision, see Decisions):
- **Sprite-sheet:** `filename` (one PNG) + `sheet {cols, rows}` — sliced row-major
  (left→right, top→bottom) into `cols*rows` equal frames (`width%cols==0`, `height%rows==0`,
  else a loud error).
- **Frame list:** `frames: [path, …]` — an ordered list of PNG paths, one per animation frame
  (all the same size; else a loud error).

Per-frame conversion params mirror `live_import_reference` exactly: `width`/`height` (target grid),
`method` (`dominant`/`average`), `palette` | `auto_colors` (+ `palette_method`) | `snap:false`,
`regrid`. Animation params: `layer` (default `"Reference Anim"`), `start_frame` (default 1),
`tag` (default `"ref"`; empty → no tag), `fps` (default 12 → per-frame duration `1/fps`),
`at_x`/`at_y`.

**Output:** N consecutive Aseprite frames drawn on `layer` (via the existing `draw_pixels` path),
per-frame durations set, an animation tag spanning them, and a JSON summary
`{frames, source_mode, per_frame:{width,height}, factor, palette_size, pixels_drawn, distinct_colors,
regrid, auto_palette, tag, fps}`.

## Behaviour
1. **Load frames** → `Vec<RgbaImage>` (pure `reference::slice_sheet` for a sheet; decode each
   path for a list). Cap: `1..=MAX_ANIM_FRAMES` (64) frames; each source edge ≤ `MAX_SOURCE_EDGE`.
2. **One shared palette** (the consistency guarantee): `auto_colors` extracts a single palette from
   **all frames combined** (`palette_extract::extract_from_images`), not per-frame; or an explicit
   `palette`; or the active sprite palette; or none (`snap:false`).
3. **Regrid once** (if requested): detect the native grid on **frame 0** and apply the same native
   to **every** frame (`regrid_then_fit`), so the whole sequence lands on one consistent grid.
4. **Resolve target dims** like SPEC-006 (explicit → honoured native → active sprite size).
5. **Ensure N frames** (`ensure_frames`), then for each frame `i`: downscale+snap the source frame to
   the target grid (reusing `downscale_to_grid` / `regrid_then_fit`), draw onto `layer` at
   `start_frame+i`, set its duration. A fully-transparent converted frame is skipped (counted), not
   an error.
6. **Tag** the range (`new_tag` from `start_frame` to `start_frame+N-1`) unless `tag` is empty.

## Decisions
1. **PNG-only, no new dependency.** The `image` crate stays `features=["png"]`. GIF (and via the
   user, video) would add the `gif`/`color_quant`/`weezl` crates — a Windows-SAC relink — so they
   stay a deliberate **dep decision** (the same gate as the paid generation path). Sprite-sheet +
   frame-list cover the free route with **zero** new deps; the user's free pre-step is "export your
   GIF/video to a sheet or PNG frames" (one `ffmpeg`/`magick` line, off-tool).
2. **One palette across all frames.** Per-frame `auto_colors` would flicker; extract once from all
   frames. This is the cross-frame-drift fix the research calls for, applied to colour.
3. **Reuse SPEC-006 core + `draw_pixels`; no new plugin command.** Same as import_reference — the
   only new live orchestration is frame creation + tagging, all via existing tools.
4. **Pure deterministic core, CI-gated.** `slice_sheet` and `extract_from_images` are pure Rust,
   unit-tested by `cargo test`.

## Acceptance criteria
- [ ] `reference::slice_sheet(img, cols, rows)` is pure + unit-tested: a `cols*rows` sheet slices
      into that many equal frames in row-major order; non-divisible dims and zero cols/rows are loud
      errors.
- [ ] `palette_extract::extract_from_images` collects opaque samples across all frames and returns
      one palette (unit-tested: two frames with disjoint colours yield a palette covering both).
- [ ] `live_import_animation` validates source mode (exactly one of sheet / frames), frame-count and
      size caps, draws N frames on one palette with durations + a tag, and returns the summary;
      mutually-exclusive `palette`/`auto_colors`/`snap:false` conflicts are loud (reused from SPEC-006).
- [ ] No new dependency; `cargo test --bins` green; schema-contract test covers the new tool.

## Eval
- **Deterministic (Rust, CI):** the `slice_sheet` + `extract_from_images` tables.
- **Live (on-demand):** "import this 4-frame walk sheet at 24×24 on an auto 8-colour palette" →
  4 Aseprite frames, one shared palette, a `ref` tag, each frame a clean palette-locked downscale.

## Traceability
- Module(s): `src/reference.rs` (`slice_sheet`), `src/palette_extract.rs` (`extract_from_images`),
  `src/live.rs` (`import_animation` + `LiveImportAnimationParams`/`LiveSheetGrid`),
  `src/server.rs` (`live_import_animation`). Reuses `downscale_to_grid`/`regrid_then_fit`/
  `is_real_upscale`/`distinct_colors`, `grid_to_pixels`, `ensure_frames`/`set_frame_properties`/
  `new_tag`/`draw_pixels`. No `plugin.lua` change.
- Test(s): `src/reference.rs::tests` (slice), `src/palette_extract.rs::tests` (multi-image),
  `src/live.rs` param validation, the schema-contract test.
