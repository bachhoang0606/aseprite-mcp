# Tool-surface measurement — run 2026-06-24 (partial)

15 cases, selector agents under FLAT vs GATED surfaces. **Flat agents completed; the 15 gated
agents hit the account session limit (resets 12:50 Asia/Tokyo) and must be re-run** — so the
gated *routing accuracy* number is pending. The flat accuracy + the analytic token model are final.

## Findings (already decisive for Claude Code)

1. **Flat selection accuracy = 1.0 (15/15).** Every agent picked the exactly-correct tool from the
   full 77-tool flat list, with sound reasoning (it distinguished `palette_snap` vs `snap_colors`,
   `new_tag` vs `set_tag_properties`, `pack_similar_tiles` vs `create_tilemap_layer`, etc.).
   → On this suite the flat surface causes **no** mis-selection. There is **no accuracy upside** for
   pruning to recover.

2. **Token model (per turn / per task):**
   | surface | standing/turn | note |
   |---|--:|---|
   | flat_eager (Cursor/Desktop/API) | **23,429** | every schema loaded |
   | flat_deferred (Claude Code today) | **1,386** | names only; +ToolSearch per use (avg task 2,275) |
   | gated_dynamic | **7,336** | core schemas + group names; +gate loop per non-core task |

   - For an **eager** client, gating saves ~**16k** standing tokens → a real win.
   - For a **deferred** client (Claude Code), gated standing (7,336) is **higher** than flat-deferred
     (1,386) — because `core` eagerly carries the big `save_preview` (2.5k) + `import_reference` (1k)
     schemas — **and** gating adds a gate-open round-trip per non-core task. Net: gating **costs
     Claude Code more**, with no accuracy to gain.

## Verdict (data-backed)
- **For Claude Code: do NOT add gating.** Flat selection is already perfect and gating only adds
  token cost + recall risk. (This matches the analytic argument: ToolSearch already defers schemas.)
- **Pruning is justified only for eager clients**, and the cheaper-and-safer lever there is **static
  profiles** (set-once, no per-task loop) or **consolidation** (fewer tools, nothing hidden) — not
  dynamic hard-gating.
- **A smaller `core`** (drop the 2.5k `save_preview` + 1k `import_reference` to a "names + ToolSearch"
  tier even within core) would cut the gated/eager standing further — the model exposed that the big
  win is *not loading giant schemas eagerly*, which is orthogonal to gating.

## Pending
- Re-run the 15 gated agents after the session reset to fill in **routing accuracy** (does hard
  gating make the model open the *wrong* group → a recall miss). Resume the same workflow
  (`_wf_select.js`); the 15 flat agents are cached. Then `score.py --selections ... --out results.json`.
