# Traceability matrix

> Checklist 12.4. Links **spec → code → test/eval → checklist item** so any change
> can be traced to its rationale and its coverage. Reference this in PRs:
> "advances 7.2; spec SPEC-002; tests evals/run.py::auto_preview".

## Specs & ADRs → implementation → verification → checklist

| Spec / ADR | Implementation | Test / Eval | Checklist |
|---|---|---|---|
| [SPEC-001](../../specs/SPEC-001-decoupled-ws-bridge.md) · [ADR-0002](../adr/0002-decouple-ws-bridge.md) | `src/bin/aseprite-live-bridge.rs`, `src/live.rs` (control-client) | `tests/bridge_loopback.rs`; `src/live.rs::tests` (preflight/refuse) | 2.4, 2.5, 3.1 |
| SPEC-001 (timeout tuning) | `src/live.rs::request_timeout_ms` (`ASEPRITE_MCP_LIVE_TIMEOUT_MS`) | `src/live.rs::tests::request_timeout_is_env_tunable_with_safe_floor` | 2.5 |
| [ADR-0001](../adr/0001-batch-vs-live-tools.md) | `src/server.rs` (live vs batch), `hooks/guard_batch_draw.py` | `evals/run.py::guard_decisions` | 2.1, 2.2, 7.1 |
| [SPEC-002](../../specs/SPEC-002-lua-execution-opt-in-gate.md) · [ADR-0003](../adr/0003-run-lua-script-security.md) | `src/tools/scripting.rs` (`lua_execution_allowed` gate), `src/server.rs` (tool descriptions), `SECURITY.md` | `src/tools/scripting.rs::tests` (gate truthy/falsy/error) | 10.1, 10.3, 10.4 |
| SPEC-002 (localhost binding) | `src/bin/aseprite-live-bridge.rs` (`loopback_addr`) | `…::tests::bind_address_is_loopback_only` | 10.2 |
| `rules/` rulebook | `agents/pixel-critic.md`, `skills/pixel-*` | `evals/run.py::palette_hueshift`, linter on fixtures | 4.1–4.5, 5.6, 6.1 |
| `knowledge/palettes/*` | `agents/palette-smith.md`, `skills/pixel-palette` | `evals/run.py::palette_hueshift` | 8.1, 5.5 |
| Tool JSON-Schema contract | `src/live.rs` (`#[schemars]`, `LiveRunAppCommandParams.params`) | `src/live.rs::tests::all_tool_param_schemas_are_valid_objects`, `::run_app_command_params_field_is_object_not_boolean` | 2.3, 9.5 |
| Sprite linter | `tools/lint_sprite.py`, `tools/pixelpng.py` | `evals/run.py::linter_*`; `hooks/palette_lint_hook.py` | 7.3, 9.3 |
| Palette extractor (reference→style, Path 4) | `tools/extract_palette.py` (frequency/median-cut/kmeans) | `tests/test_extract_palette.py` (9 cases, CI) | 8.1, 9.3 |
| Visual regression | `tests/visual/diff.py`, `tests/visual/gen_fixtures.py`, golden PNGs | `evals/run.py::visual_*`; `tests/visual/` | 9.3 |
| [SPEC-003](../../specs/SPEC-003-tilemap-tool-family.md) Phase 3 (blob-47 bitmask) | `src/autotile.rs` (256→47 corner-masked states) | `src/autotile.rs::tests` (6 cases incl. exactly-47, CI `cargo test`) | 9.4 |
| SPEC-003 Phase 4 (seam-lint) | `tools/seam_lint.py` (pair + strip edge-match) | `tests/test_seam_lint.py` (7 cases, CI) | 7.3, 9.4 |
| SPEC-003 Phase 1 (tilemap CRUD) · [ADR-0005](../adr/0005-tilemap-protocol-and-bitmask.md) | `src/live.rs` (`create_tilemap_layer`/`list_tilesets`/`get_tileset`/`stamp_tiles`/`set_tile_data`) + `src/server.rs` (`live_*` tools) + `plugin.lua` handlers | `src/live.rs::tests` (8 new structs + `build_tilemap_export`); **live-verified 2026-06-14** on Aseprite 1.3.17.2 (create 8×8+16×16, stamp overwrite+fill confirmed by render); `scripts/smoke/tilemap-selftest.lua` | 2.1, 9.5 |
| SPEC-003 Phase 2 (dedupe) | `plugin.lua::handle_pack_similar_tiles` (`Image:isEqual` dedupe → tileset + reconstructing tilemap) + `src/live.rs::pack_similar_tiles` | **live-verified 2026-06-14**: 16 cells → 2 unique tiles, pixel-faithful round-trip (export grid + render) | 2.1 |
| SPEC-003 Phase 5 (engine export) | `src/tileset_export.rs` (Tiled `.tsj`/Godot `.tres`/JSON + `blob47_wangid`) + `src/live.rs::export_tileset` + `plugin.lua::handle_export_tilemap` | `src/tileset_export.rs::tests` (9 cases incl. blob47 wangset, CI) + **live-verified 2026-06-14** (files written, grid round-trips exactly); Tiled/Godot **import** is a user check | 2.1, 9.4 |
| [SPEC-004](../../specs/SPEC-004-constrained-semantic-drawing.md) Phase 1 (colour core) | `src/color_ops.rs` (CIELAB/CIEDE2000 snap + darken/lighten hue-shift + `build_color_map`) | `src/color_ops.rs::tests` (11 cases incl. Sharma CIEDE2000 + LAB≠RGBA proof, CI) | 7.3, 9.4 |
| SPEC-004 Phases 2–4 (live colour ops) · ADR-0005 | `src/live.rs` (`palette_snap`/`adjust_pixels`/`snap_colors`) + `src/server.rs` (`live_*` tools) + `plugin.lua` (`get_region_colors`/`apply_color_map`, RGB v1) | `src/live.rs::tests` (3 new param structs in schema contract); compile-green + Lua-parse-clean; **live E2E pending** | 2.1, 9.5 |
| Eval harness (Tier-A) | `evals/run.py`, `evals/cases.json` | self (CI: `.github/workflows/quality.yml`) | 9.4 |
| Eval harness (Tier-B live) | `evals/tier_b.json`, `evals/judge.py` | `evals/run.py::tier_b_cases_wellformed` (CI struct); live run logged in `evals/RESULTS.md` | 5.1–5.4, 6.2, 6.4, 9.4 |
| Skills | `skills/pixel-{new,shade,animate,export,palette,review}` | `evals/run.py` (palette/review) + Tier-B live (`RESULTS.md`) | 5.1–5.6 |
| Subagents | `agents/{pixel-critic,palette-smith,rig-builder,animation-director}.md` | eval cases (critic/palette) + Tier-B live (director/rig) | 6.1–6.4 |
| Hooks | `hooks/hooks.json` + `{guard_batch_draw,health_check,palette_lint_hook}.py` | `tests/test_hooks.py` (all 4 hooks, CI) + `evals/run.py` | 7.1, 7.3, 7.4 |
| ADR-0003 (destructive batch gate) | `hooks/guard_batch_draw.py` (`DESTRUCTIVE_PREFIXES`) | `tests/test_hooks.py::test_blocks_batch_destructive`; `evals/run.py::guard_decisions` | 10.4 |
| Auto-preview (perception upscale) | `hooks/hooks.json` (PostToolUse `mcp_tool` → `live_save_preview`); upscale in `src/preview.rs` + `src/live.rs::save_preview` | `tests/test_hooks.py::test_auto_preview_mcp_tool_wired` (manifest); `src/preview.rs::tests` (scale policy + nearest upscale, 6 cases); live behaviour needs Aseprite | 7.2 |
| Modal-free preview ([ADR-0004](../adr/0004-realtime-preview-render-frame.md)) | `plugin.lua::handle_save_preview` (renders active frame → `Image:saveAs`, no multi-frame modal); `src/live.rs::save_preview` sends `save_preview` | live-verified on a multi-frame indexed sprite; `cargo test --bins preview` (upscale pipeline unchanged) | 7.2, 3.4 |
| ASCII readback (perception, Path 1) | `live_ascii_view` tool (`src/server.rs`) → `src/live.rs::ascii_view` (reuses `save_preview` 1× render) → `src/ascii_view.rs` (pixels→text grid + legend) | `src/ascii_view.rs::tests` (4 cases); **live-verified 2026-06-14** on a 32×32 sprite (grid + glyph legend) | 2.1, 9.5 |
| Film-strip animation review (perception, Path 1) | `live_save_filmstrip` tool (`src/server.rs`) → `src/live.rs::save_filmstrip` (per-frame `save_preview` + restore active) → `src/filmstrip.rs::compose_grid` (frames→grid) | `src/filmstrip.rs::tests` (5 cases); **live-verified 2026-06-14** on a 6-frame sprite (3×2 grid, 1040×702) | 2.1, 9.5 |
| Frame-diff pixel readback (perception, Path 1) | `live_frame_diff` tool (`src/server.rs`) → `src/live.rs::frame_diff` (render both frames via `save_preview` 1× + restore active) → `src/ascii_view.rs::diff_to_ascii` (pixels→diff grid + legend) | `src/ascii_view.rs::tests` (4 diff cases); **live-verified 2026-06-15** on a 6-frame sprite (frame 1→3 = 131 changed cells, faithful grid + palette legend) | 2.1, 9.5 |
| Packaging | `.claude-plugin/plugin.json`, `marketplace.json`, `mcp/aseprite-live.json` | `claude plugin validate` + packaging CI job | 1.1, 1.2 |
| Cross-OS install | `scripts/verify_install.py`, CI `install-verify` matrix | green run on ubuntu/windows/macos (run 27254596963) | 1.4 |
| Living scorecard | `docs/aidd/COMPLETENESS_CHECKLIST.md` | re-scored per milestone (history table) | 12.2 |
| License consistency | `LICENSE` (MIT) + `plugin.json`/`Cargo.toml` metadata | `scripts/verify_install.py::check_license` (CI 3-OS) | 1.6 |
| Tier-B 5.4 backlog (`evals/RESULTS.md`) | `src/tools/export.rs::spritesheet_cli_args` (`--list-tags` default) | `src/tools/export.rs::tests` (3 cases) | 5.4 |
| Supply-chain hygiene | `.github/dependabot.yml` + `quality.yml` `audit` job | CI `cargo audit` gate; Dependabot PRs pending first merge to main | 10.5 |
| Release pipeline | `.github/workflows/release.yml`, `docs/release.md`, `CHANGELOG.md` | dry-run 27271161111 + tag run 27271480818 green; Release `v0.1.0` published w/ 3-OS archives | 1.7, 1.3 |
| Protocol version policy | `scripts/aseprite-mcp-plugin/plugin.lua` (strict reject → `unsupported_protocol`) | manual — needs an old-extension fixture; behavior code-reviewed | 3.5 |
| Docs language convention | English-only shipped docs; domain terms defined in `knowledge/glossary.md` ("lem nhem") | diacritics grep sweep (2026-06-10 audit); not CI-gated | 11.4 |
| — | `CHANGELOG.md` + git history | manual — first tag pending (see 1.7) | 1.3 |
| — | `scripts/{install,uninstall}-plugin.{ps1,sh}` | `bash -n` in CI (`install-verify`); full uninstall manual (mutates host) | 1.5 |
| Live protocol doc | `docs/live-protocol.md` (error codes, shapes) | `src/live.rs::tests::live_error_uses_protocol_error_shape` | 2.6 |
| — | `tracing`/`tracing-subscriber` to stderr, `RUST_LOG` (`src/main.rs`) | manual — stdio MCP keeps stdout clean by design; logs observed in live sessions | 2.7 |
| — | `plugin.lua` (menu UI, connection status) | manual — needs the Aseprite UI; exercised in Tier-B run | 3.2 |
| — | `plugin.lua` hello/handshake + `live_get_capabilities` | manual — every live session exercises it; logged in Tier-B run | 3.3 |
| — | `docs/QUICKSTART.md#focus--reconnect`, `docs/ARCHITECTURE.md` | rewritten 2026-06-11 from live empirical probes (background draw verified; reconnect-needs-focus isolated) | 3.4 |
| — | `knowledge/references/{goblin,pixel-art-sources}.md` | cited by skills/agents; used by the Tier-B run | 8.2 |
| — | `knowledge/glossary.md` | cross-referenced from `rules/` + skills; reviewed | 8.3 |
| — | `src/` unit tests (live, scripting, export, bridge) | `cargo test` in CI on 3 OS | 9.1 |
| SPEC-001 | `tests/bridge_loopback.rs`; `scripts/smoke/live-smoke.ps1` | loopback in CI; smoke manual (needs Aseprite) | 9.2 |
| — | `docs/{install,troubleshooting,live-protocol,live-tools,live-command-matrix}.md` | manual review each milestone | 11.1 |
| — | `docs/QUICKSTART.md` | followed end-to-end during the v14 Tier-B session | 11.2 |
| — | `docs/ARCHITECTURE.md` | v13 independent audit verified symbols/ports match code | 11.3 |
| `specs/TEMPLATE.md` | SPEC-001, SPEC-002 (2 features spec-first so far) | per-spec rows above; coverage gap tracked as 12.1=2 | 12.1 |
| — | `docs/adr/0001–0003` | reviewed; referenced from PRs and this matrix | 12.3 |
| — | `docs/aidd/REGEN.md` (module ↔ spec map) | manual — full regen not yet exercised (12.5=2) | 12.5 |
| — | `docs/aidd/TRACEABILITY.md` (this matrix) | independent audits (v13, v17) verified coverage; ≥2-rule gap-check is manual — CI script is future work | 12.4 |

## Rules for keeping this honest
- A checklist item ≥ 2 must have a row here pointing at real code **and** a test/eval
  (or an explicit "manual — why" note where automation isn't possible).
- New feature → add its spec, then a row here, before scoring its item > 1.
- If a test is deleted, drop or downgrade the linked checklist item in the same PR.
