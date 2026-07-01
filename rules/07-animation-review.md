# 07 — Animation review (scored, for animated sprites)

> A weighted, game-feel review for **animation**, the way `06-review-checklist.md`
> is for a static sprite. Section G of `06` is a quick qualitative pass; this is
> the scored rubric used when motion quality is the question. Each deterministic
> axis is **grounded by a tool**, not by vibes — run the tool first, then score.

## How to run it
1. Render the animation at **target speed** (`live_save_filmstrip` / `live_save_preview`),
   not zoomed/slowed. Judge the loop at 100%.
2. Export the tags + per-frame durations (`live_list_tags` + `live_list_frames`) and
   run the deterministic gates:
   - **Timing** → `python tools/timing_lint.py clip.json` (bands in
     `knowledge/timing-budgets.json`).
   - **Volume / proportion drift** → `python tools/silhouette_iou.py` (adjacent-frame
     IoU; the 0.80 floor is the drift gate).
   - **Palette / strays** → `python tools/lint_sprite.py` per frame.
3. Score the rubric below, feeding the tool output into the relevant rows.

## Weighted rubric (1–5 each)
| Criterion | Weight | 1 — Poor | 3 — Acceptable | 5 — Professional | Grounded by |
|---|---|---|---|---|---|
| **Timing & easing** | 0.30 | All frames equal duration | Some variation | Eases, holds, pacing feel physical | `timing_lint` |
| **Silhouette readability at speed** | 0.25 | Unreadable at 100% | Readable with effort | Instant read in motion | eye + `lint_sprite` |
| **Volume / mass consistency** | 0.20 | Mass visibly drifts | Minor drift | Locked (IoU ≥ 0.80) | `silhouette_iou` |
| **Anticipation & follow-through** | 0.15 | Neither present | One present | Both, natural | eye (`rules/04`) |
| **Loop & polish** | 0.10 | Pops / dead frames | Loops, no polish | Clean loop + secondary motion / sub-pixel | eye |

**Weighted score = Σ(criterion × weight).** Pass when **≥ 3.5 / 5** *and* no auto-fail.

## Auto-fail (caps verdict at needs-work regardless of score)
General — apply to every animation:
- **Non-integer / non-uniform timing on an action** (no easing) — `timing_lint` `uniform_timing`.
- **No impact/hold beat** where the state needs one (e.g. attack never holds ≥150 ms) —
  `timing_lint` `no_impact_hold`.
- **Mass/volume drift** across frames (IoU below the 0.80 floor) — `silhouette_iou`.
- **A one-shot state that loops** (death/KO set to infinite repeat) — `timing_lint` `loops`.
- **Pure black (`#000000`) outline** or off-palette strays — `rules/02` / `lint_sprite`.
- **Non-integer export scale** — `tools/export_guard.py`.

> **Combat profile (opt-in).** Action-game feedback rules — *every hit must produce a
> white silhouette flash*, directional knockback, and telegraphed wind-ups — are a
> genre choice, not a pixel-art universal, so they are **not** auto-fails here. They
> live in the opt-in combat profile (`tools/combat_lint.py` + a `/pixel-anim-review`
> combat mode), planned but off by default. See
> `docs/research/pixel-art-studio-gap-analysis.md` (Group C).

## Report shape
Mirror `06-review-checklist.md`:
1. **Verdict:** pass / needs-work + headline reason (any auto-fail caps at needs-work).
2. **Findings:** only weak/failing rows, each with *what* + *where* (tag/frame) + a fix,
   citing the tool finding where one exists.
3. **Weighted score / 5** (and which auto-fails, if any, tripped).
4. **Top 3 fixes** ordered by impact, each routed to a skill (e.g. "`/pixel-animate`
   the attack — add a 180 ms hold on the strike frame").
