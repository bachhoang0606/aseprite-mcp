---
name: pixel-asset
description: Find a free/CC0 asset or palette (Lospec / Hugging Face OpenGameArt) and bring it into the live sprite — search, preview, then import to a locked Reference layer or apply a palette, with attribution captured. Use when the user wants a starting reference, a known palette, or to import/restyle a CC0 asset instead of inventing from scratch.
argument-hint: "[palette <query> | asset <query>] [--online]"
---

# /pixel-asset — search → preview → import (with provenance)

The assets-first workflow (research §F): start from a curated free/CC0 source instead of inventing
organic shapes from text. Orchestrates `tools/asset_search.py` (SPEC-011) + `live_import_reference`
+ `/pixel-palette`. Network is **opt-in** (`--online` or `ASEPRITE_MCP_ALLOW_NET=1`); offline falls
back to the bundled catalog (`knowledge/asset-catalog.json`).

## 0. Preflight
`live_preflight` → require `ready:true` (you're importing into the LIVE sprite). If it won't
connect, run `/pixel-doctor`.

## 1. Search
`python tools/asset_search.py search "<query>" [--source lospec|hf|all] [--limit N] [--online]`
Returns JSON candidates: `{source, kind (palette|asset), id, name, author, license, source_url,
preview_url?, download_url?, colors?}`. Pick one by `id`; note its `kind` and `license`. (Offline
returns the bundled catalog; add `--online` for live Lospec palettes / HF asset search.)

## 2a. Palette  (kind=palette)
1. Fetch into the reusable palette dir so `/pixel-palette` can load it:
   `python tools/asset_search.py fetch <id> --online --out knowledge/palettes`
   → writes `knowledge/palettes/<id>.json` (project format `{name, source, colors}`) + `MANIFEST.json`
   + `CREDITS.txt`.
2. Apply it live: run **`/pixel-palette load <id>`** (it `live_resize_palette` + `live_set_palette_color`
   per index), or do that directly. Confirm with `live_list_palette`.
3. Re-perceive: `live_save_preview` — does the sprite read on the new palette? Re-shade with
   `/pixel-shade` if hues moved.
> A palette is a colour list (not a copyrightable work) — keep the courtesy attribution in CREDITS;
> never relabel it CC0.

## 2b. Asset image  (kind=asset)
1. From an `--online` search result, take the candidate's `download_url`, then fetch WITH provenance:
   `python tools/asset_search.py fetch --url "<download_url>" --name "<name>" --license CC0-1.0 --source hf --online --out assets/imported`
   (HF OpenGameArt assets are CC0-1.0; verify `source_url` for anything else before claiming a licence.)
2. **PREVIEW before committing** — `Read` the downloaded PNG (or import it and `live_save_preview`)
   and confirm it's the asset you want and is usable pixel art.
3. Import onto a **locked Reference layer**:
   `live_import_reference filename="<png>" layer="Reference" snap:true` — add `regrid:true` if it's a
   scaled / "fake" pixel-art image (recovers the native grid before snapping); set `width`/`height` to
   your sprite grid, or omit to use the sprite size / detected native. Snapping to the active palette
   makes it on-model immediately.
4. Lock the reference: `live_set_layer_properties layer="Reference" editable:false` (optionally lower
   its opacity), then trace/clean on a NEW draft layer above it — don't edit the Reference itself.
5. Restyle to project-native: snap to palette, fix ramps/light, then `/pixel-review` — the §F
   "missing middle step" that turns a generic imported asset into art native to your sheet.

## 3. Provenance (always)
Every `fetch` wrote `MANIFEST.json` + `CREDITS.txt` in `--out` — keep them; that's the attribution
record captured at fetch time. Ship `CREDITS.txt` with the project.

## Definition of done
The asset/palette is in the live sprite (palette applied, or asset on a locked Reference layer ready
to trace), provenance captured in `MANIFEST.json`/`CREDITS.txt`, and `/pixel-review` passes on the
restyled result. Never invent from scratch when a fitting CC0 reference is one search away.

## Eval prompts
- "Find a CC0 16-colour palette and apply it" → `search --source lospec`, `fetch` into
  `knowledge/palettes`, `/pixel-palette load`, `live_save_preview`.
- "Use the endesga-32 palette" → `fetch endesga-32 --online --out knowledge/palettes`, apply, re-shade.
- "Import a CC0 tree sprite as a reference" → `search "tree" --source hf --online`, `fetch --url` its
  `download_url`, preview, `live_import_reference` onto a locked `Reference` layer, trace + `/pixel-review`.
