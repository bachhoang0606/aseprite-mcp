---
name: pixel-animate
description: Animate a rigged sprite live in Aseprite — build idle, walk, or attack cycles with proper key poses, timing, anticipation and follow-through, then tag them. Use when the user wants character motion from an existing layered rig.
argument-hint: "[idle|walk|attack] [frame count] [layer rig]"
---

# /pixel-animate — motion from a rig

Generate readable cycles per `rules/04-animation.md`, animating the layered rig
(`rules/05`) rather than redrawing the whole sprite. Grounded in
`knowledge/references/pixel-art-sources.md` (pose-to-pose keys, walk contact/
passing, anticipation, per-frame ms timing, onion-skin).

## Preconditions
- `live_preflight` ready. A rig exists with limbs on their own layers (`ArmL`,
  `ArmR`, `Legs`/`LegL`/`LegR`, `Head`, `Shadow`). If not, build it first (`/pixel-new`).

## Method (all cycles)
1. **Key poses first** (pose-to-pose, not straight-ahead). Place keys, then add
   breakdowns only where motion needs smoothing.
2. **Ensure frames** with `live_ensure_frames` / `live_new_frame`; animate per-layer
   cels (`live_new_cel`, `live_set_cel_properties` to offset a limb, `live_draw_pixels`).
3. **Timing** via `live_set_frame_properties` (ms). Use onion-skin mentally: keep
   volume constant between frames.
4. **Tag** the cycle with `live_new_tag` (`idle` / `walk` / `attack`), correct loop.
5. Preview at 100% and target speed; fix any volume pop or skating.

## Idle
- 2–6 frames, 1–2 px breathing bob on chest/head, slight ear/cloth sway. Slow
  (~150–250 ms). Feet planted.

## Walk (4-frame minimum)
- Keys: **Contact** (legs spread, body lowest) → **Passing** (legs together, body
  highest) → Contact (other foot) → Passing. Frames 3/4 may mirror 1/2.
- Body **bobs** (low at Contact/Down, high at Passing) — never flat (skating).
  Arms **counter-swing** the legs. Loose parts (sack/ears) lag a frame. ~120–150 ms.

## Attack (club swing, no skipped wind-up)
- **Anticipation** (1–2 fr): pull weapon back/up, weight back; hold longer (~300–400 ms).
- **Strike** (1–2 fr): fast forward/down arc; weapon may stretch/smear; shortest,
  snappiest (~60–100 ms). Keep the weapon a **big readable shape** (never a stub).
- **Follow-through/recovery** (1–2 fr): overshoot then settle to idle; loose parts lag.

## Definition of done
Passes `rules/06` section G: anticipation present, body bob, counter-swing,
overlap, preserved volume, weapon stays readable, correct tags, clean loop.

## Eval prompts
- "Make a 4-frame walk for this rig" → contact/passing keys, body bob, arm
  counter-swing, `walk` tag, ~130 ms/frame.
- "Add an attack where the goblin swings the club forward" → wind-up→strike→
  follow-through, club big throughout, eased timing.
