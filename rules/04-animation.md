# 04 — Animation

> Checklist 4.4. Timing, easing, anticipation, and the common cycles
> (idle, walk, attack). Pairs with the rig in `05-layers-and-rig.md`.

## 1. Principles that survive into pixels
The classic animation principles still apply, adapted to few frames:
- **Anticipation:** wind up before a big action (pull the club back before the
  swing). Without it, actions read as teleporting.
- **Squash & stretch:** preserve volume — a landing squashes, a jump stretches.
  Even 1–2 px of squash adds life. Don't change the total mass.
- **Follow-through & overlap:** loose parts (cloth, ears, sack) keep moving after
  the body stops, and start slightly later. Offsetting parts by a frame is what
  makes motion feel organic.
- **Arcs:** hands, weapons, and feet move along curves, not straight lines.
- **Ease in / ease out:** spend more frames where motion is slow (the extremes)
  and fewer where it is fast (the middle), so it accelerates and decelerates.

## 2. Pixel-art timing model
- You control timing with **frame count** and **per-frame duration (ms)**, not a
  smooth curve. Easing = unequal spacing of poses + unequal durations.
- Identify **key poses (extremes)** first, then add **breakdowns/in-betweens** only
  where you need smoothness. More frames ≠ better; readable keys > many mushy ones.
- Typical durations: snappy action ~60–100 ms/frame; idle ~150–250 ms/frame.
  Hold the anticipation pose a touch longer; make the fast pass-through short.

## 3. Idle
- Minimum: 2 frames (subtle). Better: 4–6. Breathe with **1–2 px** of vertical
  move on the chest/head, slight ear/cloth sway. Slow (150–250 ms).
- Keep feet planted. The motion is small and looping — never a jitter.

## 4. Walk cycle
- Classic 4-frame cycle of keys: **contact → down (recoil) → pass → up (lift)**,
  then mirror for the other leg → 8 frames for a full cycle (or share/mirror to
  keep 4–6).
- **Contact:** legs spread, front heel down, back toe down. **Down:** weight
  drops, body lowest, knee bent. **Pass:** legs together, body highest, swinging
  leg passes under. **Up:** push-off, body rising.
- **Counter-swing the arms** to the legs (opposite arm forward to the forward
  leg). Head bobs ~1 px with the body. Loose parts (sack, ears) overlap/lag.
- Keep the **center of gravity** believable: body is lowest at Down, highest at
  Pass — a flat body height looks like skating.

## 5. Attack (e.g., goblin club swing)
Three beats, do not skip the wind-up:
1. **Anticipation (1–2 frames):** pull the weapon **back/up**, shift weight back,
   maybe crouch. Hold slightly longer.
2. **Strike (1–2 frames):** fast arc **forward/down**; the weapon and arm travel a
   clear curve and may **stretch** a little along the motion. This is the shortest,
   snappiest beat. Optionally an impact frame with a 1-frame smear/flash.
3. **Follow-through & recovery (1–2 frames):** weapon overshoots, then settles
   back to idle; body recovers. Loose parts lag.
- The weapon must stay a **readable big shape** through the swing — do not let it
  collapse to a stub mid-arc (a past failure mode). Use a smear shape for the fast
  frame rather than shrinking it.

## 6. Working method
- Animate on the rig layers (move/redraw limb layers per frame), not by redrawing
  the whole sprite — keeps volume and palette consistent. See `05-layers-and-rig.md`.
- Use **onion-skinning** to keep arcs smooth and volumes constant between frames.
- Tag cycles clearly (`idle`, `walk`, `attack`) with correct loop direction.
- Test the loop at 100% and at target speed, not zoomed/slow.

## Do / Don't
| Do | Don't |
|----|-------|
| Wind up before big actions (anticipation) | Snap straight into the strike |
| Bob the body (low at Down, high at Pass) | Keep body height flat ("skating") |
| Counter-swing arms vs legs | Swing arms and legs in sync |
| Offset loose parts by a frame (overlap) | Move every part in perfect lockstep |
| Keep the weapon a big readable shape | Let the weapon shrink to a stub mid-swing |
| Few strong key poses | Many mushy in-betweens |
