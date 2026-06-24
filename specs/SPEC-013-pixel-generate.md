# SPEC-013 — `/pixel-generate` (optional hybrid-generation orchestration; Path-3 escape-hatch)

- Status: **Implemented (2026-06-25)**
- Owner: project
- Checklist items advanced: 5.x (a `/pixel-*` skill), 6.x (hybrid generation — research Path 3),
  10.x (network/cost egress discipline).
- Related ADRs: ADR-0003 (opt-in gate pattern — reused for paid-generation egress + cost). **No new
  ADR, no new tool, no new dependency, no new plugin command** — this is a docs-only orchestration
  skill (like SPEC-011 `/pixel-asset`).
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  **Path 3 "hybrid generation"** — the *free / no-backend* realization: we never build or host a
  model; we **orchestrate whatever generator is already available** and add the pixel discipline.

## Intent
Path 3 is the only route to genuinely **organic** shapes the LLM can't free-hand (a creature, a
detailed character). But it is the **escape-hatch for one hard case**, NOT the default: most pixel-art
work — simple/geometric sprites, icons, edits, recolours, animating existing art, rule-based tilemaps —
is *better drawn directly* with the perception + constrained-drawing + palette-discipline loop the
project already ships. So `/pixel-generate` is an **opt-in branch that begins by refusing itself** when
generation isn't warranted, then routes to the **cheapest already-available** generator, and — its real
value — runs the deterministic pixel pipeline over the result. **The generator is a commodity we
consume; our moat is the agent + discipline + live-editing layer around it** (cf. PixelLab et al. that
already generate in Aseprite — we compose/import, we do not compete on generation).

## Inputs / Outputs
- **Inputs:** an optional subject prompt; flags for the paid tier (`--online`, a configured
  generator MCP, an API key). No new tool params — the skill calls existing live tools + scripts.
- **Outputs:** when generation *is* warranted, a palette-locked result drawn into the open sprite via
  `live_import_reference` / `live_import_animation`, then disciplined (regrid / snap / lint / rig /
  `/pixel-review`). When it is *not* warranted, the skill hands off to `/pixel-new` (direct drawing)
  and generates nothing.

## Behaviour
1. **Decision gate (first, mandatory).** Generation is OFF the main path. Use it ONLY when the subject
   is **organic + complex** AND from-scratch fidelity matters AND the user did not ask for hand-drawn.
   Otherwise → **draw directly** (`/pixel-new` + the perception loop), generate nothing.
2. **Source ladder (cheapest-available first).** When generation is warranted, in order:
   1. the **agent's own native image-gen tool** if present (Codex CLI `$imagegen`, Cursor, …) — cost
      is the user's existing plan. *(Empty on Claude Code, which has no native generation → falls
      through.)*
   2. a **user-supplied reference / video** they already have → `live_import_reference` /
      `live_import_animation` (free, zero setup).
   3. an **opt-in generator MCP** (PixelLab / fal / Replicate / a local ComfyUI MCP) — **only** when the
      user has enabled it and provided a key. **This tier is where the cost gate applies.**
   4. a **local generator** (ComfyUI + a pixel LoRA) — offline, zero marginal cost, if installed.
3. **Always discipline the result.** Whatever the source, run the pixel pipeline: `live_import_*`
   (with `regrid:true` for scaled/"fake" pixel output), palette-snap to the active palette, lint,
   rig/animate as needed, then `/pixel-review`. General generators (gpt-image, Gemini) output AA raster
   → the full pipeline is required; pixel-native generators (PixelLab/RD) need lighter cleanup.
4. **Cost & licensing gate (paid tier only).** Mirror SPEC-011 network gating: explicit opt-in + key +
   a per-image budget note before any paid call. Record output licensing per source (OpenAI = output
   ownership/no watermark; Gemini/Imagen = SynthID watermark; a provider's ToS otherwise).

## Decisions
1. **Generation is optional and self-refusing.** The skill's first job is to say "you don't need me"
   for the common case — never force a reference/generation.
2. **No backend, no API client in our server.** The call lives *outside* the Rust server (agent-native
   / user / opt-in MCP / local). This keeps the lean-deps + Windows-SAC invariants (no HTTP crate, no
   key handling, no relink) and reflects our actual position (discipline layer, not a generator).
3. **Reuse, don't add.** Orchestrates `live_import_reference`/`live_import_animation` (SPEC-006/012),
   `/pixel-palette`, `/pixel-shade`, `/pixel-review` — no new tool/plugin command.

## Acceptance criteria
- [x] `skills/pixel-generate/SKILL.md`: a **decision-gate that routes most tasks to direct drawing**,
      the 4-tier source ladder, the post-gen discipline pipeline, and the cost/licensing gate. Listed
      in `skills/README.md`.
- [x] No new dependency, no new plugin command, no new live tool (docs-only orchestration).
- [x] References only tools/skills that exist (import_reference/import_animation, /pixel-*).

## Eval (how we grade it)
- **Routing (the point):** "draw a 16×16 health-potion icon" → the gate says *no generation*, hand to
  `/pixel-new`. "I need an organic forest-troll base I can't draw" → generation path, cheapest
  available source, then discipline + `/pixel-review`.
- **Live (on-demand):** with a generator available, generate → import → snap → review yields an
  on-palette sprite native to the sheet; with none, the skill draws directly and never blocks.

## Traceability
- Skill: `skills/pixel-generate/SKILL.md`; index `skills/README.md`. Reuses
  `live_import_reference`/`live_import_animation`, `/pixel-palette`, `/pixel-shade`, `/pixel-review`,
  `/pixel-new`. No `src/` or `plugin.lua` change.
