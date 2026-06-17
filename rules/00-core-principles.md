# 00 — Core principles

> Checklist 4.1–4.3. The non-negotiables that everything else builds on.

## 1. Pick a resolution and respect it
- Sprite resolution is a budget. A 32×32 character has ~1024 pixels total; a
  face inside it might be 6×6. **Detail you cannot afford becomes noise.**
- Common sizes: icons 16×16; small characters 24×24–32×32; detailed characters
  48×48–64×64; tiles 16×16 or 32×32. Choose the smallest size that carries the
  silhouette you need, then add detail only if pixels remain.
- **Never** draw at a tiny size by free-handing huge brushes. One logical pixel =
  one pixel. Do not anti-alias by accident with a soft tool.

## 2. Every pixel is intentional
- Each pixel must serve one of: **silhouette**, **form/volume** (light & shadow),
  **outline/separation**, or **a readable detail** (eye, buckle, edge highlight).
- If a pixel serves none of these, it is "lem nhem" — delete it. Stray single
  pixels of an off-color, lone bright dots, and ragged edges are the usual
  culprits behind a muddy look.

## 3. Silhouette first
- Block the **silhouette in one flat color** before any interior detail. If the
  black shape is not instantly recognizable (and distinct from other characters),
  no amount of shading will save it. See `03-proportions-silhouette-3-4-view.md`.
- Recognizable poses read at the extremities: head shape, weapon, stance.

## 4. Light has one source
- Decide the light direction **once** (default: top-left, slightly above) and
  keep it consistent across the whole sprite and every animation frame.
- Lit planes face the light (lighter ramp steps); planes turned away get darker
  steps; the bottom/back edge is darkest.

## 5. Contrast where it matters
- Spend your strongest value contrast on **focal points** (face, weapon, hands).
- Keep low-importance areas (boots, back of a cloak) lower-contrast so the eye
  goes where you want.

## 6. Work flat → render
1. Silhouette (1 color) → 2. local/base colors per region → 3. core shadow →
   4. highlight → 5. selective outline & AA cleanup → 6. accents (eyes, metal glint).
- Do not jump to highlights before the base shape and shadow read correctly.

## 7. Zoom out to judge
- Pixel art is judged at 100% (1:1), not zoomed in. A cluster that looks busy at
  800% may read fine at 100% — and vice versa. Always sanity-check at 100%.

## Do / Don't
| Do | Don't |
|----|-------|
| Lock size, then budget detail | Cram facial detail into a 4-px face |
| Block flat silhouette first | Start with shading on an unsure shape |
| One light direction everywhere | Shade each part with its own light |
| Delete pixels that explain nothing | "Fix" muddiness by adding more pixels |
| Judge at 100% | Judge only zoomed-in |
