# SPEC-011 — asset search (CC0 assets + Lospec palettes, stdlib, gated)

- Status: **Implemented (2026-06-22)**
- Owner: project
- Checklist items advanced: 6.x (assets-first search/import), 10.x (network egress discipline /
  provenance capture)
- Related ADRs: ADR-0003 (opt-in gate pattern — reused for network egress). No new ADR (a stdlib
  Python tool like the other `tools/`, no new protocol/return contract).
- Source: research doc [`docs/research/agent-pixel-art-techniques.md`](../docs/research/agent-pixel-art-techniques.md)
  §F "Assets-first — search / import / restyle CC0 assets" (line ~475), roadmap **#12**.

## Intent
blender-mcp's viral demos are **scene assembly from asset libraries**, not modeling from
primitives (research §B/§F). The 2D analog: let the agent **search** curated free/CC0 sources —
**Lospec** for palettes (turns `knowledge/palettes/` from a handful into thousands) and the
**Hugging Face `nyuuzyou/OpenGameArt-CC0`** dataset for 15.7k CC0 art assets — preview a
candidate, then fetch it into the project with provenance captured at fetch time. Imported
assets then ride the existing pipeline (`live_import_reference` to a locked Reference layer →
re-palette/restyle to project-native; palettes apply via `live_set_palette_color`).

Hard constraints (carry forward): **lean deps / Windows-SAC** — a stdlib-only Python tool (like
`tools/regrid.py` et al.), **no new Rust HTTP crate**, and crucially **no `requests` / `duckdb` /
`pyarrow`** (the HF dataset is Parquet, but its REST **datasets-server** returns JSON, so
`urllib` suffices). Network egress is **opt-in gated** with a **no-API default** so the tool is
useful — and CI-testable — with zero network.

## Inputs / Outputs
- **Tool:** `python tools/asset_search.py <command> ...` (stdlib only).
  - `search <query> [--source lospec|hf|all] [--limit N] [--tag T] [--online]` — returns a JSON
    list of normalized candidates.
  - `fetch <id> [--source ...] [--out DIR] [--online]` — materializes one candidate into `DIR`
    and appends provenance to `DIR/MANIFEST.json` + `DIR/CREDITS.txt`. A Lospec palette resolves by
    slug (any slug, online), a catalog/local palette offline.
  - `fetch --url <download-url> [--name --author --license --source] [--out DIR] --online` —
    completes the HF flow: download an asset URL (e.g. a `download_url` from `search --source hf
    --online`) WITH provenance. Default license for a `--url` fetch is `"unknown — verify at
    source_url"` (the caller passes `--license CC0-1.0` for HF dataset assets).
  - `--selftest` — offline self-checks (exit non-zero on failure; CI-wired).
- **Normalized candidate (search output):**
  `{source ("lospec"|"hf"|"local"), kind ("palette"|"asset"), id, name, author, license,
  source_url, preview_url?, download_url?, colors? (palettes), tags?}`. Stable, source-agnostic
  shape so the agent (or a future skill) treats all sources uniformly.
- **`MANIFEST.json`** (a JSON list appended to): one record per fetch —
  `{kind, source, id, name, author, license, source_url, file, colors?, fetched_offline (bool)}`.
- **`CREDITS.txt`**: one human-readable attribution line per fetched asset.

## Behaviour
- **Network gate.** Egress is allowed only when `--online` is passed **or** the env flag
  `ASEPRITE_MCP_ALLOW_NET` is truthy (`1/true/yes/on`) — mirrors the ADR-0003 `ASEPRITE_MCP_ALLOW_LUA`
  gate. Off by default. When off:
  - `search` falls back to the **offline default catalog** (`knowledge/asset-catalog.json`) — a
    curated set of CC0/free palette + source pointers — so it still returns useful results.
  - `fetch` of an online-only candidate returns a **loud, actionable error** (name the flag),
    never a silent no-op; a catalog candidate with a local file still fetches offline.
- **Lospec source (palettes).** Online: a single palette resolves by slug via the documented JSON
  API `https://lospec.com/palette-list/{slug}.json` → `{name, author, colors:[hex…]}`, normalized
  to a candidate (and to the project palette JSON on `fetch`). Search by query/tag filters the
  offline catalog's Lospec entries (Lospec has no stable public *search* JSON endpoint — documented
  limitation), and resolves colours on demand when `--online`.
- **HF source (CC0 assets).** Online: the **datasets-server** REST API (JSON, no Parquet reader)
  `https://datasets-server.huggingface.co/search?dataset=nyuuzyou/OpenGameArt-CC0&config=...&split=...&query=…`
  → rows parsed **defensively** (title/name, license, preview/download URL extracted from whatever
  recognizable fields exist; unparseable rows skipped, not fatal). All rows are CC0 by dataset
  construction. `fetch` downloads the asset image into `DIR`.
- **Provenance (always).** Every `fetch` appends a `MANIFEST.json` record and a `CREDITS.txt` line
  capturing source/id/author/license/url at fetch time (the ULPC pattern, §F) — so attribution is
  never reconstructed later. **License is recorded accurately per source:** HF dataset assets =
  `CC0-1.0`; Lospec palettes = `"color list — not copyrightable; attribution courtesy"` (a list of
  colours is not a copyrightable work — we record author + source for courtesy, and do **not**
  overclaim CC0 for palettes).
- **Determinism / safety.** All output is sorted and reproducible; downloads are size-capped and
  written only under `--out`; URL schemes are restricted to `http(s)`; the tool never executes
  fetched content. Pure parse/normalize/manifest logic is unit-tested offline.

### Decisions
1. **Stdlib + datasets-server JSON, not Parquet.** Reading the Parquet dataset needs
   pyarrow/duckdb (new deps); the HF datasets-server REST API returns the same rows as JSON, so
   `urllib`+`json` suffice — honours lean-deps with no SAC relink.
2. **Gate + offline catalog (no-API default).** Network is opt-in; the bundled catalog makes the
   tool useful and CI-testable with zero egress. CI never hits the network (tests are fixture- and
   catalog-based).
3. **Tool now, skill later.** This ships the deterministic stdlib tool + spec + tests (the project
   pattern: SPEC-006 tool → `/pixel-reference-motion` skill in a follow-up PR). A `/pixel-asset`
   skill that orchestrates search → preview → `live_import_reference` is a documented follow-up.
4. **Honest licensing.** CC0 only where the source guarantees it (the HF dataset); palettes are
   recorded as colour lists with courtesy attribution, not relabelled CC0.

## Acceptance criteria
- [x] `tools/asset_search.py` is stdlib-only (no third-party imports), with `search` / `fetch` /
      `--selftest` and a module docstring + argparse, matching the other `tools/` conventions.
- [x] Network is gated (`--online` / `ASEPRITE_MCP_ALLOW_NET`); **default offline** `search` returns
      results from `knowledge/asset-catalog.json`; offline `fetch` of an online-only candidate errors
      loudly and names the flag.
- [x] Lospec palette JSON and HF datasets-server rows parse into the normalized candidate shape
      (defensive HF parsing skips unparseable rows); `fetch` writes the file + appends `MANIFEST.json`
      + `CREDITS.txt` with **per-source-accurate** license strings.
- [x] `--selftest` and `tests/test_asset_search.py` run **offline** (fixtures + catalog) and are
      wired into the `quality` CI job; no network in CI.
- [x] No new dependency (Python stdlib only; no Rust change).

## Eval (how we grade it)
- **Deterministic (Tier-A, no network):** unit tests — Lospec/HF fixture JSON → normalized
  candidates; offline catalog search filters by query/tag/source; the gate blocks egress by default
  and the error names the flag; `MANIFEST.json`/`CREDITS.txt` carry the right fields and the correct
  per-source license; URL-scheme guard rejects non-http(s).
- **Live (manual, opt-in):** with `--online`, `search "endesga"` returns Lospec palettes;
  `fetch endesga-32 --online` writes `endesga-32.json` + manifest/credits; `search "tree" --source hf
  --online` returns CC0 assets with preview URLs.

## Traceability
- Module(s): `tools/asset_search.py` (search/fetch/selftest; Lospec + HF parsers; gate;
  manifest/credits), `knowledge/asset-catalog.json` (offline default catalog).
- Test(s): `tests/test_asset_search.py` (parsers, catalog search, gate, manifest/credits, URL guard);
  `--selftest`; wired into `.github/workflows/quality.yml`.
