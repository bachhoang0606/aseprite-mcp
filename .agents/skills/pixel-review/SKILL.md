---
name: pixel-review
description: Critique an Aseprite sprite or animation and return a scored, actionable report. Use when asked for quality feedback, a go / no-go before shipping, or "why does this look off / muddy". Grounds its verdict in deterministic lint gates, not vibes.
---

# pixel-review — scored critique (static + animation)

Judge at **100%** (`live_save_preview`), and per-frame for animation (`live_save_filmstrip`).

## Static axes (score each pass / weak / fail)
- **Silhouette/readability** — recognizable flat shape; extremities break the outline; no tangents.
- **Palette/colour** — all on-palette; 3–5 step **hue-shifted** ramps; no pure black/white; count within size budget.
- **Form/light** — one light direction; no **pillow-shading**; focal point strongest contrast.
- **Linework** — 1px hue-shifted outline; **no banding**; no orphan/stray pixels; AA hand-placed, not fringe.
- **Proportion/view** — matches size; if 3/4: asymmetric, planar-shaded, features off-centre.

## Animation axes (if animated) — ground with tools (aseprite-mcp repo)
- **Timing/hold/easing** → `python tools/timing_lint.py clip.json` (from `live_list_frames`+`live_list_tags`).
- **Volume/mass drift** → `python tools/silhouette_iou.py` (0.80 floor).
- **Palette/orphans per frame** → `python tools/lint_sprite.py <frame>.png --palette <pal>.json`.

## Report shape
1. **Verdict:** pass / needs-work + headline reason (any failing **must** or tool error caps at needs-work).
2. **Findings:** only weak/failing lines — *what* + *where* (region/layer/frame) + concrete fix (cite the tool finding).
3. **Score /10.**
4. **Top 3 fixes** by impact, each routed to a skill (e.g. "$pixel-shade the Head — pillow-shaded").
