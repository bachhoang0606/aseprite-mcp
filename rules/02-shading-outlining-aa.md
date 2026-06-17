# 02 — Shading, outlining, anti-aliasing

> Checklist 4.2. Selective outlining, anti-aliasing, banding/pillow-shading,
> dithering — the techniques that separate clean from "lem nhem".

## 1. Outlining
Two valid styles — pick one per sprite and stay consistent:

- **Full outline:** a 1-px dark line around the whole silhouette. Reads well on
  busy backgrounds; classic look. Outline color is a dark hue-shifted tone of the
  region it borders (not pure black) unless the art style wants hard black.
- **Selective outline (sel-out):** outline only where contrast is needed; let lit
  edges fade into the background or use a lighter outline there. More advanced,
  more "painterly," better for higher-res sprites.

Rules:
- Outline is **1 px**. A 2-px outline on a small sprite eats the form.
- **Color the outline** by darkening+hue-shifting the adjacent fill, instead of
  one flat black everywhere — flat black outlines look stickered-on.
- Interior separation lines (between arm and torso) follow the same logic and are
  often a mid-dark tone, not the full outline color.

## 2. Shading (give the form volume)
- Apply the **core shadow** on planes facing away from the light; apply the
  **highlight** sparingly on planes facing it. Most of the sprite stays base color.
- **Cast shadows** (under a chin, beneath an arm) are darker and have a hard edge;
  **form shadows** (a rounded cheek turning away) transition along the ramp.
- Highlights are the smallest area — a few pixels on the topmost lit edges and on
  shiny materials (metal, eyes). Over-highlighting makes everything look wet.

## 3. Avoid pillow-shading
- **Pillow-shading** = shading concentrically inward from the outline so the
  middle is brightest and there is no light direction. It looks puffy and amateur.
- Fix: commit to a light direction (`00-core-principles.md` §4) and put light on
  the side facing the source, shadow on the opposite side — **not** a uniform
  halo.

## 4. Avoid banding
- **Banding** = two parallel diagonal stair-step edges running alongside each
  other so their "jaggies" line up into a thick noisy seam. It reads as a fuzzy
  doubled line.
- Fix: don't place a shadow step exactly parallel-and-adjacent to the outline's
  stair-step. Offset it, or let the shadow meet the outline only at intervals.
- Keep stair-steps **consistent** (e.g., a 2:1 slope stays 2:1); irregular run
  lengths on a curve also read as noise — see jaggies below.

## 5. Clean lines & jaggies
- Draw curves/diagonals with **consistent run lengths** (e.g., 1px segments along
  a 45°, or steady 2-then-2-then-2 for a shallow slope). A line that goes
  3,1,2,1,4 looks broken.
- Remove **single-pixel bumps** ("jaggies") that stick out of an otherwise smooth
  edge — they are the most common source of a ragged look.

## 6. Anti-aliasing (AA) — manual only
- AA = placing intermediate-value pixels at a stair-step corner to smooth the
  visual edge. Do it **by hand**, with a color **between** the two it bridges
  (ideally already in the ramp), at inside corners of curves.
- AA is for **internal/large curves and key edges**, not the outer silhouette on
  small sprites (AA on the outline against an unknown background creates fringe).
- Don't over-AA: too many in-between pixels turn crisp pixel art into mush. A
  little goes a long way.

## 7. Dithering — sparingly
- **Dithering** = alternating two colors in a pattern (checker, 50%, gradient) to
  fake an intermediate shade or a gradient with a limited palette.
- Use for: large gradients (skies), texture (stone, dirt), or to extend a short
  ramp. Keep the pattern **regular** (clean 50% checker), aligned to the form.
- Avoid: dithering small areas or faces; random/noisy dithering; using it to
  cover up a missing palette step (add the step instead if you have budget).

## Do / Don't
| Do | Don't |
|----|-------|
| 1-px, hue-shifted outline | 2-px flat-black outline on small sprites |
| One light direction; shadow opposite | Pillow-shade a bright halo center |
| Offset shadow from outline stair-step | Run shadow parallel-adjacent (banding) |
| Hand-AA key inside curves with ramp colors | Auto-AA / soft brush; AA everything |
| Regular dither for gradients/texture | Noisy dither on faces or to hide gaps |
