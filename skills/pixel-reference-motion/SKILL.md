---
name: pixel-reference-motion
description: Turn a reference motion — a video clip, an animated GIF, or a PNG frame sequence — into a clean, palette-locked pixel-art animation by tracing over a per-frame locked reference layer. Fixes the cross-frame character drift you get from generating each frame independently. Use when the user has a video/GIF/reference frames of a motion and wants pixel-art animation from it (rotoscope).
argument-hint: "[clip.mp4 | frames-dir | sprite.gif] [key frames e.g. 6] [size e.g. 48] [palette]"
---

# /pixel-reference-motion — rotoscope motion from a reference (roadmap #7)

Generating each animation frame independently **drifts the character** every frame
(the SwordsBench failure mode). A single *consistent reference motion* — a 4-second
green-screen video, an animated GIF, or a hand-supplied frame sequence — anchors all
of them. This skill loads each reference frame as a **dimmed, palette-locked layer on
its own animation frame**, then the agent **traces clean pixels over it**: motion-
consistent, on-model, no drift.

Built on `live_import_reference` (SPEC-006 — content-aware downscale + CIELAB snap) and
`rules/04-animation.md`. Grounded in research §C1 (video→frames→trace; Mike Veerman,
"Claude After Dark") and §C4 (shared-palette anti-flicker). Pairs with `/pixel-palette`
(lock the palette first) and `/pixel-review` (grade section G).

## Preconditions (check FIRST, in order)
1. **Preflight.** `live_preflight` → require `ready:true`. If false, STOP and report —
   never fall back to batch/file tools (`docs/adr/0001-batch-vs-live-tools.md`).
2. **Tool present.** This skill needs a server build exposing `live_import_reference`
   (SPEC-006). If that tool is absent, the connected server is too old: STOP and tell
   the user to rebuild + reconnect the MCP server — do NOT improvise by hand-drawing a
   full-resolution reference pixel-by-pixel (loud degradation, ADR-0005).
3. **Palette locked.** Run `/pixel-palette` first. Every traced frame draws from **one
   shared palette** — that is the anti-flicker foundation (§C4), and `import_reference`
   snaps the reference to it too, so the guide is already on-model.
4. **Rig (preferred).** For limb-clean motion, trace onto rig layers (`rules/05`). For a
   straight rotoscope, a single `Art` layer over the `Reference` layer is acceptable.

## Step 1 — get the reference frames (no API key required)
All three sources end as a **PNG sequence on disk**. Use the bundled helper, which wraps
`ffmpeg` and chroma-keys the background to transparent:

- **A — user video (`.mp4` / `.mov` / `.webm`).**
  `python ${CLAUDE_PLUGIN_ROOT}/tools/video_frames.py <clip.mp4> --out C:/tmp/ref --count <K>`
  Samples `K` evenly-spaced frames, then keys a green (`#00ff00`) background out with the
  adaptive green-dominance test `g - max(r,b) > threshold` (the field-proven method, §C1).
  Tune per clip: `--chroma-threshold N` (default 20; lower = more aggressive), `--key-color
  #rrggbb` for a non-green screen, `--no-chroma` for a clip with no backdrop.
- **B — animated GIF.** Same helper: `… <in.gif> --out C:/tmp/ref --count <K>` (or
  `--no-chroma` if the GIF has its own transparency).
- **C — user-supplied PNG sequence.** Already extracted — skip to Step 2 (optionally key it:
  `… --frames C:/tmp/ref --out C:/tmp/ref`, no `ffmpeg` needed).

If `ffmpeg` is not installed, the helper says so — ask the user to install it or supply a
PNG sequence (path C). Never silently produce nothing.

## Step 2 — reduce to key poses (do NOT rotoscope every frame)
Real pixel-art animation is a few strong keys, not 60 fps (`rules/04` §2). Choose **4–8 key
poses** at the motion's *extremes* and sample only those — set `--count` in Step 1 to that
number, or cull the extracted sequence. Mapping:
- **Walk:** contact → down → pass → up (mirror for 8, or share to 4–6).
- **Attack:** anticipation → strike → follow-through.
- **Idle:** 2–4 breathing extremes.
More frames = mushier motion *and* more per-frame drift to fight.

## Step 3 — import each reference frame onto its own frame
Make the sprite the target size first (`/pixel-new` or `live_resize_canvas`). Then for each
key `i` (1-based), with `K` keys total:
1. **Ensure frames** — `live_ensure_frames { count: K }` once up front.
2. **Select the frame** — `live_set_active_frame { frame: i }`.
3. **Import the matching reference PNG**, snapped to the locked palette, onto a `Reference`
   layer at that frame:
   `live_import_reference { filename: "C:/tmp/ref/003.png", width: W, height: H,
   method: "dominant", palette: ["#…", …], layer: "Reference", frame: i }`
   (omit `palette` to snap to the active palette; `snap:false` keeps raw source colours).
4. **Dim it for tracing** — `live_set_layer_properties { name: "Reference", opacity: 128 }`
   (≈50%, §C1; `opacity` is 0–255). Do this once; the layer spans all frames.

## Step 4 — trace clean pixels over each reference
On a separate **`Art`** layer *above* `Reference`, draw the clean sprite for each frame,
following the reference silhouette but obeying the rules — not copying its fuzz:
- Readable silhouette every frame (`rules/00`, `rules/03`); clean the video's anti-aliased,
  off-grid edges to crisp pixels.
- Ramp-shade from the locked palette only (`rules/01`, `rules/02`); no off-palette strays.
- **Preserve volume** frame-to-frame (`rules/04`) — the reference may wobble in size; your
  sprite must not. Use onion-skin to hold arcs and mass.
The reference is a guide, not the output.

## Step 5 — anti-flicker pass (shared palette + static pins)
Frames already share one palette (Precondition 3). For pixels that should **not** change
across the loop (a planted foot, a still torso), pin them to the *same* colour every frame
so they don't shimmer (§C4 static-region idea). `live_frame_diff { from_frame, to_frame }`
shows exactly which cells changed between two frames — use it to catch pixels that move when
they shouldn't.

## Step 6 — SEE the motion as a film-strip (a GIF review is useless)
The Claude API reads only the **first frame** of an animated GIF — never judge animation
from a GIF. Render a film-strip instead:
`live_save_filmstrip { filename: "C:/tmp/strip.png" }` lays every frame out in one upscaled,
near-square grid (optional integer `scale`). Check: silhouette readable in each cell, **no
drift/jitter**, volume held, the loop closes (last → first).

## Step 7 — drop the reference, tag, time, review
1. **Remove the guide** — it must not ship: `live_set_layer_visibility { name: "Reference",
   visible: false }`, or `live_delete_layer { name: "Reference" }` once you're done.
2. **Tag the cycle** — `live_new_tag { name: "walk"|"attack"|"idle", from_frame: 1,
   to_frame: K }`, correct loop direction.
3. **Time it** — `live_set_frame_properties { frame: i, duration: <seconds> }` per `rules/04`
   (**duration is seconds, not ms**): snappy strike ~0.06–0.10 s, idle ~0.15–0.25 s, walk
   ~0.12–0.15 s. Hold the anticipation pose a touch longer.
4. **Self-review** — `/pixel-review` section G (anticipation, body bob, counter-swing,
   overlap, preserved volume, weapon stays a big readable shape, clean loop) + the on-palette
   linter. Fix every must-fail before declaring done.

## Definition of done
- `K` key frames, each **traced clean over a now-removed reference**, all on **one locked
  palette**; an `idle`/`walk`/`attack` tag with correct loop + per-frame timing; the
  `Reference` layer is hidden or deleted (never shipped); the **film-strip** shows readable,
  drift-free, volume-preserving motion; `/pixel-review` section G + the palette linter pass.

## Eval prompts (for graded testing)
- "Rotoscope this 4-second green-screen `run.mp4` into a 6-frame 48×48 walk on the goblin
  palette" → 6 frames extracted + chroma-keyed, imported on a dimmed `Reference` layer,
  traced clean on the locked palette, `Reference` removed, `walk` tag ~0.13 s/frame,
  film-strip drift-free, review G passes.
- "Make a pixel attack from these 5 reference PNGs" → 5 frames imported/traced,
  anticipation→strike→follow-through preserved, the weapon stays a big readable shape through
  the arc (never a stub), tagged `attack` with eased timing.
- Negative — **tool**: if `live_import_reference` is absent, STOP and tell the user to
  rebuild + reconnect the server (SPEC-006); never hand-draw a full-res reference pixel-by-
  pixel.
- Negative — **preflight**: if `live_preflight` is false the skill STOPS and writes nothing
  to disk via batch tools.
- Negative — **review medium**: never judge the result from a GIF (the API sees frame 1
  only); always review via the film-strip.
