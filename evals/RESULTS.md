# Tier-B eval results log

Outcomes of live LLM-judge runs (see [README](README.md#tier-b--llm-judged-live-manual--on-demand)).
One row per run. A case passes when `case_score >= pass_threshold` and no
`must_pass` criterion scored < 0.5. These are non-deterministic (live Aseprite +
judge model), so record the date, the build, and a one-line note.

| Date | Case | Checklist | case_score | Pass | Judge | Notes |
|------|------|-----------|-----------:|:----:|-------|-------|
| 2026-06-10 | `tb_pixel_new` | 5.1 | 0.97 | ✅ | independent subagent | 32×32, palette-first locked, full rig (AI Draft removed), readable goblin silhouette. Weak: blocky flat-fill. |
| 2026-06-10 | `tb_pixel_shade` | 5.2 | 0.97 | ✅ | independent subagent | Skin-ramp only, hue-shifted (teal→yellow-green), light upper-left, Body-only, idempotent re-run. |
| 2026-06-10 | `tb_pixel_animate` | 5.3 | 0.88 | ✅ | independent subagent | 4 frames, walk tag forward-loop, 130–140 ms, body bob + arm counter-swing, per-layer offsets. Weak: single Legs layer slides x±1 (no true spread/together); no loose-part lag. |
| 2026-06-10 | `tb_pixel_export` | 5.4 | 0.88 | ✅ | independent subagent | 128×32 sheet PNG + JSON (frame rects + per-frame durations), no draft layer. **Gap: JSON has no `frameTags`** (export tool omits tag data). |
| 2026-06-10 | `tb_animation_director` | 6.2 | 1.00 | ✅ | independent subagent | Frame-accurate attack plan (anticipation hold→fast strike→follow-through), tag `attack` loop once, flags missing Club-on-ArmR rig gap. |
| 2026-06-10 | `tb_rig_builder` | 6.4 | 0.98 | ✅ | independent subagent | Ordered rig plan + anatomy (chin→Head, shoulders→Body, Club→ArmR), PascalCase L/R, 3/4 depth note; Head+Body soloed clean (rendered). |

**Run 1 (2026-06-10):** all 6 cases PASS (threshold 0.75; no `must_pass` criterion
< 0.5). Executor ran each component live in Aseprite 1.3.17.2 via `live_*` tools;
an **independent judge subagent** scored objective evidence (tool outputs +
rendered layer/sheet PNGs) against `tier_b.json` rubrics — executor opinions were
withheld from the judge. Mean case_score 0.95.

### Known gaps surfaced by this run (backlog)
- ~~**5.4 export — JSON lacks `frameTags`.**~~ **FIXED 2026-06-10:**
  `export_spritesheet` now passes `--list-tags` by default whenever a JSON data
  file is requested (plus opt-in `list_layers`/`list_slices`), so the sheet JSON
  carries `meta.frameTags` — see `src/tools/export.rs::spritesheet_cli_args`
  (unit-tested: default-on, opt-out, no-data cases).
- **5.3 walk — leg keys are a horizontal slide**, not anatomically spread/together
  (single `Legs` layer). Splitting `LegL`/`LegR` (or per-frame leg redraw) would
  strengthen the contact/passing read.
