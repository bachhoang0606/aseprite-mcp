# Pixel-art sources & synthesized techniques

Real-world references behind `rules/` and `skills/`. Synthesized from authoritative
pixel-art education, with citations so the guidance is verifiable rather than
invented. Where these confirm or sharpen a rule, the rule file is the canonical
copy; this file records *why* and *where it came from*.

## Sources
- **Aseprite official docs — Shading.** The shading ink shifts a pixel along a
  chosen gradient/ramp: left-click moves toward shadow, right-click toward light;
  the base color must be inside the gradient. <https://www.aseprite.org/docs/shading/>
- **Derek Yu — Pixel Art Tutorial (Common Mistakes).** Canonical list: too many
  similar colors, naive coloring, pillow shading, cardboard/stiff designs; plus
  the "chunky pixels rule" (avoid 1-px-thick features) and "each pixel is a
  decision." <https://www.derekyu.com/makegames/pixelart2.html>
- **Pedro Medeiros / Saint11 — Pixel Art Tutorials.** 70+ tutorials incl.
  anti-alias & banding, animation, consistency, shine, motion.
  <https://saint11.art/blog/pixel-art-tutorials/> ·
  <https://lospec.com/pixel-art-tutorials/author/pedro-medeiros> ·
  <https://medium.com/pixel-grimoire/how-to-start-making-pixel-art-4-ff4bfcd2d085>
- **Pixel Parmesan — Anti-Aliasing Fundamentals.**
  <https://pixelparmesan.com/blog/anti-aliasing-fundamentals-for-pixel-artists>
- **Lospec — hue-shifting & dithering tutorial tags.**
  <https://lospec.com/pixel-art-tutorials/tags/hueshifting> ·
  <https://lospec.com/pixel-art-tutorials/tags/dithering>
- **Pixel-Editor.com — Sprite Animation Fundamentals (timing, walk cycles).**
  <https://www.pixel-editor.com/articles/sprite-animation-fundamentals>
- **Generalist Programmer — Aseprite Complete Professional Guide** (palette,
  shading, dithering, animation, quality checklist).
  <https://generalistprogrammer.com/tutorials/aseprite-complete-professional-pixel-art-guide>

## Synthesized technique notes (cross-checked across sources)

### Color & palette
- Limited palettes for cohesion: ~5–8 core colors per character; ramps = base +
  2–4 steps. Give **each color its own identity** (Derek Yu) — avoid near-dupes.
- **Hue-shift**: darken → toward blue/purple (cooler); lighten → toward
  yellow/orange (warmer). Value-only ramps look muddy (Aseprite/Lospec).
- Avoid **naive coloring** (pure stereotyped colors); account for reflected /
  ambient light (Derek Yu).

### Shading
- Cel/hard shading: distinct steps (~30–40% darker shade, ~20–30% lighter
  highlight), shadows as separated masses.
- Form shading: light wraps 3D forms — cylinders/spheres get gradual transitions,
  flats stay uniform. Think in primitives (sphere/cylinder/cube), not flat shapes.
- Subsurface/backlight: warmer light on thin backlit edges (ears, fabric) for
  64×64+ only.
- Aseprite **shading ink** operationalizes ramps: pick a gradient incl. base,
  left-click=shadow / right-click=light along it.

### Dithering
- Creates intermediate shades/gradients from 2 colors via patterns: 50% checker,
  75/25, gradient, noise. Best for metal shine, large gradients, texture.
- **Don't over-dither below 32×32** — it reads as noise at game scale.

### Anti-aliasing
- Add intermediate pixels on diagonals/curve inflections to smooth stair-steps;
  apply where edges contrast strongly with background.
- **Don't** AA horizontal/vertical lines, sprites < 32×32, or sharp mechanical
  objects. Don't introduce *accidental* AA.

### Linework, jaggies, banding, pillow (the muddiness culprits)
- **Jaggies:** keep equal run-length per step; a smooth curve follows a steady
  pattern. Remove lone bumps.
- **Banding:** values lining up in parallel bands along an outline; reinforces the
  grid and flattens form. Break with offset/cluster/dither.
- **Pillow shading:** shading inward from the outline with no light direction →
  blurry, depth-less. Fix with one clear light source.
- **Orphan pixels / doubles:** single disconnected pixels and accidental doubled
  edges break cohesion — clean them. **Chunky pixels rule:** no 1-px-thick limbs.

### Proportions & silhouette
- Silhouette must read at **100% zoom**; iconic shapes beat detail. Simplify and
  exaggerate at low res. Sizes: 16 (GB), 32 (platformer baseline), 64 (detailed),
  128 (hi-res).
- Outlines: 1-px around the silhouette + internal lines separating major forms
  (head/body, arm/torso). (Project rule prefers hue-shifted dark over flat black —
  `rules/02`.)

### Animation
- **Keys first, then in-betweens** (pose-to-pose), don't straight-ahead.
- Walk = 4-frame minimum: Contact (body lowest) → Passing (body highest) →
  Contact (other foot) → Passing; frames 3/4 can mirror 1/2.
- Timing via per-frame ms in Aseprite: e.g. walk ~120–150 ms; attack wind-up
  ~80 ms, hold impact ~300 ms, hold pre-attack anticipation ~400 ms.
- **Anticipation** sells big actions (even 1 frame before a jump). Squash/stretch
  ±1 px preserves volume. **Onion-skinning** keeps volume/arcs consistent.

### Quality evaluation (used by `rules/06` and `/pixel-review`)
Cross-source checklist:
- Silhouette reads at 100%; all pixels connected (no orphans).
- Palette limited & each color has identity; ramps consistent across sprites.
- One light direction; hue-shifted ramps (cool shadow / warm highlight).
- No pillow-shading, no banding, no jaggies, no accidental AA, no 1-px limbs.
- Animation: uniform/intentional timing, consistent proportions per frame,
  onion-skin-verified smooth motion.
- Meta-question (Derek Yu): "what stands out, what to improve, what seems wrong
  but works?" — treat as ongoing craft, each pixel a decision.
