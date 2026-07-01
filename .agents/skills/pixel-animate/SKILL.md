---
name: pixel-animate
description: Animate an Aseprite sprite — idle / walk / run / dodge / attack / hurt frames, tags, and per-frame timing. Use when adding motion, building a walk or attack cycle, or fixing floaty / uniform / too-fast timing. Requires a rigged sprite (use $pixel-new first).
---

# pixel-animate — frames, tags, timing (live)

## Method
1. **Preflight** + **rig first** — parts on their own layers. A 2-layer **Body / Legs** split lets
   the body bob while feet stay planted.
2. **Frames** — `live_new_frame {source_frame}` copies a frame (all layers).
   ⚠️ Parallel copies can inherit the **active** frame's cel offset and scramble durations —
   so after creating frames, **normalize cels** (`live_set_cel_properties {layer,frame,x,y}`; a
   partial update keeps the other axis; cels are full-canvas origin 0,0) and **set durations LAST**,
   then **verify with `live_list_frames`**.
3. **Motion**
   - **Idle** (slow, 150–300 ms/frame): bob the Body cel `y` by 1px on the in-breath; Legs stay (feet planted).
   - **Walk** (70–180 ms): alternate leg poses (clear the Legs cel + redraw contact/pass) + body bob (high at pass).
   - **Attack**: **anticipation** (wind up back) → **strike** (fast, ~50 ms, smear) → **impact HOLD ≥150 ms**
     (pose of max threat) → **recovery**. Vary durations (no uniform timing); weapon stays a big readable shape.
4. **Tag** — `live_new_tag {name, from_frame, to_frame, repeats}` (idle/walk loop `repeats:0`; a one-shot
   like death `repeats:1` / must not loop).
5. **Gate the timing** — export `live_list_frames` + `live_list_tags`, then
   `python tools/timing_lint.py clip.json` (aseprite-mcp repo; it reads Aseprite seconds / 1-based).
   Fix too-fast/slow, no-impact-hold, uniform, looping-death. Check volume drift with `tools/silhouette_iou.py`.
6. **Save** (often).

## Done
Tagged frames whose timing passes `timing_lint`, no cross-frame volume drift, previewed at target speed
(`live_save_filmstrip`). Review with `$pixel-review`.
