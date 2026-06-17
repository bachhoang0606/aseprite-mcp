# 01 — Palette & color

> Checklist 4.1. Palette discipline, ramps, and hue-shifting. This is the single
> biggest lever on whether a sprite looks professional or muddy.

## 1. Lock a palette before drawing
- **Decide the full palette first** and draw only from it. Never pick colors
  ad-hoc with the color picker mid-draw — that is the #1 cause of muddy sprites.
- Keep the palette small. Targets: tiny sprite ≤ 8 colors; character ≤ 16;
  detailed scene ≤ 32. Fewer colors → more cohesion.
- Reuse colors across regions (shared shadow/outline color) to bind the sprite
  together. See `knowledge/palettes/` for ready-made palettes.

## 2. Build ramps, not isolated colors
- A **ramp** is an ordered sequence of colors from dark → light for one material
  (skin, cloth, metal). Each region is shaded by stepping along its ramp.
- A character usually needs **3–5 steps per ramp**. 2 is flat; >5 on a small
  sprite wastes palette and reads as banding/AA mush.
- Ramps may **share endpoints**: the darkest step of several ramps can be the
  same near-black; highlights can converge on a shared warm white. This shrinks
  the palette and unifies the lighting.

## 3. Hue-shift along every ramp (do NOT just darken/lighten)
The professional move: as a color goes **darker, shift its hue toward the cool
end (blue/purple) and lower saturation slightly**; as it goes **lighter, shift
toward the warm end (yellow/orange)**. Also rotate hue across the ramp, not only
value.

- Shadow of green skin → green-blue / teal, not just darker green.
- Highlight of green skin → yellow-green, not just lighter green.
- Red cloth shadow → maroon shifting toward purple; highlight → orange.

**Why:** ambient/sky light is cool, direct/warm light is warm; flat value-only
ramps look gray and lifeless. Hue-shifting is what makes pixel color "pop."

```
BAD  (value only):   #2e7d32 → #43a047 → #66bb6a   (all same hue, flat)
GOOD (hue-shifted):  #1b4d3e → #2e7d32 → #6abe30   (dark=teal-ish, light=yellow-green)
```

## 4. Saturation & value rules of thumb
- Avoid pure `#000000` for shadows and pure `#ffffff` for highlights — they kill
  hue. Use a very dark hue-shifted color and a warm off-white instead.
- Mid-tones carry the most saturation; the extremes (deep shadow / hot highlight)
  are usually a bit **less** saturated.
- Keep enough **value separation** between ramp steps that they read as distinct
  at 100%. Steps that are too close = wasted colors and a blurry look.

## 5. Color identity & focal contrast
- Give the character **one or two hero hues** (identity color). Keep accents rare
  so they read as accents (e.g., a single warm eye/gem against cool armor).
- Put the **highest chroma + value contrast on the focal point** (face, weapon).

## 6. Palette-discipline check (must pass)
- [ ] All pixels come from the locked palette (no stray off-ramp colors).
- [ ] Each material is a ramp of 3–5 hue-shifted steps.
- [ ] Darkest/ lightest are hue-shifted, not pure black/white.
- [ ] Ramp steps are visually distinct at 100%.
- [ ] Total color count within budget for the sprite size.

See also: `knowledge/glossary.md` (ramp, hue-shift, banding) and
`knowledge/palettes/` (NES, Game Boy, PICO-8, project default).
