# Visual-regression fixtures

Checklist 9.3. Stdlib-only (no Pillow); PNG I/O via `tools/pixelpng.py`.

- `gen_fixtures.py` — regenerates the fixtures deterministically.
- `fixtures/` — test sprites: `good_swatch.png` (clean, on-palette),
  `bad_offpalette.png` (one off-palette pixel), `bad_orphan.png` (a stray pixel).
- `golden/` — approved reference images. `good_swatch.png` here is byte-identical
  to the fixture and is the diff baseline.
- `diff.py` — `python tests/visual/diff.py actual.png golden.png [--tolerance N]`
  compares pixel-by-pixel, writes a magenta-highlighted diff image on mismatch,
  and exits non-zero so CI catches unintended art changes.

## Workflow
1. Produce/export a sprite PNG (e.g. via `/pixel-export`).
2. Diff against its golden. If the change is intended, copy the new PNG over the
   golden and commit; otherwise fix the regression.

## Honest scope
These goldens are deterministic Python-rendered fixtures that exercise the diff +
linter pipeline without Aseprite. Golden-ing **real** Aseprite exports (re-export
+ diff) is wired conceptually but needs Aseprite, so it runs locally/manually, not
in headless CI.
