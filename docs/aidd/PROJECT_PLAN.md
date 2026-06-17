# Aseprite Pixel-Art Plugin — Project Plan (AIDD)

> Companion to `COMPLETENESS_CHECKLIST.md`. Describes the target product, the
> AIDD way of building it, and the phased roadmap to apply the ideas.

## 1. Vision
A first-class **Claude Code plugin** that makes Claude a *professional pixel-art
collaborator inside Aseprite* — it can design, rig, draw, shade, animate, review,
and export sprites **live** in the open Aseprite window, guided by encoded
pixel-art expertise, with reliable infrastructure and measurable quality gates.

## 2. Target architecture (components)
```
.claude-plugin/plugin.json        # manifest tying it all together
rules/                            # encoded pixel-art domain rules (Claude follows)
skills/ (commands/)               # /pixel-new /pixel-shade /pixel-animate /pixel-export /pixel-palette /pixel-review
agents/                           # pixel-critic, animation-director, palette-smith, rig-builder
hooks/hooks.json                  # preflight guard, auto-preview export, palette-lint, health-check
mcp/ (Rust)                       # live + batch tools; WS bridge (decoupled), schemas
plugin-lua/                       # Aseprite extension (self-healing bridge)
knowledge/                        # palettes, references, glossary
specs/                            # spec-first source of truth (AIDD)
docs/adr/                         # architecture decision records
tests/                            # unit + smoke + visual-regression + skill evals
docs/aidd/COMPLETENESS_CHECKLIST.md  # living scorecard
```

### "LSP" — honest note
Aseprite has no Language Server. We reinterpret the LSP intent two ways:
1. **Sprite linter** — a diagnostics pass (palette off-ramp, stray pixels,
   broken silhouette, layer/naming violations) surfaced like LSP diagnostics.
2. **Lua dev LSP config** (lua-language-server) shipped for contributors editing
   the Aseprite extension. Both are tracked under Hooks/Testing, not a real LSP.

## 3. AIDD method (how we build)
1. **Spec-first**: every feature gets `specs/NNN-<name>.md` (intent, I/O,
   acceptance, checklist-item links) *before* code.
2. **Traceability**: spec ID → module → test → checklist item, referenced in PRs.
3. **Eval gates**: skills/agents have graded prompt suites; CI fails on regressions.
4. **Living scorecard**: re-score `COMPLETENESS_CHECKLIST.md` each milestone; commit the delta.
5. **ADRs**: record irreversible/structural decisions (bridge, batch-vs-live, palette format).
6. **Regen-friendly**: modules map 1:1 to specs so any piece can be regenerated from its spec.

## 4. Roadmap (phases) — ordered by leverage
### Phase 0 — Foundation & infra reliability (unblocks everything) ✅ DONE 2026-06-09
- [x] ADR-001 batch-vs-live; ADR-002 WS bridge decoupling (Accepted, implemented).
- [x] **Decouple WS bridge** into a standalone singleton process (fixes churn/port — 2.4)
      — `src/bin/aseprite-live-bridge.rs` + `LiveBridge` client refactor + loopback test (SPEC-001).
- [x] Fix tool JSON-Schema validity + add contract tests (2.3, 9.5) — `src/live.rs::tests`.
- [x] `.claude-plugin/plugin.json` manifest + clean install/uninstall scripts (1.x) — Phase 5.
- Note: live end-to-end switch-over needs a rebuild + MCP re-register (user action).

### Phase 1 — Encode expertise (rules + knowledge) ✅ DONE 2026-06-09
- [x] `rules/` pixel-art rulebook (palette, hue-shift, sel-out, AA, proportions, 3/4 view, animation timing, rig conventions) (4.x) — 7 files in `rules/`.
- [x] `knowledge/` palettes (NES/GB/PICO-8/goblin-default, machine-readable + cited) + glossary + reference conventions (8.x) — `knowledge/`.
- Result: checklist 4.x and 8.x → 3; total ≈27 → ≈47/100.

### Phase 2 — Skills (the user-facing verbs) ✅ DONE 2026-06-09
- [x] `/pixel-new`, `/pixel-shade`, `/pixel-animate`, `/pixel-export`, `/pixel-palette`, `/pixel-review` — `skills/<name>/SKILL.md` (5.x → 2).
- [x] Each skill: live-first steps + DoD + eval prompts; grounded in web-researched, cited sources (`knowledge/references/pixel-art-sources.md`).
- Reach 3 when the Phase 4 eval harness (9.4) grades the eval prompts in CI.

### Phase 3 — Subagents + hooks (automation & quality) ✅ DONE 2026-06-10
- [x] Agents: pixel-critic, animation-director, palette-smith, rig-builder — `agents/*.md` (6.x → 2).
- [x] Hooks: batch-draw guard (7.1) + session health-check (7.4) — `hooks/` + `hooks.json`, both verified.
- [x] Auto-preview export (7.2, via `mcp_tool` PostToolUse) + palette-lint on save (7.3) —
      wired in `hooks.json`, covered by `tests/test_hooks.py` (live 7.2 demo still pending).

### Phase 4 — Quality gates ✅ MOSTLY DONE 2026-06-09
- [x] Visual-regression golden pixel-diff (stdlib) — `tools/pixelpng.py`, `tests/visual/` (9.3 → 2).
- [x] Tier-A eval harness in CI — `evals/run.py` (8 checks, all green) (9.4 → 2).
- [x] Tool-schema contract test (all 42 param types) — in `src/live.rs` tests (2.3 → 2, 9.5 → 3).
- [x] CI: `.github/workflows/quality.yml` (cargo + python gates).
- [x] Tier-B LLM-judged live eval run (Aseprite 1.3.17.2, all 6 cases PASS —
      `evals/RESULTS.md`) pushed skills 5.1–5.4 + agents 6.2/6.4 → 3 (v14).
- Result: total ≈67 → ≈76/100 (v9); DoD met at v10 (≈82) after closing
  Hooks (7), Security (10), AIDD traceability (12).

### Phase 5 — Polish & release ✅ DONE 2026-06-10 (v0.1.0 shipped)
- [x] `.claude-plugin/plugin.json` manifest + `marketplace.json` + `mcp/aseprite-live.json` (1.1/1.2).
- [x] `scripts/install-plugin.ps1` + `uninstall-plugin.ps1` (1.5).
- [x] Quickstart + architecture diagram — `docs/QUICKSTART.md`, `docs/ARCHITECTURE.md` (11.2/11.3).
- [x] Cross-platform install verification on 3 OS (1.4) — CI `install-verify` matrix, green run (v15).
- [x] Security pass on `run_lua_script` (10.1) — `ASEPRITE_MCP_ALLOW_LUA` gate, SPEC-002/ADR-0003 (v16);
      destructive-op guards (10.4) — live ops undoable; batch `clear_/remove_/delete_`
      blocked by the guard hook (v18).
- [x] Release pipeline — `.github/workflows/release.yml` (3-OS artifacts + GitHub
      Release on `v*` tag) + `docs/release.md` (v18).
- [x] First tagged release — `v0.1.0` published 2026-06-10 with 3-OS archives
      (Release run 27271480818) + changelog cut (1.3/1.7 → 3).

## 5. Definition of "complete plugin"
Checklist total ≥ **80/100**, with **no pillar below 60%** of its weight, every
shipped skill/agent covered by an eval, and a green CI including visual-regression.

## 6. Immediate next actions (apply plan)
1. Approve this plan + checklist (you are here).
2. Phase 0, item 1: write ADR-002 and implement the **standalone WS bridge**
   (resolves the live-connection churn that has been blocking iteration).
3. Phase 1: land the `rules/` rulebook so all future drawing follows it
   (directly fixes the "lem nhem" quality problem at the source).
4. Re-score the checklist after each phase; keep the delta in git history.
