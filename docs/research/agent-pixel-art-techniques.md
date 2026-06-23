# Agent-driven pixel art: research, strategy & roadmap

> **What this is.** The consolidated, self-contained report of a research +
> strategy session (2026-06-12/13) on the central question: *how can this plugin
> make a coding agent (Claude Code / Cursor / Codex / Copilot) draw like a real 2D
> pixel artist?* It captures everything researched and discussed so the project can
> act on it later without re-deriving context. Companion to
> [`../aidd/PROJECT_PLAN.md`](../aidd/PROJECT_PLAN.md); roadmap items below are
> candidates for `specs/` and the `COMPLETENESS_CHECKLIST.md`.
>
> **How to read it.** §1 methodology & trust levels · §2 the strategic answer
> (capability paths + feasibility + hard limits) · §A–§G the verified evidence
> base by theme · the prioritized roadmap, verification corrections, open
> questions, and a full source appendix follow.
>
> **Provenance.** Multi-agent web sweep (Reddit, X/blogs, GitHub, blender-mcp,
> other creative MCPs, generation-technique literature) → adversarial
> verification at the source → completeness critic → 4 targeted gap-fill sweeps.
> 68 raw findings → 57 deduped → **14 deep-verified at code/source level, 0
> refuted**, plus **24 gap-fill findings** on perception, reference→style,
> in-editor generation, and tilesets (§A, §E, §F, §G). Dated 2026-06-12. Sources
> are listed inline and collected in the appendix.

## 0. Thesis

The community has converged on one fact: **LLMs cannot draw organic art from
nothing, but they are strong at cleanup, recolor, batch edits, animation timing,
and export.** Every successful project organizes around three pillars:

1. **Give the agent working eyes.** Most MCP previews (ours included) ship raw
   1× PNGs that are literally below the resolution a vision model can read.
2. **Hybrid generation.** A model (diffusion / video) supplies the organic base;
   the agent supplies discipline (palette, grid, lint, timing).
3. **Assets-first**, like blender-mcp: find / import / restyle existing assets
   rather than draw from scratch.

A recurring competitive signal: **two of the strongest competitor's headline
features are partly fake in their own source** (see §D), so re-implementing them
honestly is a real differentiator, not catch-up.

---

## 1. Methodology & trust levels

**How the research was produced.** A deterministic multi-agent workflow
(`Workflow` orchestration) ran in five phases:

1. **Sweep** — 6 parallel research agents, one per angle: Reddit
   (r/aseprite, r/PixelArt, r/ClaudeAI, r/gamedev…), X/blogs/YouTube, the GitHub
   ecosystem (other Aseprite MCPs + community Lua), a blender-mcp deep dive, other
   creative-tool MCPs (Godot/Unity/Figma/Photoshop/Krita/GIMP), and the
   pixel-art generation-technique literature.
2. **DeepRead** — the 14 most promising findings sent to independent agents that
   *opened the source* (repo code, paper, blog) and adversarially confirmed or
   refuted the claim, extracting concrete detail (exact tool names, algorithms,
   parameters) and an adoption sketch naming this repo's real components.
3. **Critic** — a completeness agent flagged 4 missing angles.
4. **Gap-fill** — 4 more sweeps closed them (perception, reference→style,
   Retro Diffusion, tilesets), adding 24 findings.

Totals: **68 raw → 57 deduped → 14 deep-verified (0 refuted) + 24 gap-fill.**
The run survived several daily session-limit interruptions via journal-based
resume (completed agents returned from cache; only failed/new agents re-ran), so
no verified work was lost or duplicated.

**Trust levels — read every claim against these:**

| Tier | Meaning | How to treat it |
|------|---------|-----------------|
| **Deep-verified** | An agent opened the primary source and confirmed it at code/paper level; corrections noted inline. | Safe to act on; the cited specifics are real. |
| **Swept** | Surfaced and summarized from a real source seen in results, but not independently re-opened. | Credible; re-open the source before implementing load-bearing detail. |
| **Gap-fill** | From the 4 targeted gap sweeps; source-summarized, not independently verified. | Same caution as *swept*. |

When a finding's headline differed from its own source, verification caught it —
see **"Corrections surfaced during verification"** near the end. Those are the
load-bearing trust adjustments (e.g. a competitor's "LAB snapping" is RGBA in
code; SwordsBench's "empirical" claims are the author's hypotheses).

---

## 2. The strategic answer — capability paths to "real pixel art"

**Core finding, stated plainly:** there is **no single setting that makes one LLM
free-hand professional organic pixel art.** That ceiling is a model-capability
limit, not a tooling gap (donut test, SwordsBench, MindStudio consensus). "Real
pixel art" is reachable, but only as a **system** in which the LLM is *director +
finisher*, not the brush. The question is therefore *which capabilities to stack*,
not *how to make the agent draw better*. Six independent-but-composable paths:

### Path 1 — "See correctly, then fix" (perception + critique loop)
- **What.** Make the agent actually *see* its work — upscale preview to ~1024 px,
  labeled coordinate gutter, Set-of-Mark numbered regions, before/after composite —
  closed with an objective linter. Detail in §A.
- **Feasibility: VERY HIGH.** Cheap, native, independent. The *enabling condition*
  for every other path — today the auto-preview hook ships 1× PNGs a vision model
  effectively cannot read.
- **Raises the ceiling to:** "acceptable for small/simple sprites, icons, and
  edits," and multiplies the quality of all other paths.
- **Limitation.** Doesn't address organic complexity; VLMs stay weak at fine
  localization.
- **Mitigation.** Route spatial truth (stray pixels, palette, symmetry) to the
  deterministic linter; use the VLM for taste only; Set-of-Mark instead of raw
  coordinates.

### Path 2 — "Constrained / semantic drawing"
- **What.** Make every stroke legal by construction: tool-layer `palette_snap`
  (real LAB), intent ops (`live_adjust_pixels`: darken/hue-shift/colorize clamped to
  palette, à la Magic Pencil), SVG/DSL for silhouettes, free symmetry via
  `scale(-1,1)`. Detail in §B/§D.
- **Feasibility: HIGH.** Small Rust algorithms. Bonus: the leading competitor's
  "LAB snapping" and "8 light directions" are **fake in their own source**, so an
  honest implementation *surpasses* them.
- **Raises the ceiling to:** the actual "pixel-art look" for hand-drawn work —
  clean palette, correct ramps, symmetric forms; turns the linter from catcher into
  non-issue.
- **Limitation.** Still needs a correct mental image of the composition; legal ≠
  beautiful/anatomically right.
- **Mitigation.** Combine with Path 4 (reference) for the mental image, Path 1 to
  see the result.

### Path 3 — "Hybrid generation"
- **What.** A model supplies the organic base (Veo video / diffusion / Retro
  Diffusion) → the agent regrids, quantizes, cleans, refines. The only route to
  genuinely organic characters/creatures. Detail in §C.
- **Feasibility: MEDIUM–HIGH.** Free default path (`magick -remap` / K-Centroid
  Lua via `run_lua_script`, plus *user-supplied video*); optional paid path (Retro
  Diffusion **in-editor**, painting onto the AI-draft layer at native res/palette,
  sidestepping regrid).
- **Raises the ceiling to:** the highest — professional-looking organic sprites;
  the video→frames→trace pipeline fixes cross-frame drift.
- **Limitation.** Paid/network deps; output always needs cleanup; pseudo-pixel-art
  has AA/off-grid edges; live-bridge focus/connection.
- **Mitigation.** Ship the no-API path as default; linter + pixel-critic as a hard
  QA gate; correct regrid algorithm is **Sobel edge profiles + histogram voting**
  (not autocorrelation, as an early sweep mislabeled it).

### Path 4 — "Reference & style grounding"
- **What.** Trace over a *locked reference layer* (video frame, CC0 asset, or user
  image) + a machine-checkable `StyleProfile` contract
  `{grid, palette, ramps, outline_policy, light_dir, heads_tall, frame_counts}`.
  Turns "draw a goblin matching my hero sheet" into a deterministic task. Detail in
  §G.
- **Feasibility: MEDIUM–HIGH.** The steps (normalize → extract palette → ramp-sort
  → derive profile) are textbook, native-Rust.
- **Raises the ceiling to:** beats the hardest weakness — drawing organic shapes
  from text — by *matching/tracing* instead of *inventing*; makes imported/generated
  assets look native (the 2D analog of Figma "named tokens, not hex").
- **Limitation.** Needs a reference to exist; style-extraction heuristics imperfect.
- **Mitigation.** CC0 libraries (HF OpenGameArt 15.7k, Lospec API) to *source*
  references; 3 extraction methods; ramp-lint as an objective axis.

### Path 5 — "Structured assembly" (tiles, cutout animation, rigs)
- **What.** Play to LLM strengths — deterministic, structured work: tilesets via
  bitmask templates (draw ~5 minitiles → assemble 47), cutout animation via
  rotsprite/tween, skeleton-pose animation. Detail in §E.
- **Feasibility: MEDIUM–HIGH.** Deterministic algorithms (8-neighbor bitmask
  256→47, `rotsprite` crate, blobator to port). Ecosystem blind spot — no MCP
  exposes tilemaps despite 2D games being "mostly tiles."
- **Raises the ceiling to:** *deterministic* real game art exactly where freehand
  fails; the asset class the game-dev audience needs most (and a competitor already
  monetizes).
- **Limitation.** Applies only to structured asset classes, not freeform creatures.
- **Mitigation.** Combine with Path 3/4 for the organic pieces (draw the minitile /
  keyframe); seam-lint as a hard eval gate.

### Path 6 — "Objective validation harness"
- **What.** Not a drawing method but what makes the others *trustworthy*: the
  linter as a **hard gate**, SwordsBench-style evals, seam-lint, ramp-lint,
  long-session degradation tests. Detail in §F/§A.
- **Feasibility: HIGH.** The repo already has the rubric machinery in
  `evals/tier_b.json` + `judge.py`.
- **Why mandatory.** The donut test proved that as context fills, the model
  "congratulates itself on a great image that objectively has problems." An
  objective validator is the only antidote — precisely when it matters most.
- **Limitation.** Measures "correct/clean," not "beautiful."
- **Mitigation.** Split axes — linter for correctness, upscaled+SoM pixel-critic
  for aesthetics.

### Deliverable → recommended path stack (with realistic ceiling)

| Deliverable | Stack | Realistic ceiling |
|---|---|---|
| Icon / small item (16–32px) | 1 + 2 + 6 | **High** — constrained freehand + correct sight is enough |
| Character / creature | 3 (gen base) → 4 (style) → 2 (clean) → 6 | **High** *with* API or reference; pure freehand = low ceiling |
| Character animation | 3 (video→frames) or 5 (cutout/rig) → 1 (film-strip review) → 6 | **Medium–high**; GIF review is useless (Claude sees only frame 1) → film-strip mandatory |
| Tileset / level art | 5 (bitmask template) → seam-lint → engine export | **High & deterministic** — the clearest strength |
| Palette-swap / variant | 2 + 4 (ramp-map) | **Very high** — recolor/batch is where LLMs reliably win |

### Feasibility verdict & hard limits

**Achievable:** a plugin that produces *real, game-usable* pixel art is feasible —
as a **system** (generate + constrain + reference + assemble + validate) with the
LLM as director/finisher. Most steps are **native-Rust deterministic**, no API
required.

**Hard limits that remain (honest):**
1. **Pure freehand organic** will never reach artist quality — a model ceiling, not
   a tooling one. *Mitigation: don't ask it to; route through Path 3/4.*
2. **VLM fine localization is weak.** *Mitigation: linter for truth, VLM for taste.*
3. **Live bridge needs one focus after a real disconnect** (documented elsewhere) —
   doesn't block most workflows.
4. **The best generation routes are paid/network.** *Mitigation: no-API default.*

**Single biggest lever to start:** **Path 1 (perception)** — the agent currently
can't even score its own work correctly; every other path multiplies after it can
see. That is why the roadmap ranks the preview overhaul #1.

---

## A. Perception engineering — why LLMs draw pixels badly, and what works

This is the highest-leverage cluster: it *explains* the failures and the fixes
are cheap.

- **Read/write asymmetry.** [Text2Space](https://arxiv.org/html/2604.14641v1)
  measured every model reading ASCII grids far better than writing them
  (~58% vs ~44%). → Invest in *read-back* tooling for verification; never let the
  agent imagine canvas state.
- **One token = one pixel.** [LLMs Can't See Pixels or Characters (B. Long,
  Anthropic)](https://www.lesswrong.com/posts/uhTN8zqXD9rJam3b7/llms-can-t-see-pixels-or-characters):
  vision encoders patch images (14×14 / 16×16 / 32×32 px), so a 16×16 sprite
  straddling patch boundaries is reconstructed from partial patches; an ASCII
  grid with exactly one token per cell solved an ARC puzzle the screenshot
  failed. Recommends: rescale so one logical cell aligns to a patch, and a
  zoom/crop tool.
- **Claude's patch math (verified at source).**
  [Anthropic vision docs](https://platform.claude.com/docs/en/docs/build-with-claude/vision):
  images are tokenized in 28×28 patches; anything < 200 px hallucinates. A raw
  32×32 preview ≈ 4 patches of signal — effectively invisible. → Upscale
  nearest-neighbor so the canvas lands near 1024–1536 px; for a 32×32 sprite a
  clean 32× integer scale = 1024 px is the sweet spot, and integer scale lets the
  harness divide back to exact (x,y).
- **VLMs are blind to grid geometry.**
  [VLMs are Blind](https://arxiv.org/abs/2407.06581): counting grid rows/cols
  ≈ 47%, but **text labels inside the grid roughly double accuracy**. → Print
  numeric row/col labels in a gutter every 8 px; use chunky 8-px guides, never
  1-px hairlines (worse than useless).
- **Animation review needs a single composite image.**
  [IG-VLM](https://arxiv.org/abs/2403.18406): 6 frames in a near-square 3×2 grid
  + a prompt explaining frame order beat dedicated video pipelines. Critical
  corollary: **the Claude API only sees the first frame of an animated GIF** — so
  any "review this GIF" loop is silently broken.
- **Long-session self-deception.**
  [Donut test](https://www.mindstudio.ai/blog/claude-blender-mcp-60-percent-tokens-donut-test-results):
  as context fills, the model "congratulates itself on a great image that
  objectively has problems." The only antidote is an **objective validator in the
  loop** — validating our bet on the linter, but it must be a *hard gate*.
- **Harness lessons from a tiny pixel screen.**
  [ClaudePlaysPokemon](https://michaelyliu6.github.io/posts/claude-plays-pokemon/):
  pair every screenshot with machine-derived text ground-truth; give a crop/zoom
  tool; choose annotation colors with neutral semantics (a red marker on a sprite
  with red pixels confused the model); let the critic place persistent named
  markers that re-render across iterations.
- **Drawing via code/DSL beats coordinate emission.**
  [LTD-Bench (NeurIPS)](https://arxiv.org/pdf/2511.02347),
  [DrawingBench](https://arxiv.org/pdf/2512.01174),
  [SVGenius](https://arxiv.org/html/2506.03139v1): LLMs emit SVG paths/polygons/
  transforms far better than pixel coordinates; symmetry mirroring is free via
  `scale(-1,1)` — directly fixes the asymmetric-face failure.
- **SwordsBench** (see §D for the verified scope caveats): the dominant failure
  in agent-drawn animation is **cross-frame proportion drift**.

Gap-fill additions (more specific, all verified-during-sweep):

- **Optimal grounding scale ≈ 1000 px.**
  [AdaZoom / MEGA-GUI](https://arxiv.org/html/2511.13087): VLMs ground most
  accurately when the *target* occupies ~1000 px; crop the cel bbox first, then
  upscale (nearest-neighbor for pixel art) so the *sprite*, not the whole canvas,
  fills that budget.
- **Set-of-Mark beats free-form coordinates.**
  [SoM](https://arxiv.org/abs/2310.11441): overlay *numbered marks* on regions and
  have the critic reference them by number. Pixel art segments for free — by layer,
  by Aseprite slice, or by the linter's connected-component output (no SAM needed).
  `pixel-critic` returns "region 3 (weapon) has a stray pixel" and the orchestrator
  maps mark→layer/cel deterministically — sidesteps VLM coordinate weakness.
- **Coordinate-labeled margins.**
  [SketchAgent](https://arxiv.org/html/2604.22875v2): axis ticks (0,4,8,12…) up the
  left and along the bottom of the upscaled sprite let the agent name exact (x,y)
  that map back to `live_draw_pixels`. Ship it as an *optional* second preview
  variant and let the eval harness pick the winner per model.
- **Don't ask the model questions it provably can't answer.**
  [GPT-4V directional dyslexia](https://towardsdatascience.com/gpt-4v-has-directional-dyslexia-2e94a675bc1b/):
  VLMs are weakest at exactly the fine directional/position reasoning pixel-art
  critique needs. → Keep stray-pixel / palette / symmetry checks on the
  **deterministic linter**; use the VLM for taste, not pixel-accurate localization.
  Bake a "don't trust unscaled-preview spatial claims" clause into the critic prompt.
- **Before/after diff + checkerboard alpha.**
  [VLM resolution curse](https://huggingface.co/blog/visheratin/vlm-resolution-curse):
  emit a labeled `before | after` composite so the critic judges the *delta*, and
  render transparent sprites over a checkerboard so AA fringe / accidental
  transparency become visible (the defects the linter already flags).

## B. Workflow doctrine — patterns from blender-mcp & other creative MCPs

- **Tiny structured surface + assets-first.** blender-mcp
  ([server.py](https://github.com/ahujasid/blender-mcp)) exposes only ~4 core
  editor tools + ~18 asset-acquisition tools; no per-feature tools. Its viral
  demos are *scene assembly from libraries*, not modeling from primitives. Our
  ~150 flat tools are the opposite design and past the size where tool-selection
  errors appear. → Ship **tool profiles / groups** (drawing, animation, palette,
  export) ([CoplayDev unity-mcp](https://github.com/CoplayDev/unity-mcp),
  [GoPeak](https://github.com/HaD0Yun/Gopeak-godot-mcp)).
- **Doctrine as an MCP prompt (verified verbatim).** blender-mcp's
  `@mcp.prompt() asset_creation_strategy()` hard-orders the workflow: inspect →
  check integrations → prefer assets → generate → **screenshot BEFORE and AFTER**
  → script only as last resort. We have all the pieces (live_preflight, rules/,
  skills, AI-draft layer, knowledge/palettes) but the doctrine is scattered in
  skill files. → Ship a **`pixel_creation_strategy` MCP prompt** from the Rust
  server so Cursor/Codex/Copilot get it too, not just Claude-Code-skill users.
- **Escape-hatch discipline (verified).** blender-mcp keeps `execute_blender_code`
  but the chunking instruction lives **in the tool docstring** (every client sees
  it), with "ALWAYS save first" and verbatim error return for self-repair. Our
  gated `run_lua_script` is already stricter; adopt: docstring chunking guidance,
  auto-snapshot before each script, a **harvest loop** (log repeated scripts →
  promote to first-class tools, the
  [adb-mcp](https://github.com/mikechambers/adb-mcp) pattern).
- **Transaction-scoped undo.** Wrap each agent tool batch in
  `app.transaction("agent: <intent>")` so the human watching the live window gets
  exactly one Ctrl+Z per agent action — big trust win for the live-editing story
  ([Unity MCP](https://medium.com/@jengas/advanced-agentic-game-development-in-unity-with-mcp-5add91c579e9),
  [krita-mcp](https://github.com/nanayax3/krita-mcp)).
- **Adoption ≫ capability.**
  [pixel-plugin](https://github.com/willibrandon/pixel-plugin) (Claude Code plugin
  packaging + trigger-keyword-dense skill descriptions) has **208 stars — more
  than any actual Aseprite MCP server**. → Package as an installable plugin; add a
  `/pixel-doctor` command that checks Aseprite path + bridge + **registered binary
  version** (would have caught our stale-binary reconnect bug).

## C. Hybrid generation pipelines (how real AI pixel art is made today)

1. **Video → frames → trace** (deep-verified: blog + repo).
   [Mike Veerman "Claude After Dark"](https://mikeveerman.be/blog/github-2026-01-17-claude-after-dark/):
   per-frame image generation drifts the character every frame; a **Veo 4-second
   video on a #00FF00 background** keeps it consistent. `ffmpeg -vf fps=N/4`
   extracts frames; adaptive PIL chroma key (`green_dominance = g - max(r,b)`;
   > 20 → transparent). → Skill `pixel-reference-motion`: load extracted frames as
   a **locked reference layer (~50% opacity) per frame**, agent traces clean
   pixels over a motion-consistent reference — fixes SwordsBench's cross-frame
   drift. **Accept a user-supplied .mp4 too** (zero API cost; many users have no
   Gemini key). Chroma thresholds must be CLI-configurable (tuned to one clip).
2. **Regrid "fake" AI pixel art.** Diffusion output is 1024 px that is "really"
   64×64 off-grid. Field-proven algorithm (corrected during verification — *not*
   "edge autocorrelation"): **Sobel edge profiles per row/col → histogram-vote the
   dominant cell spacing → snap → quantize**
   ([unfake.js](https://github.com/jenissimo/unfake.js/), Rust core +
   [deep-dive](https://dev.to/jenissimo/how-to-tame-your-ai-pixel-art-3pk5);
   [proper-pixel-art](https://github.com/KennethJAllen/proper-pixel-art)).
   Fastest in-editor route: **[K-Centroid](https://github.com/Astropulse/K-Centroid-Aseprite)
   is pure Lua MIT — runs in Aseprite via our gated `run_lua_script` today**.
   Cheapest v1: a one-line `magick ref.png -resize 64x64 -dither None -remap
   palette.png` in a skill, no server change.
3. **Pixel-native generation APIs (optional, paid).**
   [Retro Diffusion](https://github.com/Retro-Diffusion/api-examples) — only model
   trained for pixel art, takes `input_palette` to **lock colors at generation**,
   outputs true native resolution. Three routes, increasing integration:
   (a) **In-editor extension** (best fit — we own the bridge into the same Aseprite
   UI): `live_ensure_ai_draft_layer` → set selection → trigger RD's generation via
   `live_run_app_command` / a thin gated `run_lua_script` so it paints onto the
   draft layer **at the open sprite's exact size and palette** — sidesteps
   regridding entirely; (b) **HTTP API** (`POST /v1/inferences`, `X-RD-Token`,
   `input_palette` lock, `input_image`+`strength` img2img, `remove_bg`, async poll)
   for a headless path that needs no focused GUI; (c) **Replicate-hosted** backend
   (rd-fast/plus/tile/animation) for users with that account — `rd-tile` is the
   only surfaced route that generates **seamless tilesets**. RD's own animation
   models (starting-frame → idle/walk/jump/attack, fixed frame counts 4–16) map
   onto our frames/tags/cels CRUD. All paid/BYO-key → gate on key presence, and
   every generated layer still goes through the linter + pixel-critic.
   [PixelLab](https://github.com/pixellab-code/pixellab-mcp) — 4/8-direction
   characters, skeleton-based animation. Verification correction: PixelLab's API
   is **synchronous base64, not the async generate→poll→import** of blender-mcp;
   size limits are real (animate 64×64, BitForge ≤200×200). Worth doing
   independently first: a **palette-quantizing import** onto the AI-draft layer.
   Borrow the **skeleton/pose representation** for rig-builder regardless (poses
   are LLM-friendly; raw limb pixels are not).
4. **Anti-flicker for AI animation** (verified code).
   [sarthakmishra.com](https://sarthakmishra.com/blog/building-animated-sprite-hero):
   quantize all frames against **one shared palette** (`dither=NONE`), then a
   static-region mask pins unchanging pixels to their temporal **mode color**
   across the loop. The two load-bearing constants (`CHANGE_THRESHOLD`,
   `get_mode_color`) are omitted in the post — tune ours (~10–16/channel) and keep
   the auto-fix opt-in (it can kill intentional 1-frame accents: blinks, glints).
5. **Palette is the glue.**
   [Void Balls postmortem](https://bigdevsoon.me/blog/building-games-with-ai-indie-game-dev-workflow/):
   "the 7-color palette constraint was crucial — without it AI art looks like a
   random collage." → Enforce palette **at the tool layer**, not just post-hoc
   lint.

## D. Competitor & community tool mining

- **pixel-mcp** ([repo](https://github.com/willibrandon/pixel-mcp), 85★, Go,
  batch CLI). Verification found **its two headline features are overstated in its
  own code**:
  - "LAB palette snapping" — draw-time snap is plain **RGBA squared-euclidean** in
    generated Lua; LAB exists only in the offline analysis path.
  - "8 light directions" in auto-shading are **functionally dead** — the light
    vector is computed but the surface normal is hardcoded `(0,0,1)` and the dot
    product is never used; output is identical for all 8 directions.
  → A **real LAB snap** and **real light-direction shading** in our Rust server
  would genuinely surpass them. Also worth taking: dither-pattern fill,
  `analyze_reference` (palette + value map from an image), AA-jaggy detector
  (suggest-only by default — it flags intentional 45° steps).
- **Aseprite MCP Pro** ([itch, $10](https://y1uda.itch.io/aseprite-mcp-pro);
  [Lua extension](https://github.com/youichi-uda/aseprite-mcp-pro), MIT). **Same
  WebSocket + Lua-extension architecture as ours** (validates the design; server
  is proprietary). Categories we lack: **tilemap** (5 cmds), **Godot export**
  (SpriteFrames `.tres`, AtlasTexture, TileSet), **analysis** (`compare_frames`,
  `visual_diff`, `validate_animation`), Lospec fetch, `interpolate_frames`. A
  sibling godot-mcp-pro pairs for a full pipeline — users want art-MCP + engine-MCP
  combos.
- **Magic Pencil** ([itch](https://thkaspar.itch.io/magic-pencil); source in
  [thkwznk/aseprite-scripts](https://github.com/thkwznk/aseprite-scripts), no
  license → reimplement). Verified algorithms for **semantic paint**: Shift
  (HSV/HSL ± by %), Colorize (set hue, average saturation), Indexed-Mode snap.
  → A `live_adjust_pixels(op=darken|hue_shift|colorize, amount, clamp_to_palette)`
  lets the agent shade *by intent* and makes palette violations impossible —
  attacks the #1 failure (agent hand-computing slightly-wrong shade colors).
- **Sprite Analyzer** (same repo, verified). Renders a sprite as **value map**
  (Rec.601 luma), **silhouette** (alpha), **outline** (declared color set, *not*
  edge detection), **color blocks**. → A `live_save_analysis_views` tool saves 2–3
  breakdown PNGs alongside the preview so pixel-critic judges value/silhouette
  readability the way human artists do.
- **Ready-made Rust crates / small algorithms:**
  [rotsprite](https://docs.rs/rotsprite) (artifact-free rotation, no new colors →
  unlocks cutout animation), [libimagequant](https://github.com/ImageOptim/libimagequant)
  (pngquant engine, pure Rust), pixel-perfect L-shape cleanup (~50 lines),
  Scale2x/Eagle palette-safe upscale.

## E. Tilemap / autotile — an ecosystem-wide blind spot

No Aseprite MCP exposes tilemaps (ours reads only an `isTilemap` flag), yet they
are core to 2D games.

- Aseprite 1.3's Lua API treats a tilemap cel as an image whose "pixels" are tile
  indices — `putPixel(col,row,index)` — so tile placement **reuses our
  `live_draw_pixels` plumbing almost verbatim**
  ([tileset API](https://www.aseprite.org/api/tileset)).
- [Pack Similar Tiles.lua](https://github.com/aseprite/Aseprite-Script-Examples/blob/main/Pack%20Similar%20Tiles.lua)
  (official) turns a painted mockup into a deduped tileset + index map — the agent
  paints freely (its strength), the tool emits the tileset for free.
- The algorithmic core is an **8-neighbor bitmask with corner masking** (a diagonal
  counts only if both adjacent cardinals are filled), which collapses 256 → the 47
  canonical blob states
  ([Red Blob Games](https://www.redblobgames.com/articles/autotile/)).
  [BorisTheBrave's taxonomy](http://www.boristhebrave.com/permanent/24/06/cr31/stagecast/wang/blob.html)
  maps minimal artist inputs (13/15/20 tiles, or **4 corner quarters**) to the full
  16/47 sets — belongs in `knowledge/` so a tileset skill knows what minimal input
  to ask for. The **4-corners-per-tile compositing** model is the LLM-friendly path:
  draw ~5 inputs once, assemble all 47 by blitting quarters via the live bridge
  instead of hand-placing 47 full tiles.
- Open generators to port (small, language-agnostic):
  [blobator](https://github.com/Magicianred/blobator) (bitmask table + corner
  compositing) and itsjavi/autotiler (15-tile Wang → blob PNG **+ Godot `.tres`
  with bitmask/collision/region baked in**).
  [Tilesetter's model](https://www.tilesetter.org/docs/generating_tilesets)
  (base + 4 edges → auto-reorient + corner-composite, repeat per terrain pair) is
  the UX a tileset skill should adopt. ⚠ The generators disagree on bit weights —
  normalize to **one** convention in our docs to avoid silent mis-mapping.
- **Seam validation is the single most agent-friendly verifiable art check**
  ([pycheung](https://www.pycheung.com/projects/seamless-texture-checker/)): for a
  wrap tile assert `left edge == right edge` and `top == bottom`; for a 47-set
  assert every adjacency-compatible tile pair has matching edge masks. Fully
  deterministic → ideal eval gate (generate tileset → run checker → fail on any
  seam). Add as a `tileset_lint` sibling to the existing Python sprite linter.
- Export targets are machine-writable:
  [Tiled wangsets `.tsj`](https://doc.mapeditor.org/en/stable/manual/terrain/),
  [LDtk reads `.aseprite` directly with hot-reload](https://ldtk.io/), Godot
  `.tres`,
  [Gabinou's Lua PNG-atlas + JSON-index export](https://github.com/Gabinou/tilemap_scripts_aseprite)
  (note: it exports the active region only — handle whole-sprite explicitly).
  → Candidate **SPEC-003: tilemap tool family** (`live_create_tilemap_layer`,
  `live_get_tileset`, `live_set_tile`/`live_stamp_tiles`, `live_dedupe_tiles` over
  Auto mode, `tileset_lint`, engine export) — the biggest strategic gap, already
  monetized by the Pro competitor.

## F. Assets-first — search / import / restyle CC0 assets

The 2D analog of blender-mcp's flagship PolyHaven/Sketchfab pattern.

- [Lospec palette API](https://lospec.com/palettes/api) — `load_lospec_palette(slug)`
  is trivial (GET JSON → resize + set palette), turns knowledge/palettes from
  dozens into thousands.
- [HF OpenGameArt-CC0 dataset](https://huggingface.co/datasets/nyuuzyou/OpenGameArt-CC0)
  — 15.7k CC0 assets as Parquet (DuckDB-queryable, preview URLs + download links,
  all-CC0 removes per-asset license checks). Kenney's 60k assets: vendor-locally
  then index. Pattern: search → **preview thumbnail the agent can SEE** → import to
  a locked reference layer → restyle/repalette (the missing middle step that turns
  "generic Kenney look" into project-native art).
- Ship a `MANIFEST.json` + auto-generated `CREDITS.txt` on export (the
  [ULPC](https://github.com/LiberatedPixelCup/Universal-LPC-Spritesheet-Character-Generator)
  pattern) so provenance/attribution is captured at fetch time.

## G. Reference → style profile (verifiable "match my existing sheet")

The most under-covered high-value workflow: turn a reference sheet into a
**machine-checkable style contract** so "make a goblin matching my hero sheet"
becomes a deterministic, lintable task instead of vibes.

1. **Normalize the reference.**
   [proper-pixel-art](https://github.com/KennethJAllen/proper-pixel-art): grid
   detect → median cell spacing → per-cell mode color recovers the *true* native
   resolution + clean palette from any scaled/dithered/JPEG reference, so analysis
   never runs at the wrong scale. Output e.g. "32×32 at 4× scale" → feeds grid +
   proportions (heads-tall = sprite height / head-cluster height). Native via the
   `image`/`imageproc` Rust crates.
2. **Extract the palette** with the right method per task —
   frequency / median-cut / k-means histogram reduction
   ([Aseprite community script](https://community.aseprite.org/t/script-reference-color-extractor-frequency-diversity-k-means/28258));
   textbook algorithms, trivially native in Rust.
3. **Recover ramp *structure*, not just colors.**
   [color-ramp-sort](https://github.com/matsagad/color-ramp-sort): Hough
   collinearity in RGB + average-linkage dendrogram groups flat colors into
   shadow→highlight ramps and labels them by the region they cover (skin/metal/cloth).
4. **Serialize to a `StyleProfile`** —
   `{grid, origin, palette, ramps:[{role, colors[], length}], outline_policy,
   light_dir, heads_tall, frame_counts}` (outline by sampling the silhouette
   boundary color; light_dir by comparing top-left vs bottom-right cluster
   luminance) ([spritesheets.ai](https://www.spritesheets.ai/blog/pixel-art-spritesheet-tutorial)).
   Feed it to rig-builder / animation-director as **hard constraints** and to
   pixel-critic / the linter as a **checklist**.
5. **Lint ramp quality** with codifiable rules — monotone value, positive per-step
   hue-shift, mid-peaked saturation, no max-sat+max-value corner
   ([SLYNYRD](https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes)) —
   a new objective scoring axis for the eval harness.

This composes a pipeline of mostly-deterministic, native-Rust steps:
`live_normalize_reference` → `live_extract_palette_from_image` → ramp-sort →
`derive_style_profile` → enforce in generation + lint. It's the 2D analog of
Figma's "return named tokens, not hex," and the thing that makes imported/generated
assets actually look native to the project.

---

## Prioritized roadmap (impact × effort)

| # | Item | Effort | Impact | Why |
|---|------|--------|--------|-----|
| 1 | **Preview overhaul**: nearest-neighbor upscale to ~1024 px, labeled 8-px gutter, return image inline (MCP Image content) | S | ★★★ | Everything else is moot if the agent can't *see* its work (§A). Strongest evidence in the report. **Upscale landed** (`live_save_preview` + `src/preview.rs`, auto-preview hook rewired, 6 tests); gutter / inline-image / region-crop are the fast-follow. |
| 2 | **`ascii_view`** — one char per palette index + row/col rulers | S | ★★ | LLMs' best channel (§A); works for non-vision clients (Codex). |
| 3 | **Film-strip tool** for animation (near-square labeled grid, upscaled) + **`frame_diff`** as text grid | S–M | ★★ | GIFs are invisible to Claude API (§A); drift is failure #1. |
| 4 | **`palette_snap` (real LAB)** on draw tools + **`live_adjust_pixels`** (Magic Pencil semantic ops) | M | ★★★ | Turns the linter from catcher into non-issue; surpasses pixel-mcp's fake LAB/light-dir (§D). |
| 5 | **`pixel_creation_strategy` MCP prompt** + `app.transaction` per call + auto-snapshot before `run_lua_script` | S–M | ★★ | Doctrine into the protocol; every client benefits (§B). |
| 6 | **`import_reference`** (v1: K-Centroid Lua / `magick -remap` in a skill; v2: unfake/imagequant crate, Sobel-profile regrid) | M | ★★★ | Unlocks the whole hybrid pipeline (§C2). |
| 7 | **Skill `pixel-reference-motion`** (video → frames → locked reference layer → trace; accept user .mp4) | M | ★★★ | Fully-designed, deep-verified (§C1). |
| 8 | **`dither_fill_region` + `gradient_map` + rotsprite rotation** | M | ★★ | The exact tedious work LLMs do worst; algorithms are solved (§D). |
| 9 | **Evals**: SwordsBench task (verbatim prompts), silhouette-IoU across frames, long-session degradation; A/B the "artistic agent" persona line | M | ★★ | Measures the documented failures; repo already has the rubric machinery in `evals/tier_b.json` (§A,§D). |
| 10 | **SPEC-003 tilemap family**: CRUD + Pack Similar Tiles + blob-47 (8-bit bitmask) template + **`tileset_lint` seam check** + Tiled/LDtk/Godot export | L | ★★★ | Ecosystem blind spot, monetized by a competitor, exactly our game-dev audience; seam check is the most verifiable art gate (§E). |
| 11 | **`StyleProfile` pipeline**: normalize → extract palette → ramp-sort → derive profile → enforce + ramp-lint | M–L | ★★★ | Makes "match my existing sheet" deterministic; native-Rust steps (§G). |
| 12 | **Asset search** (HF CC0 + Lospec API) + manifest/CREDITS | M | ★★ | Assets-first is blender-mcp's winning move (§F). |
| 13 | **Plugin packaging** + `/pixel-doctor` + demo clips | M | ★★ | 208-star pixel-plugin shows distribution > tool count (§B). |

**Suggested order:** items 1–3 ("perception overhaul") first as one batch — cheap,
independent, and the foundation for everything else; then 4–5; then the hybrid
pipeline (6–7); tilemap (10) deserves its own spec per the project's spec-first
tradition.

## Corrections surfaced during verification (don't skip)

- pixel-mcp's "LAB snapping" and "8 light directions" are **partly fake in their
  own source** — re-implementing honestly is differentiation, not catch-up.
- SwordsBench is an **N=2, single-run, self-scored weekend experiment**; its two
  "empirical" claims (tool granularity → quality; artistic prompt → better) are
  the author's *hypotheses*. Port the methodology; A/B the persona line in our
  harness before believing it.
- SpriteCook's blog is **marketing with no algorithm** — cite it for problem
  framing only; build the unfake.js Sobel-profile method instead.
- Sprite Analyzer & Magic Pencil source have **no license** → reimplement (the
  transforms are standard).
- Veo / PixelLab / Retro Diffusion are **paid and optional** — gate behind
  capability/API-key checks; always ship the no-API path (user video, local
  `magick`, curated palettes) as the default.

## Open questions & out-of-scope (future work)

Honest gaps in this research, so a future effort knows where to dig:

- ~~**No live capability benchmark of *our* stack.**~~ **Addressed 2026-06-24.**
  `evals/BENCHMARK.md` §A now records blind-judged with/without runs on this stack:
  Perception **+1.33**, Constrained colour **+75pp**, Reference grounding **+4.0**.
  (Paths 3/5/6 remain unmeasured; §C added a long-session degradation run → no decay.)
- ~~**The "artistic agent" persona is unmeasured.**~~ **Measured & REJECTED 2026-06-24.**
  The A/B in `evals/judge.py` was run de-confounded over 3 runs (§B); the sign was
  inconsistent (+0.43 / −0.33 / +0.10), so the persona line is **kept out of prompts**.
- **Tuning constants are unpublished.** Anti-flicker `CHANGE_THRESHOLD` /
  `get_mode_color`, chroma-key thresholds, and regrid peak-spacing tolerances must
  be tuned against golden PNGs — start points are noted in §C but not validated
  here.
- **Generation-route economics unmeasured.** Per-image cost/latency of Veo /
  PixelLab / Retro Diffusion / Replicate were not benchmarked; treat the paid paths
  as optional until a cost gate (`check_cost`-style) is wired.
- **Tool-surface pruning not designed.** §B argues our ~150 flat tools hurt
  selection, but the actual profile/grouping split (which tools in the "compact"
  default) needs its own design pass informed by eval transcripts.
- **License posture per source not fully audited.** Several referenced repos
  (Sprite Analyzer, Magic Pencil, AsepriteAddons=GPL-3.0) are reimplement-only;
  any adoption PR must re-check the license of its specific source.
- **Not covered at all:** audio/SFX, in-engine runtime behavior, non-Aseprite
  editors, and 3D→2D pre-rendering pipelines (the DALL-E→Tripo→Mixamo→Blender
  route appeared but was de-prioritized as too heavy for an Aseprite-native tool).

## Appendix — verified sources

Research methodology & key sources (all opened during verification):
SwordsBench <https://ljvmiranda921.github.io/notebook/2025/07/20/draw-me-a-swordsman/> ·
Text2Space <https://arxiv.org/html/2604.14641v1> ·
LLMs Can't See Pixels <https://www.lesswrong.com/posts/uhTN8zqXD9rJam3b7/llms-can-t-see-pixels-or-characters> ·
Anthropic vision <https://platform.claude.com/docs/en/docs/build-with-claude/vision> ·
VLMs are Blind <https://arxiv.org/abs/2407.06581> ·
IG-VLM <https://arxiv.org/abs/2403.18406> ·
Donut test <https://www.mindstudio.ai/blog/claude-blender-mcp-60-percent-tokens-donut-test-results> ·
ClaudePlaysPokemon <https://michaelyliu6.github.io/posts/claude-plays-pokemon/> ·
LTD-Bench <https://arxiv.org/pdf/2511.02347> · SVGenius <https://arxiv.org/html/2506.03139v1> ·
blender-mcp <https://github.com/ahujasid/blender-mcp> ·
Claude After Dark <https://mikeveerman.be/blog/github-2026-01-17-claude-after-dark/> ·
unfake.js <https://github.com/jenissimo/unfake.js/> · K-Centroid <https://github.com/Astropulse/K-Centroid-Aseprite> ·
Retro Diffusion API <https://github.com/Retro-Diffusion/api-examples> ·
PixelLab MCP <https://github.com/pixellab-code/pixellab-mcp> ·
anti-flicker <https://sarthakmishra.com/blog/building-animated-sprite-hero> ·
pixel-mcp <https://github.com/willibrandon/pixel-mcp> ·
Aseprite MCP Pro <https://github.com/youichi-uda/aseprite-mcp-pro> ·
Magic Pencil / Sprite Analyzer <https://github.com/thkwznk/aseprite-scripts> ·
Aseprite tileset API <https://www.aseprite.org/api/tileset> ·
Pack Similar Tiles <https://github.com/aseprite/Aseprite-Script-Examples/blob/main/Pack%20Similar%20Tiles.lua> ·
OpenGameArt-CC0 <https://huggingface.co/datasets/nyuuzyou/OpenGameArt-CC0> ·
Lospec API <https://lospec.com/palettes/api>

Gap-fill sources (perception / reference→style / in-editor gen / tilesets):
AdaZoom/MEGA-GUI <https://arxiv.org/html/2511.13087> ·
Set-of-Mark <https://arxiv.org/abs/2310.11441> ·
SketchAgent coord-grid <https://arxiv.org/html/2604.22875v2> ·
GPT-4V directional dyslexia <https://towardsdatascience.com/gpt-4v-has-directional-dyslexia-2e94a675bc1b/> ·
VLM resolution curse <https://huggingface.co/blog/visheratin/vlm-resolution-curse> ·
reference color extractor <https://community.aseprite.org/t/script-reference-color-extractor-frequency-diversity-k-means/28258> ·
color-ramp-sort <https://github.com/matsagad/color-ramp-sort> ·
proper-pixel-art <https://github.com/KennethJAllen/proper-pixel-art> ·
SLYNYRD palettes <https://www.slynyrd.com/blog/2018/1/10/pixelblog-1-color-palettes> ·
spritesheets.ai style guide <https://www.spritesheets.ai/blog/pixel-art-spritesheet-tutorial> ·
Retro Diffusion API examples <https://github.com/Retro-Diffusion/api-examples> ·
RD on Replicate <https://replicate.com/blog/retro-diffusions-pixel-art-models-are-now-on-replicate> ·
Red Blob autotile <https://www.redblobgames.com/articles/autotile/> ·
BorisTheBrave blob/Wang <http://www.boristhebrave.com/permanent/24/06/cr31/stagecast/wang/blob.html> ·
Aseprite tilemap docs <https://www.aseprite.org/docs/tilemap/> ·
Tilesetter <https://www.tilesetter.org/docs/generating_tilesets> ·
blobator <https://github.com/Magicianred/blobator> ·
seamless checker <https://www.pycheung.com/projects/seamless-texture-checker/> ·
Gabinou tilemap scripts <https://github.com/Gabinou/tilemap_scripts_aseprite>
