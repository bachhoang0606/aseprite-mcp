---
name: pixel-generate
description: OPT-IN escape-hatch for organic shapes the model can't free-hand (a creature, a detailed character). It first checks whether you even need generation — most pixel art is better DRAWN DIRECTLY — and only if warranted, routes to the cheapest already-available generator (the agent's own image tool, a user reference, an opt-in generator MCP, or local), then runs the pixel-discipline pipeline over the result. We never build/host a model; we orchestrate + discipline. Use ONLY for from-scratch organic subjects; for simple/geometric sprites, icons, edits, animation, or hand-drawn requests, use /pixel-new instead.
argument-hint: "[subject] [--online] [--mcp <server>]"
---

# /pixel-generate — optional hybrid generation (Path 3), discipline-first

The model is weak at **inventing organic shapes from text** (research Path 3/4, the #1 hard case).
Generation fixes that — but it is an **escape-hatch, not the default**. This skill's first job is to
decide you probably **don't** need it; if you do, it uses whatever generator is **already available**
(cheapest first) and then does the part that's actually ours: **disciplining the result into
palette-locked pixel art**. We build no backend and call no API from the server — see SPEC-013.

## 0. Decision gate — do you even need to generate? (usually: NO)
**Draw DIRECTLY (hand off to `/pixel-new` + the perception loop, generate nothing) when the subject is:**
- simple / geometric / iconic — an item, icon, UI element, tile, weapon, potion;
- an **edit / recolour / cleanup** of art that already exists;
- an **animation** of an existing rig (bob / walk / attack / timing);
- a **rule-based tilemap** (autotile / dedupe — use `/pixel-tileset`);
- anything where the user wants **hand-drawn / full pixel control**.

**Only generate when ALL of these hold:**
- the subject is **organic + complex** (a creature, a detailed character/scene), AND
- it must be built **from scratch** (no existing art/rig to edit or animate), AND
- the user wants **high fidelity** and did **not** ask for hand-drawn.

If in doubt, prefer direct drawing — it's the supported default and never costs a paid call. State the
choice ("this is geometric → drawing directly, no generation") so the user can override.

## 1. Preflight
`live_preflight` → require `ready:true` (you import into the LIVE sprite). If it won't connect, run
`/pixel-doctor`. Lock a palette first (`/pixel-palette`) so the import can snap on-model.

## 2. Source ladder — cheapest already-available first
Pick the FIRST available; do not introduce a paid dependency you don't need.
1. **Agent-native image tool** — if your current toolset already has image generation (e.g. Codex
   CLI `$imagegen`, Cursor's image agent), use it to make the organic base. Cost is the user's
   existing plan. *(Note: Claude Code has NO native generation — on this host, skip to 2/3/4.)*
2. **User-supplied reference / video** — if the user already has a concept image, photo, AI render,
   or short clip, use that (free, zero setup). This is often the best path.
3. **Opt-in generator MCP** — only when the user has enabled one and set a key
   (`--mcp <server>`, e.g. a PixelLab / fal / Replicate / local-ComfyUI MCP). **Cost gate here**
   (§4) — confirm opt-in + budget before any paid call.
4. **Local generator** — ComfyUI + a pixel LoRA (e.g. Pixel Art XL), offline / zero marginal cost,
   if the user has it installed.

Pixel-native generators (PixelLab, Retro Diffusion) emit on-grid limited-palette output (less
cleanup); general ones (gpt-image, Gemini/Imagen, FLUX) emit AA raster (needs the full pipeline §3).

## 3. Discipline the result (this is the value-add — always do it)
Whatever the source, bring the base in and make it native to the sheet:
1. **Import** onto a locked Reference layer:
   - still image → `live_import_reference filename="<png>" layer="Reference" snap:true`
     (add `regrid:true` if it's scaled / "fake" pixel art — recovers the native grid; set
     `width`/`height` to your sprite grid or omit for the sprite size / detected native;
     `auto_colors:N` if you have no palette yet).
   - animation (sheet / frame list) → `live_import_animation` (sheet `{cols,rows}` or `frames[]`,
     one shared palette via `palette`/`auto_colors`, `fps`, `tag`).
2. **Lock** it (`live_set_layer_properties layer="Reference" editable:false`) and trace/clean on a
   NEW draft layer above — don't edit the Reference.
3. **Restyle to project-native:** snap to the active palette, fix ramps/light (`/pixel-shade`),
   remove orphans/strays.
4. **Rig / animate** if needed (`/pixel-new` rig, `/pixel-animate`).
5. **Self-review:** `/pixel-review` (rules/06) — the "missing middle step" that turns a generic
   generated base into art native to your sheet. Fix must-fail items before done.

## 4. Cost & licensing gate (paid tier only — §2.3 / paid §2.1)
- **Opt-in + key + budget.** Never make a paid call without an explicit user opt-in (`--online` /
  a configured MCP) and a one-line budget note (rough $/image). Mirrors SPEC-011 network gating.
- **Licensing follows the source:** OpenAI gpt-image — you own the output, commercial OK, no
  watermark; Google Gemini/Imagen — commercial OK but **SynthID watermark**; PixelLab / others —
  per their ToS. Record it; don't relabel a watermarked/owned output as CC0.
- Native (§2.1) and user/local (§2.2/2.4) tiers have no extra paid call — no gate needed there.

## Definition of done
Either: (a) the gate routed to direct drawing and you produced the sprite with `/pixel-new` + the
perception loop (no generation); OR (b) an organic base was generated from the cheapest available
source, imported, disciplined to the active palette, and passes `/pixel-review`. Never force a
generation/reference when the task is better drawn directly, and never make a paid call without opt-in.

## Eval prompts
- "Draw a 16×16 health-potion icon" → gate: geometric → **no generation**, hand to `/pixel-new`.
- "Animate this existing goblin walking" → gate: editing existing art → **no generation**, `/pixel-animate`.
- "I need a from-scratch forest-troll I can't draw" → generate (native tool / user-ref / opt-in MCP /
  local) → `live_import_reference regrid:true snap:true` → `/pixel-shade` → `/pixel-review`.
- "Bring this AI walk GIF in as an animation" → no gen needed; `live_import_animation` + `/pixel-review`.
