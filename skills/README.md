# Skills — the `/pixel-*` verbs

User-facing slash commands that turn the `rules/` expertise + `knowledge/` data
into repeatable actions, executed **live** in the open Aseprite window. Checklist
pillar **5. Skills**.

| Skill | Does | Rules it enforces |
|-------|------|-------------------|
| [`/pixel-new`](pixel-new/SKILL.md) | Scaffold a sprite: size, palette, rigged layers | `00`, `01`, `05` |
| [`/pixel-palette`](pixel-palette/SKILL.md) | Set / load / optimize a palette with ramps | `01` |
| [`/pixel-shade`](pixel-shade/SKILL.md) | Apply hue-shifted ramp shading to a layer | `01`, `02` |
| [`/pixel-animate`](pixel-animate/SKILL.md) | Build idle/walk/attack frames + tags from a rig | `04`, `05` |
| [`/pixel-export`](pixel-export/SKILL.md) | Export PNG/GIF/spritesheet (+ JSON meta) | `06` (output) |
| [`/pixel-review`](pixel-review/SKILL.md) | Critique a sprite against the rulebook, scored | `06` (all) |
| [`/pixel-reference-motion`](pixel-reference-motion/SKILL.md) | Rotoscope a video/GIF/frame sequence into a clean animation (trace over a per-frame reference) | `01`, `04` |

## Non-negotiable for every skill
1. **Preflight first.** Call `live_preflight`; proceed only when `ready: true`.
   If false, STOP and report — never fall back to batch/file tools to "work
   around" it (see `docs/adr/0001-batch-vs-live-tools.md`).
2. **Palette before pixels.** Lock a palette/ramps (`/pixel-palette`) before any
   drawing or shading; draw only from it.
3. **Rig before animation.** Limbs on their own layers (`rules/05`) before
   `/pixel-animate`.
4. **Self-review before done.** Run the `/pixel-review` rubric (`rules/06`) and
   fix must-fail items before declaring complete.

Grounding for the techniques these skills apply:
[`knowledge/references/pixel-art-sources.md`](../knowledge/references/pixel-art-sources.md).

## Conventions (installed-plugin correctness)
- **Namespaced names:** once installed these invoke as
  `/aseprite-pixel-art:pixel-new` … `:pixel-review`. Written `/pixel-*` here for
  brevity (Claude Code namespaces plugin skills to avoid collisions).
- **Path resolution:** a plugin is copied to a cache dir, so file references like
  `rules/06-...` / `knowledge/...` resolve **under the plugin root**. When a path
  must be exact (reading a rule file, running a script), use
  `${CLAUDE_PLUGIN_ROOT}/rules/...` rather than a bare relative path. In local
  `--plugin-dir ./` mode the repo root is the plugin root, so bare paths also work.
