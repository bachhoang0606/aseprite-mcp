# Goblin — reference sheet

The project's recurring subject. Encodes decisions made during development so
every redraw is consistent. See `rules/03` (proportions/3-4 view), `rules/05`
(rig), and `knowledge/palettes/goblin-default.json` (palette).

## Identity & silhouette
- Squat, hunched humanoid: **big head, big nose, pointed ears, long arms, big
  hands, short bent legs.** Low, wide, asymmetric silhouette.
- Distinguishing extremities (must break the outline): pointed ears, big nose,
  and a **large readable club**.
- Mischievous/feral read, not noble. Hunch + forward head.

## Proportions
- Default canvas **64×64** for the detailed sprite (also works at 32×32 with
  reduced detail). ~3 heads tall counting the hunch; head is oversized.
- Arms reach near the knees (long). Hands are mitt-sized and clearly readable.
- Club is a **big shape** — a thick handle + heavy head; never a stub. This was a
  past failure mode (weapon collapsed mid-swing); keep it chunky in every frame.

## Palette
- Use `knowledge/palettes/goblin-default.json`.
- Skin = hue-shifted green ramp `#1B4D3E → #2E7D32 → #4CA02C → #6ABE30 → #A6D94A`
  (dark step teal-ish, highlight yellow-green — not a flat darken).
- Club/leather = brown ramp. Outline = cool near-black `#1B1226`, not pure black.
- Sparse accents only: `#FFEC27` eye glint, `#D8202E` for a rare focal hit.

## Rig (layers, bottom → top)
`Shadow` · `Legs` (or `LegL`/`LegR`) · `Body` (torso + shorts, complete from neck
down, shoulders included) · `ArmL` · `ArmR` (+ club on this hand) · `Head` (face,
ears, **chin**, nose).
- **Chin is on Head, shoulders on Body** (corrects an earlier mis-assignment).
- Each layer must read cleanly when soloed (rules/05 §4).

## Views
- **Side:** one ear/nose break the profile; near arm (with club) overlaps torso.
- **Front:** symmetric; show form via the green ramp, not foreshortening.
- **3/4 (preferred for RPG):** asymmetric face, nose off-center toward the far
  edge, far arm/leg foreshortened & partly occluded, feet staggered in depth,
  planar shading (front base / side one step darker / tops one step lighter).
  The held club reads in front, near hand.

## Animations
- `idle` — 1–2 px breathing bob, slight ear/sack sway.
- `walk` — 4-frame contact/down/pass/up; body bobs (low at down, high at pass);
  arms counter-swing; club-arm leads naturally.
- `attack` — wind up the club back/up (anticipation), fast forward/down arc
  (strike, optional smear), overshoot then settle (follow-through). Club stays a
  big readable shape throughout.

## Known failure modes to avoid
- "Lem nhem" seams where layers meet (stray neck pixels on Head, etc.).
- Weapon shrinking to a stub during the attack.
- Mechanical slicing from a source image instead of drawing each part complete.
- Front-view-nudged-sideways masquerading as 3/4 (must be truly asymmetric).
