# Pixel-art glossary

Terms used across `rules/` and the skills. Kept short and operational.

- **Aliasing / jaggies** — the stair-step look of a diagonal/curve on a pixel
  grid; "jaggies" specifically are stray single-pixel bumps that break an
  otherwise smooth edge. Remove them for clean lines.
- **Anti-aliasing (AA)** — manually placing intermediate-value pixels at a
  stair-step corner to smooth the visual edge. Use ramp colors, by hand, on key
  inside curves; don't over-apply or AA the outer silhouette on small sprites.
- **Banding** — two stair-stepped edges running parallel and adjacent so their
  jaggies align into a thick, noisy seam. Avoid by offsetting shadow from outline.
- **Breakdown / in-between** — a frame between two key poses; defines how motion
  travels (arc, ease). Add only where smoothness is needed.
- **Cast shadow** — shadow one object throws onto another (chin onto neck). Harder
  edge, darker than form shadow.
- **Cluster** — a group of same-color pixels read as one shape. Good pixel art is
  made of intentional clusters, not scattered pixels.
- **Dithering** — alternating two colors in a pattern (checker/gradient) to fake
  an intermediate shade or gradient with a limited palette. Use regularly and
  sparingly.
- **Form shadow** — the gradual darkening as a rounded surface turns away from the
  light; transitions along the ramp.
- **Hue-shift** — rotating hue (not just value) along a ramp: shadows toward cool
  (blue/purple), highlights toward warm (yellow/orange). The key to lively color.
- **Key pose / extreme** — the defining pose of an action (full wind-up, full
  strike). Animate keys first.
- **Lem nhem** (Vietnamese: "muddy/smudged") — this project's term of art for
  noisy, unreadable pixel work: stray pixels, off-ramp colors, banding, mushy
  clusters. The quality bar in `rules/00` and `rules/02` is its absence.
- **Lospec** — community palette/reference site; common source for cited palettes.
- **Outline (full vs selective)** — full = 1-px dark line around the whole
  silhouette; selective (sel-out) = outline only where contrast is needed.
- **Pillow-shading** — shading concentrically inward (bright center halo) with no
  light direction. An amateur tell; avoid.
- **Pivot / registration** — the consistent anchor point (e.g. feet-center) that
  keeps frames and layers aligned.
- **Ramp** — an ordered dark→light sequence of colors for one material; you shade
  by stepping along it. Usually 3–5 steps.
- **Readability** — how instantly the sprite/pose is understood, especially as a
  flat silhouette at 100%.
- **Rig** — the set of layers (limbs/parts) a sprite is built from so it can be
  animated by moving/redrawing layers.
- **Silhouette** — the flat outer shape; the first test of a design.
- **Smear** — an elongated/blurred shape on a fast frame to convey speed (e.g. a
  weapon arc) without the shape collapsing.
- **Squash & stretch** — deforming a shape to show force/impact while preserving
  its volume/mass.
- **Sub-pixel / 1:1** — pixel art is authored and judged at 100% (one logical
  pixel = one screen pixel); always sanity-check there.
- **Tangent** — two edges that just barely touch, flattening depth; avoid by
  adding a gap or a clear overlap.
- **3/4 view** — camera rotated ~45° and tilted down; shows front + one side + a
  little top. Asymmetric and planar-shaded. See `rules/03`.
