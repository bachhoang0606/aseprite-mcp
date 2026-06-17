# Knowledge base

Curated, machine-readable reference data the rules and skills draw on.
Checklist pillar **8. Knowledge base**.

## Contents
| Path | What | Checklist |
|------|------|-----------|
| [`palettes/`](palettes/) | Ready-to-load palettes (JSON), each with ramps + citation | 8.1 |
| [`glossary.md`](glossary.md) | Definitions of pixel-art terms used by the rules | 8.3 |
| [`references/`](references/) | Subject reference conventions (e.g. goblin, 3/4 view) | 8.2 |

## Palette JSON format
Each file in `palettes/` is:
```json
{
  "name": "PICO-8",
  "source": "Lexaloffle PICO-8 fantasy console (official 16-color palette)",
  "colors": ["#000000", "..."],
  "ramps": { "skin": ["#...","#..."], "...": [] },
  "notes": "..."
}
```
- `colors` — the flat palette (load order).
- `ramps` — optional named dark→light sequences for shading (see
  `rules/01-palette-and-color.md`).
- `source` — citation so palettes are verifiable, not invented.

Skills (`/pixel-palette`, `/pixel-new`) load these; the agent must shade along the
named ramps rather than picking colors ad-hoc.
