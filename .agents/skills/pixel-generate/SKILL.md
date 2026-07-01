---
name: pixel-generate
description: OPT-IN escape-hatch to make pixel art from a GENERATED or supplied image / sprite sheet — generate (or take a reference), then discipline it into palette-locked pixels. Use ONLY for organic-from-scratch subjects (a creature, a detailed character/scene) or when turning an AI image / sheet into game-ready pixels. For simple / geometric sprites, edits, or animating an existing rig, use $pixel-new / $pixel-animate instead.
---

# pixel-generate — image/sheet → disciplined pixel art (opt-in)

The model is weak at inventing organic shapes from text; generation fixes that — but it
is an **escape-hatch, not the default**. First decide you probably DON'T need it; if you
do, use the cheapest available generator, then do the real value-add: **discipline the
result into palette-locked pixels**. No backend is built — we orchestrate + discipline.

## 0. Decision gate (usually: draw directly)
Draw directly (`$pixel-new` + the perception loop, generate nothing) when the subject is
simple / geometric / iconic, an **edit / recolour**, an **animation of an existing rig**,
or the user wants **hand-drawn** control. Only generate when ALL hold: **organic + complex**,
**from scratch**, **high-fidelity**, and not asked hand-drawn. State the choice so the user can override.

## 1. Preflight + palette
`live_preflight` → `ready:true` (else `$pixel-doctor`). Lock a palette first (`$pixel-palette`)
so the import can snap on-model.

## 2. Source ladder — cheapest available first
1. **Agent-native image tool** — on **Codex, use `$imagegen`** to make the organic base or
   sheet (cost = your plan). *(Claude Code has no native generation → skip to 2/3/4.)*
2. **User-supplied reference** — an existing concept image / photo / AI render / short clip (free, often best).
3. **Opt-in generator MCP** — PixelLab / fal / Replicate / local ComfyUI. **Cost gate:** confirm
   opt-in + a rough $/image note before any paid call.
Pixel-native generators (PixelLab, Retro Diffusion) emit on-grid limited-palette output (less
cleanup); general ones (gpt-image, Imagen, FLUX) emit AA raster (needs the full §3 cleanup).

## 3. Import + discipline (the value-add — always do it)
- **Single sprite** → `live_import_reference filename=<png> layer="Reference" snap:true`
  (add `regrid:true` if it's scaled / "fake" pixel art; set `width`/`height` to your sprite
  grid; `auto_colors:N` if you have no palette yet).
- **Sprite sheet / animation** → `live_import_animation` (sheet `{cols,rows}` or `frames[]`,
  one shared `palette` / `auto_colors`, `fps`, `tag`) — slices the sheet into frames + a tag.
- **Lock** the Reference layer (`editable:false`); trace / clean on a NEW layer above it.
- **Restyle native:** snap to the palette, fix ramps/light (`$pixel-shade`), remove orphans/strays.
  > Note: downscaling a big AI sheet → small clean sprite is still basic (dominant/average + regrid);
  > expect manual cleanup. A dedicated downscaler/alpha-cleanup is planned (gap-analysis Group B).

## 4. Cost & licensing gate (paid tier only)
Never make a paid call without explicit user opt-in + a one-line budget note. **Licensing follows
the source:** gpt-image — you own the output, commercial OK, no watermark; Imagen — commercial OK
but **SynthID watermark**; PixelLab/others — per their ToS. Record it; don't relabel a
watermarked/owned output as CC0.

## 5. Rig / animate / review
`$pixel-animate` if rigging further, then `$pixel-review` (fix must-fails) — the missing middle
step that turns a generic generated base into art native to your sheet.

## Done
Either the gate routed to **direct drawing** (no generation), OR an organic base was
generated/supplied, imported, disciplined to the palette, and passes `$pixel-review`. Never
force generation when the task is better drawn; never a paid call without opt-in.
