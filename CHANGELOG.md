# Changelog

## Unreleased

### Added
- **SPEC-003 Phase 3 — `live_create_autotile_template`: draw ~4 quarters → get 47 tiles (roadmap #10).**
  Completes the last deferred piece of the tilemap family. The agent draws the **4 corner quarters**
  as a strip `[fill | outer | edge | inner]` (each `tile_size/2` square, canonical orientation:
  `outer`=convex top-left, `edge`=boundary on top, `inner`=concave top-left notch); the tool composes
  **all 47 blob tiles deterministically** (the 4-corners-per-tile model) onto a new layer, ready for
  `live_pack_similar_tiles(grid_size=tile_size)` to build the tileset — with tile order matching
  `autotile::blob47_tile_index` by construction. Pure compositor in `src/autotile.rs` (`CornerPieces`,
  lossless `rotate90`, the `quadrant_piece` lookup, `assemble_tile`/`assemble_blob47`,
  `slice_corner_pieces`, `sheet_dims`) — **palette-legal by construction** (only the source colours,
  like `live_rotate`); rotation/placement verified by marker tests, the no-new-colour guarantee
  asserted across all 47 tiles (13 autotile unit tests, 153 total). Live tool reads the render and
  draws via the existing `draw_pixels` path — **no new plugin command, no new dependency**;
  schema-contract test covers the new param. `wang16` is a follow-up; live-verify on an Aseprite
  session pending.
- **`/pixel-asset` skill — assets-first: search → preview → import (SPEC-011 follow-up).** The skill
  (`skills/pixel-asset/`) that chains the asset-search tool into the live editor (research §F): start
  from a curated free/CC0 source instead of inventing from text. It orchestrates existing pieces —
  `tools/asset_search.py` (`search`/`fetch`, network-gated) → **preview the candidate** (Read the PNG
  / `live_save_preview`) → for a **palette**, fetch into `knowledge/palettes/` and hand off to
  `/pixel-palette load`; for an **asset image**, `fetch --url` it then `live_import_reference`
  (with `regrid:true` for scaled refs, snapped to the active palette) onto a **locked** `Reference`
  layer (`live_set_layer_properties editable:false`) to trace over, then restyle + `/pixel-review` (the
  §F "missing middle step"). Provenance (`MANIFEST.json` + `CREDITS.txt`) is kept at fetch time;
  honest licensing carried through (CC0 only where the source guarantees it; palettes = courtesy
  attribution). No new tool, no new dependency — composes shipped tools/skills.
- **SPEC-011 — asset search: CC0 assets + Lospec palettes, stdlib & gated (roadmap #12).** The 2D
  analog of blender-mcp's PolyHaven/Sketchfab flow (research §F): `tools/asset_search.py` searches
  curated free/CC0 sources — **Lospec** palettes (turns `knowledge/palettes/` from a handful into
  thousands) and the **Hugging Face `nyuuzyou/OpenGameArt-CC0`** dataset (15.7k genuinely-CC0
  assets) — then `fetch`es a candidate into the project with provenance captured at fetch time
  (`MANIFEST.json` + auto-generated `CREDITS.txt`, the ULPC pattern). **Lean-deps / Windows-SAC:**
  stdlib only (`urllib`+`json`) — **no `requests`/`duckdb`/`pyarrow`** (the HF dataset is Parquet,
  but its datasets-server REST API returns JSON) and **no Rust HTTP crate**. **Network egress is
  opt-in** (`--online` or `ASEPRITE_MCP_ALLOW_NET=1`, mirroring the ADR-0003 Lua gate) with a
  **no-API default**: offline `search` falls back to a bundled catalog (`knowledge/asset-catalog.json`)
  so it stays useful — and CI-testable — with zero network; offline `fetch` of an online-only
  candidate refuses **loudly** (names the flag). **Honest licensing:** CC0 only where the source
  guarantees it (the HF dataset); a palette is a colour list (not copyrightable), recorded with
  courtesy attribution, never relabelled CC0. Normalized source-agnostic candidate shape; URL-scheme
  guard (http(s) only), path-traversal-safe filenames, size-capped downloads. A `fetch --url`
  mode completes the HF flow (download an asset URL from a `search --online` result with
  provenance). Offline `--selftest` + 26 unit tests (Lospec/HF parsers, catalog search/filter,
  gate, manifest/credits, guards, `--url` CLI), wired into the `quality` CI job (no network). No
  new dependency, no Rust change. (A `/pixel-asset` skill
  that chains search → preview → `live_import_reference` is the documented follow-up.)
- **`/pixel-doctor` — diagnose the live-infra dance (roadmap #13).** A skill (`skills/pixel-doctor/`)
  + a stdlib helper (`scripts/pixel_doctor.py`) that diagnose the recurring connection pain and give
  the **exact** fix, so the agent stops retrying blindly (or — forbidden — falling back to offline
  file tools). The skill's decision tree branches on `live_preflight`'s **`bridgeLinked`** field
  (`connected = bridgeLinked && plugin_connected`): `bridgeLinked:false` → the bridge layer (missing
  sibling `aseprite-live-bridge` beside the server exe / SAC-blocked fresh build / orphan holding the
  port); `bridgeLinked:true` + `lastHello:null` → the plugin layer (launch Aseprite, enable the
  extension, focus the window). The helper automates the deterministic file/config side: validates
  the `~/.claude.json` `aseprite-live` `command` exists with its **sibling bridge co-located** (any
  build dir — not just `target/release`), flags a **wrong-repo** pointer (underscore `aseprite_mcp`
  vs hyphen `aseprite-mcp`) and a **shadowing** `aseprite` stdio server, replicates the offline
  Aseprite-exe resolution (`ASEPRITE_PATH` → install dirs → PATH), detects a **stale** registered
  binary by mtime (`serverVersion` is hardwired `0.1.0`, so it can't), reads **Windows SAC** state
  (Enforce → OS 4551 / `EUNKNOWN`), and probes ports 9876/9877. Grounded in a code-verification pass
  over `src/live.rs` (`spawn_bridge`, `status_json`/`preflight`) + `src/aseprite.rs`
  (`locate_executable`). Pure diagnosis core unit-tested (18 tests) + offline `--selftest`, wired into
  the `quality` CI job. Stdlib-only, no new dependency, no Rust change.
- **SPEC-006 Phase 2 — `live_import_reference` regrid (de-fake scaled references; roadmap #6-v2).**
  A new `regrid: true` option recovers a *scaled* reference to its **true pixel grid** before
  snapping. AI/diffusion output and screenshots are often a 1024×1024 image that is "really" 64×64
  upscaled 16× — importing that as-is resamples a blurred grid. With `regrid`, the import auto-detects
  the native cell size (block-uniformity / GCD vote, the proven `tools/regrid.py` detector now shared
  as `style_profile::detect_grid`) and, when a real upscale is found (`scale > 1`), recovers the exact
  1× pixels: a single dominant-vote pass when the target equals the native grid, else a two-pass
  *recover-native → fit* so the final downscale starts from clean pixels rather than the scaled blur.
  When `width`/`height` are omitted the import now lands on the **detected native resolution** instead
  of the active sprite size. **Loud degradation** throughout: a native-art/photo source (`scale == 1`)
  is a no-op; a flat-swatch *degenerate* detection (block-uniformity collapses to a ~1×1 native) is
  rejected by `is_real_upscale` rather than silently imported as one pixel; and a detected native over
  the 256px import cap (with no dims to fit it) returns a dedicated `native_exceeds_cap` error. The
  summary gains a `regrid` block (`detected_scale`, `native`, `applied`). Pure, unit-tested helpers
  (`reference::regrid_then_fit` two-pass recover→fit, `is_real_upscale`, `resolve_import_target`): a
  4×-upscale recovers its native 8×8 bit-for-bit, the two-pass fit equals fitting the true native (and
  beats a single downscale of the blur), a solid source is not treated as an upscale, and
  `resolve_import_target` precedence/fallback. **143 unit tests pass**; the schema-contract test
  validates the new param. **No new dependency, no new plugin command** — reuses the existing
  `detect_grid` + `downscale_to_grid` + `draw_pixels` paths. SPEC-006 Phase 2 (regrid) is now landed;
  the auto-palette / non-PNG decoders remain deferred.
- **SPEC-010 — `pixel_creation_strategy` MCP prompt: the drawing doctrine, baked into the server
  (roadmap #5).** The first MCP **prompt** (not a tool) — a hard-ordered, pull-only playbook the
  client injects before any drawing/editing/animation task, so every client (Cursor / Codex /
  Copilot, incl. non-vision) gets the doctrine, not just Claude-Code-skill users (the verified
  blender-mcp `asset_creation_strategy()` pattern, research §B). It orders the workflow:
  **0)** connect first (live-first / `live_preflight`); **1)** perceive before acting — a raw 32²
  preview ≈ 4 vision patches ≈ invisible, so always `live_save_preview` (~1024px) + `live_ascii_view`
  machine-truth; **2)** lock a palette, snap with the real CIELAB tools (never hand-pick shades);
  **3)** prefer constrained tools (`live_import_reference`/`dither_fill`/`gradient_map`/`rotate`) over
  hand-plotting; **4)** ramp & light discipline (`live_extract_style_profile`); **5)** re-perceive
  before AND after (`live_frame_diff`, `live_save_filmstrip` — the API sees only a GIF's first frame);
  **6)** validate against a hard gate; **7)** script last. Wired via rmcp's `#[prompt_router]` with a
  `PromptRouter` field + `enable_prompts()` + manual `list_prompts`/`get_prompt` delegation (mirrors
  the existing manual tool wiring, since the server keeps a custom `get_info`). Pure static content
  (`src/prompts.rs`) — works disconnected. Content + registration unit-tested so the doctrine can't
  silently rot. **Transaction/snapshot confirmation (the roadmap's part 2):** audited `plugin.lua` —
  content/create-delete ops (`draw_pixels`, cels, frames, tags, snap/adjust, tiles) already wrap in a
  named `app.transaction("MCP Live …")` → one Ctrl+Z per action, but in-place property-setters
  (`set_*_properties`, `rename`/`delete`/`create-group` layer, `set_palette_color`, `resize_*`) are
  NOT — wrapping them uniformly is recorded as a plugin follow-up (needs a validate-then-transact
  restructure + live verify). `run_lua_script`/`execute_cli` are offline/batch (no live undo stack)
  and gated by `ASEPRITE_MCP_ALLOW_LUA`. No plugin code change here. 146 unit tests pass. No new
  dependency, no new plugin command.
- **SPEC-009 COMPLETE — `live_rotate`: artifact-free RotSprite rotation, hand-rolled dep-free
  (roadmap #8).** Rotate a region of the sprite by **any angle** (positive = clockwise) and stamp
  the clean result onto a NEW layer (the source is untouched). The classic **RotSprite** pipeline
  (Xenowhirl) hand-rolled in pure Rust (`src/rotate.rs`) rather than pulling the `rotsprite` crate —
  no Windows-SAC relink: **Scale2× (EPX) ×3 → nearest-neighbour rotate into the rotated bbox → ×8
  mode-downscale**. Every stage *selects* an existing colour and none *blends*, so the output
  palette ⊆ input ∪ {transparent} — **palette-legal by construction**, none of the anti-aliased
  fringe a naive rotate leaves. Right angles (0/90/180/270) are exact rearrangements. The live tool
  reads the flattened render (modal-free `save_preview`), resolves a `rect` / `selection_only` /
  whole canvas, centres or places the result (`at_x`/`at_y`), and draws via the existing
  `draw_pixels` path (**no new plugin command**; source area capped at 200² px for the ×8 buffer).
  8 unit tests (right-angle exactness, no-new-colours at 45°, √2 bbox growth, solid-stays-solid,
  block-mode majority + deterministic tie-break, flat-image round-trip); **139 unit tests pass**;
  the schema-contract test validates the tool. **No new dependency.** SPEC-009 is now complete.
- **SPEC-009 Phase 2 — `live_gradient_map`: re-shade a region onto a ramp (roadmap #8).**
  Map every colour in a layer/selection to the ramp step matching its luma (dark→light) — turn a
  rough/grey region or a recolour onto a target ramp in one call. **Palette-legal by construction**
  (only ramp colours are emitted), and a **StyleProfile ramp** (`live_extract_style_profile`) feeds
  straight in. Pure `color_ops::gradient_map(c, ramp)` (luma → ramp index, alpha preserved);
  the live tool builds a per-unique-colour map and applies it via the **SPEC-004
  `get_region_colors` → `apply_color_map`** path — no render, **no new plugin command**.
  Unit-tested (black→darkest, white→lightest, mid-grey→mid, transparent/empty no-ops, alpha
  preserved); 131 unit tests pass; the schema-contract test validates the tool. No new dependency.
  Only `rotsprite` rotation (a crate dep) remains in SPEC-009.
- **SPEC-009 Phase 1 — `live_dither_fill`: ordered dithering between two palette colours
  (roadmap #8, Path 2/5).** The tedious deterministic shading an LLM does worst freehand,
  made **palette-legal by construction** — an ordered (Bayer) dither emits only its two input
  colours, so the result never needs a snap pass. Pure-Rust core `src/dither.rs` (Bayer-4/2 or
  checker threshold matrix tiled over the region; a cell takes `color_b` when its threshold <
  `level`, else `color_a`); the live tool resolves a `rect`, validates the two colours + `level`
  + `matrix`, and draws via the existing `draw_pixels` path (**no new plugin command**, region
  capped at 256² px). 4 unit tests (pure endpoints at level 0/1, even Bayer-4 split at 0.5,
  checker alternation, offset+coverage); 130 unit tests pass; the schema-contract test validates
  the tool. No new dependency. Phase 2 (`gradient_map`, `rotsprite` rotation — the latter carries
  a crate dep) is deferred — see SPEC-009.
- **SPEC-008 — `live_extract_style_profile`: derive a StyleProfile live (roadmap #11 complete).**
  The Rust live tool that completes the StyleProfile pipeline: derive a machine-checkable
  `{grid, palette, ramps:[{role, colors, length, lint}], light_dir, heads_tall, outline_policy}`
  from the **open sprite** so "match my hero sheet" is a deterministic, lintable task (§G).
  Renders a modal-free 1× copy (no plugin change — reuses the raw `save_preview` render) and
  analyses it in pure Rust (`src/style_profile.rs`) — a **faithful port** of the offline
  `tools/{regrid,ramp_lint,extract_palette,style_profile}.py`, with Rust unit tests mirroring
  their selftests (grid de-fake native→1/4×→4, the goblin-ramp lint calibration, the
  geometry read) so the two implementations can't diverge. `grid` auto-detects the native
  resolution; `ramps` carry their ramp-lint score; reuses `color_ops::Rgba`. 126 unit tests pass
  (3 new); the schema-contract test validates the tool. No new dependency.
- **SPEC-008 Phase 2 — native-grid auto-detect (de-fake scaled references) (roadmap #11, §C2/§G).**
  `tools/regrid.py` recovers the true native resolution of "fake" pixel art (e.g. a 1024px image
  that is really 64×64) so palette/ramp/proportion analysis runs at the right scale. Method
  (corrected from "autocorrelation"): the **largest n whose grid-aligned n×n blocks are
  mode-uniform** — a clean n×-upscale makes every block one source pixel, while native art fails
  at n=2 (adjacent pixels differ). Pure stdlib (**no `imageproc`/new dependency** — that cost was
  only the Rust path), `tol=0.9` tolerates light dithering. `style_profile.py` now fills `grid`
  (`{cell_w, cell_h, native, scale}`) from it; `--selftest` covers native→1 / 4×→4 / 3×→3, and a
  new eval gate `regrid_detects_scale` makes it deterministic in CI. 14/14 Tier-A checks pass. The
  only remaining SPEC-008 piece is the Rust live tool `live_extract_style_profile`.
- **SPEC-008 Phase 1 — StyleProfile pipeline core: ramp-lint + profile derivation (roadmap #11,
  Path 4).** Turns "match my hero sheet" into a deterministic, lintable contract (§G).
  `tools/ramp_lint.py` (pure stdlib) is the keystone — an **objective ramp-quality axis**:
  value-monotonic (must-pass), per-step **hue-shift** (cooler dark / warmer light, coupled to
  real hue rotation), **mid-peaked saturation**, **no max-sat+max-value corner**, 3–5 steps
  (`rules/01` + SLYNYRD); pass = score ≥ 0.70. `tools/style_profile.py` derives a StyleProfile
  JSON from a reference PNG — `palette` (reuses `extract_palette.py`), `ramps:[{role, colors,
  length, lint}]` (hue-clustered + luma-sorted), `light_dir` (top-left vs bottom-right luma),
  `heads_tall` (silhouette ÷ head height), `outline_policy` (boundary sampling); `grid` /
  `frame_counts` are Phase 2 (Sobel auto-detect). Wired into the eval harness as a new
  deterministic gate `ramp_lint_quality` (the project's own goblin-default ramps lint as good —
  skin 1.0, leather 0.99, mouth 0.79, tooth 0.9 — and a value-only grey ramp is flagged). Both
  tools `--selftest`; 13/13 Tier-A checks pass. No new dependency, no Aseprite. Phase 2 (grid
  auto-detect + `live_extract_style_profile` feeding rig-builder/animation-director) — see SPEC-008.
- **SPEC-007 Phase 2 — degradation / persona-A/B / cross-path measurement tooling (roadmap #9).**
  The live/on-demand half: machinery is CI-verified, the *runs* are operator-driven. `judge.py`
  gains `--emit-ab <case>` (a paired **persona A/B** prompt — Variant A with the candidate
  `PERSONA_CANDIDATE` line, Variant B without, judged blind; adopt only if mean Δ ≥ +0.05 over
  ≥3 runs) and `compute_slope` / `--slope <json>` (the **long-session degradation / donut-test**
  helper: snapshots → slope + regression flag, exit 1 if regressed). A new deterministic Tier-A
  check `degradation_slope_math` gates the slope logic in CI (flags a decaying series, passes a
  stable one). New `evals/BENCHMARK.md` scaffolds the cross-path (perception/colour on-vs-off) +
  persona-A/B + degradation result tables (re-derivable from `evals/runs/<date>/`); README
  documents the three protocols. 12/12 Tier-A checks pass; no new dependency. The recorded live
  runs remain on-demand (Aseprite + tokens + judge) by design.
- **SPEC-007 Phase 1 — silhouette-IoU animation-drift gate + SwordsBench cases (roadmap #9,
  checklist 9.3/9.4).** The objective validator the research calls *mandatory* (the donut-test
  antidote), now a **hard CI gate**. `tools/silhouette_iou.py` (pure stdlib, reuses
  `pixelpng.py`) measures cross-frame **proportion drift** — SwordsBench's #1 animation failure
  — as the IoU of consecutive frames' opaque-pixel silhouettes (high-motion tags fall back to
  bbox-area stability). Two deterministic checks wired into `evals/run.py` (so `quality.yml`
  blocks on a non-zero exit): `silhouette_iou_stable` (a clean walk stays above the 0.80 floor)
  and `silhouette_iou_detects_drift` (a ballooned frame is caught below it), against committed
  golden fixtures under `evals/fixtures/` (regenerated by `make_fixtures.py`, a snapshot
  contract). Adds the verbatim-style **SwordsBench** cases `tb_swords_static` / `tb_swords_walk`
  to `evals/tier_b.json` (CI-validated well-formed by `tier_b_cases_wellformed`). Metric
  `--selftest` covers identity/1px-shift/disjoint/strip-slice; 11/11 Tier-A checks pass. No new
  dependency; no Aseprite needed (Tier-A is deterministic). Phase 2 (long-session degradation,
  persona A/B, cross-path benchmark) is live/on-demand — see SPEC-007.
- **`/pixel-tileset` skill (SPEC-003 checklist 5.x).** A `/pixel-*` verb that wraps the
  live tilemap tools end-to-end: preflight + `tilemap`-capability gate, then either paint
  a seamless mockup → `live_pack_similar_tiles` (dedupe) or author tiles →
  `live_stamp_tiles`, review the packed tiles via `live_get_tileset` (vision PNG), validate
  edges with `tools/seam_lint.py`, and `live_export_tileset` to Tiled/Godot/JSON (blob47
  wangset for terrain; LDtk reads `.aseprite` directly). Palette-locked tiles, seam gate,
  loud capability/preflight stops — no batch fallback. `skills/pixel-tileset/SKILL.md`.
- **`/pixel-reference-motion` skill — rotoscope a reference motion into a clean animation
  (roadmap #7, research §C1).** Turn a video clip, an animated GIF, or a PNG frame sequence
  into a palette-locked pixel-art animation by importing each reference frame as a dimmed,
  on-palette `Reference` layer (via `live_import_reference`, SPEC-006) on its own animation
  frame, then tracing clean pixels over it — fixing the cross-frame character drift you get
  from generating each frame independently. Ships `tools/video_frames.py` (stdlib-only,
  reuses `pixelpng.py`): wraps `ffmpeg` to sample K evenly-spaced key frames and chroma-keys
  a green (`#00ff00`) background to transparent with the adaptive green-dominance test
  `g - max(r,b) > threshold` (Mike Veerman's "Claude After Dark" method) — thresholds and
  key colour CLI-configurable, `--frames` skips extraction for a pre-made sequence,
  `--selftest` verifies the keying logic. The skill enforces: lock one shared palette first
  (anti-flicker, §C4), reduce to 4–8 key poses (`rules/04`), review via `live_save_filmstrip`
  (never a GIF — the API sees only frame 1), and remove the reference layer before shipping.
  No new dependency (ffmpeg is the one external tool, and only the extract stage needs it).
- **SPEC-006 Phase 1 — `live_import_reference`: reference image → palette-locked pixel art
  (Path 3/4, the hybrid-pipeline unlock).** Import a PNG reference (photo, illustration, AI
  image, CC0 asset) onto a layer in the open sprite so the agent can trace/clean over it
  instead of inventing organic shapes from text. Two deterministic steps fused in one pass:
  a **content-aware downscale** to the target grid (`method:"dominant"` = per-cell majority
  vote — edge-preserving, never a bilinear blur that invents colours — or `"average"`) and a
  **CIELAB snap** to a palette (`palette` list, or the active sprite palette by default;
  `snap:false` keeps source colours). Target defaults to the active sprite's size; result is
  drawn via the existing `draw_pixels` path (**no new plugin command**) at `at_x`/`at_y` on a
  `Reference` layer. Pure core in `src/reference.rs` (downscale/snap/transparency/area, 7
  tests) reusing `color_ops` CIELAB; pure live helpers (target/palette/grid validation, 4
  tests). **No new dependency**; source-size guard reads dimensions before decode so a huge
  PNG can't OOM; PNG-only input + Sobel grid auto-detect / auto-palette are SPEC-006 Phase 2.
  83 unit tests pass; clippy adds no new lints.
- **SPEC-005 Phase 4 — `live_save_preview` Set-of-Mark numbered regions (`marks_from`).**
  Overlay numbered badges on regions and return a `marks:[{n, region, bbox}]` map so the
  critic can say "region 3 has a stray pixel" and the orchestrator maps `3 → that
  slice/layer/blob` — no fragile free-form coordinates (research §A SoM). `marks_from`:
  `"slices"` (one per named slice), `"layers"` (one per visible layer's cel at the active
  frame), or `"components"` (one per 4-connected opaque blob). New pure `src/marks.rs`:
  `connected_components` (iterative flood fill mirroring `tools/lint_sprite.py`'s opacity +
  4-neighbour notion), `assign_marks` (numbers 1..N; inverse is `marks[n-1]`), `draw_badge`
  (numbered badge over a neutral box, clamped on-canvas), reusing the one shared bitmap font
  from `gutter.rs`. No new plugin command — `slices`/`layers` reuse `list_slices`/`list_cels`
  ∩ `list_layers`; `components` is pure Rust. Layer visibility honours **effective** group
  visibility (a layer in a hidden group isn't marked) and disambiguates duplicate layer
  names (`Body`, `Body #2`); `components` runs CC at source resolution (reconstructed from
  the upscaled buffer, so it never touches the up-to-67M-px buffer); a `MAX_MARKS` cap keeps
  the largest regions and reports the total in `marks_truncated`. `finish_preview` filters
  regions to the crop window then numbers them (every mark has a visible badge, contiguous
  numbering) and draws each at `band + (centroid − crop)·scale`, returning `marks` even when
  empty ("requested, none found"). Unit-tested (CC disjoint/L-merge/empty, mark numbering +
  inversion, badge bounds/clamp, slice/layer parse, group-visibility, duplicate names,
  crop-window filter under a non-zero crop, marks-over-an-applied-gutter-band, truncation).
  112 unit tests pass; clippy adds no new lints.
- **SPEC-005 Phase 5 — plugin `0.3.2` advertises `perception2`.** The only plugin change
  across SPEC-005 is the Phase-2 `cel.bounds` report in `save_preview`, so the new
  `perception2` feature flag means "`crop="cel"` works"; the gutter / crop-math / inline /
  marks features are server-side and degrade loudly on an old plugin rather than being gated.
- **`live_frame_diff` — pixel-level diff of two frames as a text grid (Perception
  fast-follow, research Path 1).** Renders `from_frame` and `to_frame` (modal-free
  `save_preview`, 1×) and emits a one-glyph-per-cell grid: `.` = unchanged, `-` =
  erased (became transparent), otherwise the glyph of the **new** colour at that cell
  (with a glyph→`#rrggbb` legend) plus a changed-cell count. Lets the agent see
  EXACTLY what an edit changed, or where two animation frames differ at the pixel
  level (the verify half of the draw→see→fix loop). Validates frames in range and that
  they differ; restores the user's active frame. The pixels→diff transform is pure
  Rust in `src/ascii_view.rs::diff_to_ascii` (4 unit tests); refuses grids > 64×64
  (crop first). Live-verified on a 6-frame sprite (frame 1→3 = 131 changed cells,
  correct grid + palette legend).
- **SPEC-005 Phase 1 — `src/gutter.rs`: coordinate gutter compositor (Perception
  fast-follow, research Path 1 §A).** A pure-Rust margin band that labels the
  upscaled preview with **chunky numeric ticks** every 8 source-px along the top and
  left — VLMs are blind to grid geometry, but in-grid numeric labels roughly double
  row/col accuracy ([VLMs are Blind]). A built-in 3×5 bitmap font (no font dep); the
  label colour is the candidate **maximally distant in CIELAB ΔE** from the sprite
  palette *and* the band (reuses `color_ops`), so labels never read as art; and
  because the upscale factor is integer, any (x,y) the agent reads off the gutter
  **inverts back to an exact source coordinate** for `live_draw_pixels`. Refuses a
  tick density below the legibility floor (`step × scale < 24px`). **7 unit tests**
  (inversion identity, off-palette pick, density refusal, byte-faithful art blit).
  See SPEC-005 / research §A.
- **SPEC-005 Phase 1 — gutter wired onto `live_save_preview` (on by default).** The
  preview is upscaled to an in-memory buffer (`preview::render_preview_buffer`), then —
  whenever the tick spacing is legible at the chosen scale — composited with the
  coordinate gutter before the PNG is written. New `gutter` / `gutter_step` options:
  `gutter:false` suppresses it, `gutter:true` requires it (loud `gutter_unreadable`
  refusal if illegible), and the default quietly degrades to a plain preview with a
  `gutter_skipped` note. The result JSON gains `gutter_applied` plus a `gutter`
  `{left_w, top_h, step, image}` sidecar so any (x,y) read off the preview inverts
  exactly (`source = (preview − {left_w,top_h}) / scale`). The legibility floor now
  also rejects spacings where multi-digit labels would overlap, and the label colour
  is steered off the sprite's own sampled colours (`gutter::sprite_palette`). Pure
  helpers `live::finish_preview` + `gutter::sprite_palette` unit-tested (transparent
  art, explicit-require success/refusal, default degrade, write-failure, label-overlap
  refusal). No plugin change — works with any connected plugin version.
- **SPEC-005 Phase 2 — `live_save_preview` region crop (`crop`).** Focus the upscale
  budget on the subject: `crop:"sprite"` (whole canvas, default), `crop:"cel"` (the
  active cel's bbox — a 16×16 cel on a 256×256 canvas now fills ~1024px instead of
  ~64px), or `crop:{x,y,width,height}`. `render_preview_buffer` clamps the rect, crops
  the decoded RGBA, then auto-scales on the **crop's** long edge; `PreviewInfo` gains
  `crop_x/crop_y` and the sidecar a `crop:{x,y}`. The gutter draws labels in **absolute**
  sprite coordinates (`gutter::render_with_gutter_at`, origin = crop), so the agent reads
  the real (x,y) with no arithmetic. `crop:"cel"` resolves from a new read-only `cel`
  bounds field the plugin reports in `save_preview`; an empty cel or an old plugin is a
  loud `cel_bounds_unavailable` degrade (never a silent guess). Pure crop/validation
  helpers unit-tested (`clamp_crop`, `resolve_crop_plan`, `rect_to_crop`,
  `cel_crop_from_response`, crop-then-scale, full-crop no-regression, absolute-label
  origin). 87 unit tests pass; clippy adds no new lints. (Live-verify of `crop:"cel"`
  pending a plugin reload.)
- **SPEC-005 Phase 3 — `live_save_preview` optional inline image return (`inline`,
  [ADR-0007](docs/adr/0007-inline-image-content.md)).** `inline:true` returns the PNG as
  an MCP `image/png` content block (base64) so a vision client sees the pixels directly,
  not just a path — the first tool in the crate to emit image content (`live_save_preview`
  now returns `Result<CallToolResult, McpError>`). The path is always present too, so the
  auto-preview hook and non-vision clients are unchanged (the no-inline wire shape is
  byte-identical). A preview over the 1 MiB ceiling degrades to path + a text note rather
  than blowing the context budget. Pure `preview::read_inline_png` → `InlinePng::{Ready,
  TooLarge}` + a hand-rolled RFC 4648 `base64_encode` (no new dependency); unit-tested
  (known-vector encode, round-trip decode to dimensions, size-guard). 89 unit tests pass;
  clippy adds no new lints.

  [VLMs are Blind]: https://arxiv.org/abs/2407.06581
- **SPEC-004 Phases 2–4 — live constrained/semantic colour tools (Path 2).** Three
  new `live_*` tools that make every colour operation legal by construction:
  `live_palette_snap` (snap a layer/selection's off-palette colours to the nearest
  CIELAB palette colour), `live_adjust_pixels` (shade by INTENT —
  darken/lighten/hue_shift/colorize, with darken/lighten applying the project
  hue-shift rule and `clamp_to_palette` on by default), and `live_snap_colors`
  (snap a hex list to the active palette WITHOUT editing — legalise a stroke before
  `live_draw_pixels`). The colour math is the pure `color_ops` core; the tools fetch
  a region's *unique* colours (new plugin `get_region_colors`), build a colour→colour
  map in Rust, and apply it in one undoable pass (new plugin `apply_color_map`,
  clone→mutate→reassign). RGB sprites only (v1). Plugin advertises
  `features += ["color_ops"]` (v0.3.0); old plugins reject the commands per-command
  (ADR-0005). Live E2E pending an Aseprite run.
- **SPEC-004 Phase 1 — `src/color_ops.rs`: pure constrained/semantic colour core
  (Path 2).** Real **CIELAB + CIEDE2000** palette snapping (validated against the
  Sharma reference pairs) — the honest version of the competitor pixel-mcp's
  claimed-but-RGBA "LAB snap"; plus intent ops `darken`/`lighten` (value shift **+**
  the project hue-shift rule: shadows cool toward blue, highlights warm toward
  orange), `hue_shift`, `colorize`, `clamp_to_palette`, and `build_color_map` (the
  per-unique-colour map the live tools will apply). Pure Rust, **11 unit tests**
  incl. a brute-forced "LAB nearest ≠ RGBA nearest" proof and the darken-cools /
  lighten-warms direction. Live tools (`live_palette_snap`, `live_adjust_pixels`,
  `live_snap_colors`) are SPEC-004 Phases 2–4. See SPEC-004 / research §B,§D.
- **`live_save_filmstrip` — review animation in one image (Perception
  fast-follow, research Path 1).** Composites every frame into a near-square
  row-major grid (gray gaps between cells), nearest-neighbor upscaled toward
  ~1024px. The Claude API only reads the *first* frame of an animated GIF, so a
  strip is the only way to review a walk/attack cycle and the #1 animation failure
  (cross-frame proportion drift). Renders each frame via the modal-free
  `save_preview` and restores the user's active frame; the frames→grid compositor
  is pure Rust in `src/filmstrip.rs` (5 unit tests). Live-verified on a 6-frame
  sprite (3×2 grid, 1040×702).
- **`live_ascii_view` — text-grid readback of the active frame (Perception
  fast-follow, research Path 1).** One glyph per pixel (`.` = transparent) with
  tens/units column rulers, row labels, and a glyph→`#rrggbb` legend. LLMs read a
  one-token-per-cell grid far more reliably than a small sprite image (Text2Space),
  so this is the agent's exact, token-space view for VERIFYING pixel values /
  positions — and it works for non-vision clients. Reuses the modal-free
  `save_preview` 1× render; the pixels→text transform is pure Rust in
  `src/ascii_view.rs` (4 unit tests). Refuses sprites > 64×64 (crop first).
- **SPEC-003 tilemap / tileset / autotile tool family (Phases 1, 2, 5).** Seven new
  live tools: `live_create_tilemap_layer`, `live_list_tilesets`, `live_get_tileset`
  (with a vision-legible upscaled packed PNG), `live_stamp_tiles` (the tile-grid
  analogue of `live_draw_pixels`), `live_set_tile_data`, `live_pack_similar_tiles`
  (dedupe a painted mockup into a tileset + reconstructing tilemap), and
  `live_export_tileset` (Tiled `.tsj` with a blob47 wangset, Godot `.tres`, or JSON
  + a sibling packed PNG; whole-canvas). Engine-format serializers are pure Rust in
  `src/tileset_export.rs` (9 unit tests, reuses the Phase-3 `autotile` blob47 order);
  tile CRUD/dedupe are new `plugin.lua` handlers. The wire protocol stays v1; the
  plugin advertises `features=["tilemap"]` and old plugins reject the new commands
  loudly per-command (ADR-0005). `scripts/smoke/tilemap-selftest.lua` exercises the
  Aseprite-side primitives for the live E2E check. Joins the already-landed Phase 3
  (`src/autotile.rs` blob-47 bitmask) and Phase 4 (`tools/seam_lint.py`).
  Live-verified end-to-end on Aseprite 1.3.17.2: paint mockup → `pack_similar_tiles`
  (16 cells → 2 unique tiles, pixel-faithful) → `get_tileset` vision preview →
  `stamp_tiles` (overwrite + fill cells, confirmed by render) → `set_tile_data` →
  export Tiled/Godot/JSON + blob47 wangset (grid round-trips exactly). **Three bugs
  the live run surfaced and fixed** (plugin 0.2.3): (1) `stamp_tiles` sent the nested
  `LiveTile.tile_index` as snake_case on the wire — now remapped to `tileIndex`;
  (2) `create_tilemap_layer`/`pack_similar_tiles` anchor onto a non-tilemap layer
  before `NewLayer{tilemap=true}` so the new tileset takes the requested tile size
  instead of inheriting the active tilemap's grid; (3) JSON numbers decode to Lua
  floats and `Image:putPixel` writes a float tile index as the empty tile 0 — the
  stamp/rebuild path now `math.floor`-coerces tile indices to integers.
- `live_save_preview` tool + auto-preview hook rewired to it: saves a faithful 1×
  copy, then nearest-neighbor upscales it in the Rust server (live document
  untouched) so the sprite's long edge lands near ~1024px. Raw 1× previews of
  16–64px sprites are below the resolution a vision model can read reliably, so
  this is the perception half of the agent's see→fix loop (research doc Path 1).
  Pure-Rust image math in `src/preview.rs` (new `image` png-only dep), 6 unit tests.

### Fixed
- **Adversarial-audit follow-ups** (no behaviour-changing bugs were found; these
  harden error reporting + a documented contract):
  - Colour ops: `selection_only=true` with **no active selection** now returns an
    `empty_selection` error instead of silently recolouring the whole layer; a
    group/tilemap target now returns a clear `not_an_image_layer` error instead of
    a confusing "0 colours changed" no-op (plugin returns an `imageLayer` flag);
    `adjust_pixels(op=snap)` now requires a palette even when `clamp_to_palette=false`.
  - `live_ascii_view`: the size cap is now a true **per-edge 64×64** check (was a
    4096-*cell* area cap, which let a 256×16 sprite through and produced an
    unreadable 256-glyph row) — matches the documented "64×64" contract.
  - `live_set_tile_data` description corrected: tile user-data is stored in the
    `.aseprite` file and read back by `live_get_tileset`, but is **not** emitted by
    `live_export_tileset` (Tiled wangsets come from the blob47 layout).
  - Removed dead `get_or_create_tilemap_cel` (obsolete after the stamp rewrite).

## v0.1.0 — 2026-06-10

First tagged release: the Claude Code pixel-art plugin for Aseprite — live MCP
drawing, encoded rules, `/pixel-*` skills, review/rig/palette/animation agents,
live-first hooks, 3-OS CI quality gates. Checklist v18 ≈95.8/100.

### Added
- Standalone `aseprite-live-bridge` singleton (decoupled WS bridge, ports 9876/9877)
  so MCP server restarts never drop the Aseprite plugin connection (SPEC-001, ADR-0002).
- Full live tool surface (`live_*`) for sprite, layer, frame, cel, drawing, tag,
  slice, selection, palette, and app-command workflows, with `live_preflight` guard.
- Claude Code plugin packaging: `.claude-plugin/plugin.json`, marketplace manifest,
  `mcp/aseprite-live.json`, install/uninstall scripts, QUICKSTART + ARCHITECTURE docs.
- Pixel-art expertise pack: `rules/` rulebook, `knowledge/` palettes/glossary/references,
  `/pixel-*` skills, and pixel-critic / palette-smith / rig-builder / animation-director agents.
- Hooks: live-first batch-draw guard, session health check, palette-lint on save,
  auto-preview export (`mcp_tool` PostToolUse).
- Quality gates in CI (3-OS): Rust unit + schema-contract tests, sprite linter,
  visual-regression golden diff, Tier-A eval harness, hook contract tests,
  install verification, packaging manifest validation.
- Security: `ASEPRITE_MCP_ALLOW_LUA` opt-in gate for `run_lua_script` (SPEC-002,
  ADR-0003), loopback-only bridge binding test, `SECURITY.md`.
- `export_spritesheet` now emits `meta.frameTags` in the JSON data by default
  (`--list-tags`), with opt-in `list_layers` / `list_slices` (closes Tier-B 5.4 gap).
- MIT `LICENSE` file; Dependabot config for cargo + GitHub Actions.

### Changed
- Hardened Lua plugin error shape and self-healing reconnect (no Aseprite restart needed).
- Live protocol capabilities reporting and namespaced request IDs.

### Fixed
- Tool JSON-Schema validity (`params` boolean-schema regression) with contract tests.
- Spurious `live_timeout` during unfocused Aseprite periods.
