# pixel-art-studio → aseprite-mcp: gap analysis

> What the sibling **`pixel-art-studio`** repo contains, how it relates to this
> project, and which of its assets are worth porting here. Companion to
> `docs/research/agent-pixel-art-techniques.md` (the strategy report). Produced
> 2026-06-30 from a map → synthesize → verify pass over both repos.

## 1. What `pixel-art-studio` is

It is **not a code project** — it is the *R&D lab + operating manual* behind one
specific game (Goblin Dodge, a top-down action / dodge-survival title). Layout:

| Area | Contents |
|---|---|
| `knowledge/` (6 docs) | Theory: animation principles, **combat readability**, pixel-art philosophy / scale hierarchy, indie production constraints, knowledge distillation |
| `scripts/` (~54 files, ~4.4k LoC) | **Real algorithms**: downscalers (alpha-majority, nearest-presnap, premultiplied-alpha area-average, edge-preserving, palette-priority, importance-map), source enrichers, a **pixel-authenticity gate**, a **chroma/alpha cleanup gate**, Aseprite import `.lua` |
| `experiments/` (~6.9k files) | The v2-downscale matrix: `methods/` + `evaluations/` + `profiles/` + `validation/` JSON — empirical record of which method won for which asset |
| `agents/ workflows/ pipelines/ review-systems/ checklists/ style-guides/ prompts/` | Process: agent roster, production workflows, **scored critique framework**, QA checklists, a **VFX system** with per-effect budgets |

**Core relationship.** `pixel-art-studio` is the *experimental / knowledge* side;
`aseprite-mcp` is the *shipped tooling* side. They are **complementary, not
overlapping**. This project is strong at **perception / detection** (e.g.
`tools/regrid.py` *detects* the native grid of scaled art); the studio is strong
at **transformation** (e.g. `scripts/downscale_alpha_majority.py` *turns* a 96×96
AI/construction frame into a clean 24×24 runtime sprite). The studio is also
built around the **generate-large → downscale-small** paradigm, which this
project deliberately treats as a secondary path (`/pixel-generate` is an opt-in
escape-hatch; draw-directly is the default).

## 2. Method & honesty note

A background workflow ran 6 parallel mappers (4 over the studio, 2 over this
repo's existing skills/specs/tools) → 1 synthesis agent (11 ranked
recommendations) → 11 adversarial verifiers (one per recommendation, each
re-checking the *real* repo to avoid recommending something already built).

The verify stage hit a session limit: **only 2 of 11 verifiers completed by
agent** (`alpha_cleanup`, `pixel-vfx` — both confirmed genuine gaps). The
remaining **9 were verified manually** against the repo (reading
`src/reference.rs`, `rules/04`, `rules/06`, `tools/style_profile.py`,
`tools/lint_sprite.py`, `evals/`, etc.). All 11 are real absences. Note the
distinction the verification could establish vs. not:

- **Accuracy of the gap** ("is it truly missing?") — verified: **11/11 missing**.
- **Necessity** ("should *this* project have it?") — a judgement, made in §4
  against the project's actual identity, *not* something the verifiers graded.

## 3. Verified gaps (11)

| # | Item | Value/Effort | Verdict — evidence |
|---|---|---|---|
| 1 | `tools/timing_lint.py` — per-state frame-duration lint | High/Small | **PORT** — `rules/04` is prose only; `duration` appears only in `video_frames.py` (extraction). No QA tool reads timing. |
| 2 | `tools/combat_lint.py` — flash-frame + impact-hold gate | High/Med | **PORT (conditional)** — nothing scans cels for flash/hold; `rules/04` mentions flash only as *optional*. Game-combat-specific. |
| 3 | `tools/alpha_cleanup.py` — edge-connected chroma removal + despill + despeckle (the FIX half) | High/Med | **PORT** (agent-verified) — `lint_sprite` only *detects* orphans; `video_frames` chroma-keys naïvely and deletes interior detail. |
| 4 | `/pixel-anim-review` + weighted rubric / red-flags | High/Med | **ENHANCE** — `rules/06` G is a 5-line qualitative checklist + one "feel ≥9/10". No weighted scoring, no auto-fail gate. |
| 5 | `tools/downscale.py` — selectable strategies + premultiplied-alpha area-average | High/Large | **PORT (conditional)** — `src/reference.rs` exposes only `Dominant`/`Average`; no premul, edge-preserving, palette-priority, importance-map, or fractional windows. |
| 6 | Pre-export technical-correctness guard | Med/Small | **PORT** — `/pixel-export` advises "integer scale" in prose; no validator rejects fractional scale / wrong mode. |
| 7 | Scale-hierarchy color budget (size-derived) | Med/Small | **PORT** — `lint_sprite --budget` is a manual color count, not derived from canvas size. |
| 8 | Transform-receipt + 7-criterion downscale eval methodology | Med/Small | **PORT (later)** — `evals/` has Tier-A/B but no per-transform provenance receipt. Worth it only once transforms (#3/#5) exist. |
| 9 | StyleProfile `roleColors`/`regions`/`protectedColors` | Med/Med | **ENHANCE** — `style_profile.py` has hue-roles + ramps + light_dir but no *semantic* roles, region boxes, or protected colors. |
| 10 | `/pixel-downscale` skill — run all strategies + comparison board | Med/Med | **PORT (blocked on #5)** — no orchestrated multi-method workflow / sheet normalization. |
| 11 | `/pixel-vfx` skill — budgeted VFX recipes + element palettes | Med/Med | **PORT (conditional)** (agent-verified) — no VFX skill/rule/budget; primitives exist (`live_new_frame` + draw tools). Game-combat-specific. |

> Caveat on accuracy: the studio's "winning method" findings (e.g. alpha-majority
> `m04`) are empirical for **one game / one art style**; they may not generalise
> as defaults.

## 4. Necessity framework (A / B / C)

Judged against this project's identity: a **general, draw-first** pixel-art MCP
with an existing **quality-gate machinery** (`tools/*.py` lint → `evals/run.py`
Tier-A → `/pixel-review`), which has previously **rejected scope creep** (persona,
tool-surface gating). "Necessary" = *fills a hole in a system the project already
committed to*; "scope creep" = *adds something outside it*.

### Group A — aligned, do first
`#1 timing_lint`, `#4 anim-review`, `#6 export-guard`, `#7 scale-budget`.
Paradigm-agnostic. The animation rule family (`rules/04`) is the **one major
ruleset still stuck as prose with no machine gate** — Group A completes the exact
pattern the project already uses for palette/orphan/silhouette. **Necessity: high**
(A1/A4 highest; A3 a small safety net).

### Group B — applicable, but serves a path kept secondary
`#3 alpha_cleanup`, `#5 downscale`, `#8 receipt`, `#9 styleprofile`, `#10 skill`.
All revolve around "import external/AI art → process it". That path **exists and is
live-verified** (`live_import_reference`, `live_import_animation`,
`/pixel-generate`, `/pixel-reference-motion`) but is positioned as an escape-hatch.
`#3` is the most aligned (improves *any* import; pairs detect-here/fix-there with
`lint_sprite`). `#5` is large effort serving the de-emphasised paradigm — start
minimal (just `area_average_premul` to fix edge fringing) rather than the full
matrix. **Necessity: conditional** on how much the project invests in import.

### Group C — expands scope toward one game genre
`#2 combat_lint`, `#11 pixel-vfx`. This is **action-game** knowledge ("every hit
must flash, hold ≥150 ms, telegraph the wind-up") — a game-design opinion, not a
pixel-art universal. Baking it into a general tool is the kind of scope creep the
project has avoided. **Resolution: ship as an opt-in rule profile** (like
`lint_sprite --palette`), off by default. **Necessity: conditional** — high only
if the project chooses to target action-game asset production.

## 5. Recommended path & open decision

1. **Group A now** (this work) — cheap, on-identity, closes the biggest hole.
2. Then **`#3 alpha_cleanup`** — independent, improves the existing import path.
3. Defer **#5 / Group C** until one product decision is made:

> **Is aseprite-mcp a *general* pixel-art tool, or a *production tool for action-game
> assets*?** The answer flips Groups B and C from "skip" to "high priority".

Port note: every studio script uses **PIL/Pillow**; this repo is **stdlib-only on
`tools/pixelpng.py`** — ports must be reimplemented, not copied.

## 6. Status

- **2026-06-30:** Group A under implementation (`tools/timing_lint.py` +
  `knowledge/timing-budgets.json`, `knowledge/scale-hierarchy.json` +
  `lint_sprite` auto-budget, `tools/export_guard.py`,
  `rules/07-animation-review.md`), each wired into the Tier-A eval harness.
- Groups B/C: pending the §5 positioning decision.
