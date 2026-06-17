# Aseprite Pixel-Art Plugin — Completeness Checklist

> Self-evaluation rubric for turning `aseprite_mcp` into a complete, professional
> Claude Code **plugin** for pixel-art work in Aseprite.
> Scoring per item: **0** = missing · **1** = stub/partial · **2** = working · **3** = polished + tested + documented.
> This file is the living source of truth — re-score it each milestone (see `PROJECT_PLAN.md`).
>
> Revision: v19 (v0.1.0 released — tag → 3-OS binaries → GitHub Release, observed end-to-end) · Last scored: 2026-06-10

## How to score
- Each pillar has a **weight**. Pillar score = avg(item scores)/3 × weight.
- Project completeness = Σ pillar scores (max 100).
- "DoD" = Definition of Done: the bar an item must clear to earn a **3**.

---

## 1. Plugin packaging & distribution — weight 10
| # | Item | Now | DoD |
|---|------|-----|-----|
| 1.1 | `.claude-plugin/plugin.json` manifest (name, version, components) | 3 | Valid manifest, loads in Claude Code |
| 1.2 | Marketplace entry / install instructions | 2 | `claude plugin install` works end-to-end |
| 1.3 | Semantic versioning + CHANGELOG | 3 | Every release tagged + changelog entry |
| 1.4 | Cross-platform install (win/mac/linux paths) | 3 | Verified on ≥2 OS; no hardcoded user paths |
| 1.5 | Clean uninstall (removes MCP reg, extension, ports) | 2 | One command restores pre-install state |
| 1.6 | LICENSE file shipped + consistent with manifests | 3 | `LICENSE` matches plugin.json/Cargo.toml; CI-checked |
| 1.7 | Release artifacts (prebuilt binaries) via CI on tag | 3 | Tagged release publishes 3-OS binaries |

## 2. MCP server (Rust) — weight 14
| # | Item | Now | DoD |
|---|------|-----|-----|
| 2.1 | Live tools (`live_*`) over WS plugin | 2 | All documented, schema-valid |
| 2.2 | Batch tools (file/CLI) clearly separated | 2 | Cannot be used as silent live fallback |
| 2.3 | **Tool JSON-Schema validity** (no `params` boolean-schema bug) | 2 | `tools/list` passes strict clients |
| 2.4 | **WS bridge decoupled** from stdio lifecycle (no churn/port-contention) | 3 | Server restarts never drop the plugin |
| 2.5 | Request timeouts + async/queue tolerance | 3 | Tunable; no spurious `live_timeout` |
| 2.6 | Error taxonomy (codes, actionable messages) | 2 | Documented enum; every error has remedy |
| 2.7 | Observability (structured logs to stderr, levels) | 2 | `RUST_LOG` usable; no stdout pollution |
| 2.8 | Large-sprite payload limits / chunking | 1 | Pixel-data paths bounded + tested |

## 3. Aseprite plugin (Lua bridge) — weight 8
| # | Item | Now | DoD |
|---|------|-----|-----|
| 3.1 | Self-healing reconnect (no manual Aseprite restart) | 3 | Survives server restart automatically |
| 3.2 | Connection status UI (menu) | 2 | Clear connected/disconnected feedback |
| 3.3 | Capability/version handshake | 2 | Server/plugin negotiate protocol version |
| 3.4 | Focus/reconnect behaviour documented | 3 | Docs match verified behavior: background editing works; reconnect needs one focus |
| 3.5 | Protocol version-mismatch policy (upgrade story) | 2 | Mismatch rejected + documented + tested |

## 4. Domain rules (pixel-art expertise) — weight 12
| # | Item | Now | DoD |
|---|------|-----|-----|
| 4.1 | Rules doc: palette discipline, hue-shifting, ramps | 3 | Encoded as plugin rules Claude follows |
| 4.2 | Rules: selective outlining, anti-aliasing, banding | 3 | With do/don't examples |
| 4.3 | Rules: proportions, silhouette readability, 3/4 view | 3 | Reference angles & sizes |
| 4.4 | Rules: animation timing, easing, anticipation | 3 | Frame-count + duration guidance |
| 4.5 | Rules: layer/rig conventions (separate limbs, naming) | 3 | Standard rig documented |

## 5. Skills (slash commands) — weight 10
| # | Item | Now | DoD |
|---|------|-----|-----|
| 5.1 | `/pixel-new` scaffold sprite (size, palette, layers) | 3 | Creates rigged base live |
| 5.2 | `/pixel-shade` apply ramp/hue-shift to a layer | 3 | Idempotent, previewable |
| 5.3 | `/pixel-animate` walk/idle/attack from a rig | 3 | Generates tagged frames |
| 5.4 | `/pixel-export` PNG/GIF/spritesheet + JSON meta | 3 | Game-engine ready output |
| 5.5 | `/pixel-palette` set/optimize/load presets | 3 | Lospec presets supported |
| 5.6 | `/pixel-review` critique against the rules | 3 | Scored report w/ fixes |

## 6. Subagents — weight 8
| # | Item | Now | DoD |
|---|------|-----|-----|
| 6.1 | `pixel-critic` (visual QA vs rules) | 3 | Returns scored findings |
| 6.2 | `animation-director` (timing/poses) | 3 | Plans frames, no main-ctx bloat |
| 6.3 | `palette-smith` (ramps, color theory) | 3 | Proposes cohesive palettes |
| 6.4 | `rig-builder` (layer/limb decomposition) | 3 | Outputs layer plan |

## 7. Hooks — weight 8
| # | Item | Now | DoD |
|---|------|-----|-----|
| 7.1 | Pre-live-edit guard (force `live_preflight`) | 3 | Blocks batch fallback automatically |
| 7.2 | Post-edit auto-export preview PNG | 2 | Updates a preview on each change |
| 7.3 | Palette-lint on save (flag off-ramp colors) | 3 | Warns on palette violations |
| 7.4 | Session-start MCP health check | 3 | Surfaces churn/port issues early |

## 8. Knowledge base — weight 6
| # | Item | Now | DoD |
|---|------|-----|-----|
| 8.1 | Curated palettes (NES, GB, PICO-8, custom) | 3 | Machine-readable + cited |
| 8.2 | Reference sheet conventions (goblin/3-4 view etc.) | 3 | In-repo, linkable |
| 8.3 | Glossary (dithering, sel-out, AA, banding) | 3 | Used by rules + skills |

## 9. Testing & quality gates — weight 10
| # | Item | Now | DoD |
|---|------|-----|-----|
| 9.1 | Rust unit tests (protocol, errors) | 2 | Cover core paths |
| 9.2 | Smoke tests (live + batch round-trip) | 2 | CI-runnable, deterministic |
| 9.3 | **Visual-regression for sprites** (pixel-diff golden) | 2 | Catches art regressions |
| 9.4 | Eval harness for skills/agents (graded prompts) | 2 | Pass/fail gates in CI |
| 9.5 | Schema/contract tests for MCP tools | 3 | Validates every tool schema |

## 10. Security & safety — weight 8
| # | Item | Now | DoD |
|---|------|-----|-----|
| 10.1 | **`run_lua_script` arbitrary-code risk** controlled | 3 | Opt-in/sandboxed/documented danger |
| 10.2 | Localhost-only binding + port notes | 3 | No external exposure by default |
| 10.3 | No secrets/telemetry without consent | 3 | Documented; off by default |
| 10.4 | Destructive-op confirmations (clear/delete) | 3 | Guarded or reversible |
| 10.5 | Supply-chain hygiene (dep updates + audit) | 3 | Dependabot active + `cargo audit` gate in CI |

## 11. Docs & onboarding — weight 6
| # | Item | Now | DoD |
|---|------|-----|-----|
| 11.1 | Install / troubleshooting / protocol docs | 2 | Already strong; keep current |
| 11.2 | Quickstart ("draw your first sprite live") | 2 | 5-min path, copy-paste |
| 11.3 | Architecture diagram (MCP↔bridge↔plugin) | 3 | Matches code |
| 11.4 | Docs language consistency (English + glossary terms) | 2 | Shipped docs English-only; terms defined; CI-checked |

## 12. AIDD practice — weight 10
| # | Item | Now | DoD |
|---|------|-----|-----|
| 12.1 | Spec-first: each feature has a spec before code | 2 | `specs/` dir, traceable IDs |
| 12.2 | This checklist embedded + re-scored per milestone | 3 | Scored history kept |
| 12.3 | ADRs for key decisions (bridge, batch-vs-live) | 3 | `docs/adr/NNN-*.md` |
| 12.4 | Traceability: spec → code → test → checklist item | 3 | Linkable in PRs |
| 12.5 | Regenerate-from-spec friendly structure | 2 | Modules map 1:1 to specs |

---

## Current score snapshot (v19 — v0.1.0 released end-to-end)
> The release pipeline was exercised for real: dispatch dry-run green (3
> artifacts), changelog cut, tag `v0.1.0` pushed → Release run 27271480818 built
> 3-OS binaries and **published the GitHub Release** with
> `aseprite-mcp-{linux-x86_64,windows-x86_64,macos-arm64}.tar.gz` attached.
> 1.3 and 1.7 now meet their DoD on observed evidence. (v18 details: history.)

| Pillar | Weight | Avg item (0-3) | Weighted | % of weight | Δ from v18 |
|--------|-------:|---------------:|---------:|------------:|----------:|
| 1 Packaging | 10 | 2.71 | 9.0 | 90% | **+1.4** (1.3→3, 1.7→3) |
| 2 MCP server | 14 | 2.13 | 9.9 | 71% | — |
| 3 Lua plugin | 8 | 2.4 | 6.4 | 80% | — |
| 4 Domain rules | 12 | 3.0 | 12.0 | 100% | — |
| 5 Skills | 10 | 3.0 | 10.0 | 100% | — |
| 6 Subagents | 8 | 3.0 | 8.0 | 100% | — |
| 7 Hooks | 8 | 2.75 | 7.3 | 92% | — |
| 8 Knowledge base | 6 | 3.0 | 6.0 | 100% | — |
| 9 Testing | 10 | 2.2 | 7.3 | 73% | — |
| 10 Security | 8 | 3.0 | 8.0 | 100% | — |
| 11 Docs | 6 | 2.25 | 4.5 | 75% | — |
| 12 AIDD | 10 | 2.6 | 8.7 | 87% | — |
| **Total** | **100** | — | **≈97.2 / 100** | **all ≥70%** | **+1.4** |

> **DoD status: MET** — total 97.2 ≥ 80 and every pillar ≥70% (floor: pillar 2 at
> 70.8%, rounded 71%).
> **What v19 earned (evidence-backed):**
> - **1.7 → 3:** the Release workflow ran end-to-end on the real tag — run
>   27271480818 success (3 build jobs + publish), GitHub Release `v0.1.0` is
>   live (not draft) with all three OS archives attached (2.6–3.2 MB each).
>   The dispatch dry-run beforehand (run 27271161111) also succeeded with
>   publish correctly skipped.
> - **1.3 → 3:** `v0.1.0` annotated tag + `CHANGELOG.md` cut with a matching
>   `## v0.1.0` section; versions consistent across Cargo.toml/plugin.json.
> **Still honest <3:** **1.2** (`claude plugin install` end-to-end unverified),
> **1.5** (uninstall not exercised on a fresh host), **7.2** (live auto-preview
> not demonstrated), **9.4** (Tier-B not an outcome gate), **2.8** (no
> pixel-payload chunking — only palette `limit`), **3.5** (mismatch strict-reject
> exists in `plugin.lua` but untested/undocumented as policy), **11.4**
> (convention swept manually, not CI-gated), **12.1/12.5** (specs not universal).

### Score history
- **v3 (baseline)** 2026-06-09 — ≈27/100. AIDD checklist + plan + ADRs scaffolded.
- **v4** 2026-06-09 — ≈47/100. Phase 1 landed: `rules/` rulebook (4.1–4.5 → 3)
  + `knowledge/` palettes/glossary/references (8.1–8.3 → 3); AIDD ADRs + spec
  template recognized (12.1–12.3 bumped).
- **v5** 2026-06-09 — ≈50/100. Phase 0 bridge decoupling (SPEC-001 / ADR-0002):
  standalone `aseprite-live-bridge` singleton + control-client refactor →
  2.4→3, 2.5→2, 3.1→3; loopback contract test → 9.2→2.
- **v6** 2026-06-09 — ≈56/100. Phase 2 skills: web-researched + cited
  (`knowledge/references/pixel-art-sources.md`) and built `skills/` —
  `/pixel-new /pixel-palette /pixel-shade /pixel-animate /pixel-export
  /pixel-review` (5.1–5.6 → 2). Reach 3 once an eval harness (9.4) grades them.
- **v7** 2026-06-09 — ≈63/100. Phase 3: `agents/` — pixel-critic, palette-smith,
  rig-builder, animation-director (6.x → 2); `hooks/` — batch-draw guard +
  session health check, both verified (7.1, 7.4 → 2). Post-edit preview (7.2) +
  palette-lint (7.3) deferred (need MCP-from-hook).
- **v8** 2026-06-09 — ≈67/100. Phase 5 packaging: `.claude-plugin/plugin.json` +
  `marketplace.json` + `mcp/aseprite-live.json` (all JSON-validated), `scripts/
  install-plugin.ps1` + `uninstall-plugin.ps1`, `docs/QUICKSTART.md` +
  `docs/ARCHITECTURE.md` (1.1/1.2/1.5 → 2, 11.2/11.3 → 2).
- **v9** 2026-06-09 — ≈76/100. Phase 4 quality gates (stdlib-only + CI):
  Rust tool-schema contract test over all 42 param types (2.3→2, 9.5→3);
  `tools/lint_sprite.py` sprite linter + `tests/visual/` golden pixel-diff
  (9.3→2); `evals/run.py` 8-check tier-A harness, all green (9.4→2);
  `.github/workflows/quality.yml` runs cargo + python gates. Eval coverage
  promotes pixel-palette/pixel-review (5.5/5.6→3) and palette-smith/pixel-critic
  (6.3/6.1→3). Linter engine also lifts palette-lint hook (7.3→1).

- **v10** 2026-06-10 — ≈82/100, **DoD met**. Wave 1 closed the three sub-60%
  pillars: Hooks (7.2 auto-preview via `mcp_tool` hook + 7.3 palette-lint wired →
  7.x all 2); Security (ADR-0003 `run_lua_script`/destructive-op posture →
  10.1/10.4→2); AIDD (`TRACEABILITY.md` 12.4, `REGEN.md` 12.5, SPEC-001 →
  12.1/12.4/12.5→2). Official Claude Code docs review unlocked 7.2 (`mcp_tool`
  hook) and corrected the deferral.

- **v11** 2026-06-10 — ≈83/100. Waves 2–3 (docs-driven correctness + packaging gate):
  cross-platform `mcp/README.md` + parameterised command (1.4→2); namespacing +
  `${CLAUDE_PLUGIN_ROOT}` cache-safety convention in `skills/README.md` &
  QUICKSTART; CI `packaging` job validates all plugin JSON manifests + best-effort
  `claude plugin validate`; `--plugin-dir`/`/reload-plugins` documented. Verified
  with the real CLI (2.1.142): `claude plugin validate ./` passes **clean** and a
  local marketplace add accepts the plugin → **1.1→3** (enable deferred to avoid
  MCP port conflict).

- **v12** 2026-06-10 — *self-claimed ≈90.6/100; superseded by v13 audit.* Polish
  pass built real artifacts: cross-OS CI matrix (`rust` + `install-verify` on
  ubuntu/windows/macos) + `scripts/verify_install.py`; Tier-B LLM-judge eval suite
  (`evals/tier_b.json` rubrics for `/pixel-new /pixel-shade /pixel-animate
  /pixel-export`, `animation-director`, `rig-builder`) + `evals/judge.py`
  validator/prompt-emitter, CI-gated structurally via `tier_b_cases_wellformed` +
  `evals/RESULTS.md` log; `docs/ARCHITECTURE.md` refreshed to code symbols +
  id-namespacing sequence; focus/throttle docs. v12 *scored* 1.4/5.1–5.4/6.2/6.4/
  9.4 as 3 — too generous (see v13).
- **v13** 2026-06-10 — ≈85.8/100, **honest re-score**. An independent subagent
  audit (read-only, evidence-based) found the v12 bumps treated CI-validated test
  *definitions* as *passed tests*. No fabricated files; arithmetic was clean. Per
  the DoD ("3 = polished + **tested** + documented"), reverted to 2: **1.4** (CI
  config, no green 2-OS run), **5.1–5.4 / 6.2 / 6.4** (rubrics defined,
  `RESULTS.md` all pending — never judged live), **9.4** (Tier-B adds structural
  validation, not an outcome gate). Confirmed genuine: **3.4 1→3** and **11.3
  2→3**. Net session gain over v11 = **+2** (≈84→85.8), not +7.

- **v14** 2026-06-10 — ≈89.3/100. Ran the **Tier-B live evals** end-to-end in
  Aseprite 1.3.17.2 (one 32×32 goblin built/shaded/animated/exported via `live_*`;
  rig-builder + animation-director plans produced) and had an **independent judge
  subagent** score the objective evidence (tool outputs + rendered PNGs) against
  `evals/tier_b.json` — all six PASS (5.1 0.97, 5.2 0.97, 5.3 0.88, 5.4 0.88,
  6.2 1.00, 6.4 0.98; logged in `evals/RESULTS.md`). 5.1–5.4 / 6.2 / 6.4 → 3.
  Surfaced two real backlog gaps: export JSON lacks `frameTags`; walk legs slide
  vs spread/together.

- **v15** 2026-06-10 — ≈90.0/100. Pushed the polish branch; **CI ran green on all
  three OS** (run 27254596963: Rust tests + install-verify on ubuntu/windows/macos
  + Python gates). "Verified on ≥2 OS, no hardcoded paths" now observed → 1.4 → 3.
- **v16** 2026-06-10 — ≈96.0/100. Beyond-DoD hardening of the three 67% pillars:
  **Security** — shipped `ASEPRITE_MCP_ALLOW_LUA` opt-in gate (`src/tools/scripting.rs`,
  4 unit tests), loopback-bind regression test, `SECURITY.md`, SPEC-002, ADR-0003
  update → 10.1/10.2/10.3 → 3. **Hooks** — `tests/test_hooks.py` (11 e2e cases, in
  CI) → 7.1/7.3/7.4 → 3. **AIDD** — SPEC-002 + current `TRACEABILITY.md` →
  12.2/12.3/12.4 → 3. Kept 7.2/9.4/10.4/12.1/12.5 at 2 (honest — not fully
  demonstrated). All Rust unit tests + Tier-A + hook tests green.

- **v17** 2026-06-10 — ≈94.2/100, **audit + hygiene + scope growth**. Full
  audit verified v16's claims (arithmetic + evidence all clean) and then raised
  the bar: added items 1.6 (LICENSE — shipped + CI-checked → 3), 1.7 (release
  artifacts → 0), 10.5 (supply-chain — Dependabot config → 1). Closed the 5.4
  `frameTags` backlog gap in `export_spritesheet` (+3 unit tests). Hygiene:
  PROJECT_PLAN/CHANGELOG synced, duplicate `rust.yml` removed, actions bumped to
  checkout@v5/setup-python@v6, dead `LiveTargetParams` removed, `example/` →
  `examples/sprites/`, Cargo.toml repo URL fixed. Apples-to-apples: v16 under
  the v17 item set ≈92.7, so net earned ≈+1.5.

- **v18** 2026-06-10 — ≈95.2/100, **audited backlog closed**. Every v17-audit P2
  fixed with tests: destructive batch gate (10.4→3), env-tunable live timeout
  (2.5→3), `cargo audit` CI gate (10.5→2), release pipeline + rewritten
  release docs (1.7→1, run pending merge), 17 missing traceability rows
  (12.4 restored). P3s: mac/linux install scripts, CI concurrency +
  dedup, Tier-B evidence-archive convention, language sweep + glossary term.
  New honest items: 2.8 payload limits (1), 3.5 version-mismatch policy (2),
  11.4 docs language (2). 10.5 bumped 2→3 same-day once the audit job's first
  CI run went green (run 27269544824). Net earned ≈+2.7 apples-to-apples.
  v18 details that moved out of the snapshot: 10.4→3 (destructive batch gate,
  tested), 2.5→3 (tunable timeout), 12.4 dip-and-restore (17 missing rows),
  hygiene (CI dedup/concurrency, mac/linux scripts, language sweep).
- **v19** 2026-06-10 — ≈97.2/100, **v0.1.0 released**. Dispatch dry-run green →
  changelog cut → tag `v0.1.0` → Release run 27271480818 published the GitHub
  Release with 3-OS archives. 1.3→3, 1.7→3 on observed evidence.

**SPEC-003 tilemap (Phases 1–5 done, live-verified 2026-06-14):** Phases 3 (blob-47
bitmask) and 4 (seam-lint) landed earlier with CI tests. Phases 1 (tilemap CRUD),
2 (dedupe), and 5 (engine export) shipped 7 `live_*` tools, pure-Rust engine
serializers (`src/tileset_export.rs`, 9 tests), and new `plugin.lua` handlers.
Deterministic parts are `cargo test --bins` green (38 total); the live tile
CRUD/dedupe/export was then **verified end-to-end on Aseprite 1.3.17.2** (plugin
0.2.3) — create 8×8+16×16, `pack_similar_tiles` 16→2 pixel-faithful, `stamp_tiles`
overwrite+fill confirmed by render, export Tiled/Godot/JSON+blob47 grid round-trips
exactly; three live-surfaced bugs fixed (see CHANGELOG). The new live tool surface
strengthens 2.1 (a formal re-score is deferred to the next milestone pass). Only
Tiled/Godot **import** of the emitted files remains a user check. A `pixel-tileset`
skill (5.x) is still future work. See SPEC-003 Acceptance + ADR-0005.

**Remaining polish (beyond DoD, optional):** split `LegL`/`LegR` for the walk +
re-run Tier-B with evidence archived under `evals/runs/` (5.3, 9.4); demonstrate
7.2 auto-preview live; make Tier-B a real CI/outcome gate (9.4); verify
`claude plugin install` from the marketplace end-to-end (1.2) and a clean-host
uninstall (1.5); pixel-payload limits/chunking (2.8); version-mismatch policy
doc + test (3.5); language convention in CI (11.4); specs for remaining
features (12.1/12.5). (Minor: bump `upload-artifact` when its Node24 major
lands — Dependabot will PR it.)

**Accepted limitation (3.1/3.4, deliberately not fixed):** after a *real*
disconnect (Aseprite/bridge restart, sleep/resume) the plugin's reconnect runs
on UI Timers that may pause while unfocused → one click into Aseprite heals it.
Candidate fix exists (stop Timer-based socket teardown; lean on ixwebsocket's
own background auto-reconnect + bridge-side WS-protocol ping liveness) but it
rewires the most regression-prone state machine in the project (3 historical
bugs) for a now-rare trigger — revisit only if it bites in practice.
