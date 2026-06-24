# ADR 0008 — Tool-surface: no profile gating (measured, rejected)

- Status: Accepted (decision: do NOT add tool-surface gating)
- Date: 2026-06-24
- Checklist: 12.x (tool-surface design), supersedes the research's open "tool-surface pruning" item
- Evidence: `evals/tool_select/` (harness + run `runs/2026-06-24/`)

## Context
The server exposes **77 flat MCP tools**. The research doc (§B,
`docs/research/agent-pixel-art-techniques.md`) raised an *untested hypothesis*: a large flat tool
surface hurts a model's tool-selection, suggesting we **prune** it by grouping tools into a `core`
set plus workflow **profiles**, exposed behind a gate (the model opens a profile to reveal its
tools — "hard gating"). Before building that, two risks were raised in review:

1. **Recall** — hiding a tool behind a profile may make it *harder* to find (you can't pick what you
   can't see), especially when a tool's group is non-obvious.
2. **Token economics** — a two-step "open the group, then call the tool" discovery loop may cost
   *more* tokens than the flat surface, particularly for clients that already defer tool schemas.

The project rule is **measure before building** (cf. the persona-line A/B in ADR-adjacent eval work,
which was rejected on data). So we built a measurement harness rather than assume.

## What we did to prove it
- **Harness** (`evals/tool_select/`): a `surface.json` model (core 18 + 7 profiles, all 77 tools
  mapped, per-tool token weights); a `cases.json` gold set of 15 realistic tasks each labelled with
  the correct tool + its profile (11 in non-core profiles, to stress recall); selector agents that
  pick a tool under a **flat** vs **hard-gated** surface; and a deterministic `score.py`
  (stdlib, `--selftest`, CI-gated) computing selection/routing accuracy plus an **analytic token
  model per client type** (`flat_eager` / `flat_deferred` / `gated_dynamic`) that prices the
  ToolSearch and gate-open round-trips.
- **Client research**: how real MCP clients actually load tools (Claude Code, GitHub Copilot in
  VS Code, OpenAI Codex CLI).

## Evidence (run 2026-06-24, `runs/2026-06-24/`)
- **Flat selection accuracy = 1.000 (15/15).** From the full 77-tool flat list the model picked the
  exact correct tool every time — distinguishing near-synonyms (`palette_snap` vs `snap_colors`,
  `new_tag` vs `set_tag_properties`, `pack_similar_tiles` vs `create_tilemap_layer`). There is **no
  selection problem** on this surface, hence no accuracy upside for pruning to recover.
- **Hard gating measurably hurts recall: routing accuracy 0.933 (−6.7pp).** One miss —
  `live_extract_style_profile` lives in the `color` group but the model reasoned "style/tileset" and
  opened `tilemap`, so it would never reach the tool. Exactly the predicted non-obvious-group failure.
- **The discovery loop costs deferred clients MORE, not less.** For Claude Code (which already defers
  schemas via ToolSearch), gating added **+6,032 tokens/task** and a *higher* standing cost (7,336 vs
  1,386 — `core` eagerly carries the 2.5k `save_preview` + 1k `import_reference` schemas). Gating's
  token win is **only** for purely eager clients (standing 23,429 → 7,336).
- **Real clients already defer/group, so server-side gating is redundant.** Copilot (VS Code) caps at
  128 tools and uses embedding-grouped "virtual tools" (`activate_*` stubs) above a threshold; Codex
  CLI auto-defers tool descriptions past ~10% of the context window via an `MCPSearch` tool. Both are
  the same lazy/grouped pattern as Claude Code's ToolSearch. A server-side hard gate duplicates
  (often worse) what the client already does.

## Decision
**Do NOT add tool-surface profile gating (static or dynamic) to this server.** On the data it is
*strictly worse* for deferred clients (lower accuracy AND higher token cost), and redundant for the
real clients (Claude Code / Copilot / Codex), which all defer or group tools themselves.

Sanctioned surface optimizations, in priority order:
1. **Trim oversized tool descriptions** (e.g. `live_save_preview` ~2.5k, `live_import_reference` ~1k).
   Pure docstring shortening — no tools removed, no behaviour change, no hiding → zero recall risk;
   lowers the token floor for every client and keeps Codex under its defer threshold longer. **Do.**
2. **Consolidation** (merge genuinely redundant tools to reduce the *count*) — helps Copilot's hard
   128-tool cap and the token floor without hiding anything. **Defer** until a concrete trigger
   (e.g. actually hitting the 128 cap alongside other servers); the data shows no current need.
3. **Hard gating / dynamic profiles** — **rejected** (this ADR).

Any future surface proposal must be re-measured with the harness before adoption.

## Consequences
- The 77-tool flat surface stays. No `enable_toolset` meta-tool, no profile config.
- `evals/tool_select/` remains as the reusable measurement harness (CI-gated `score.py --selftest`).
- A follow-up may trim the few oversized schemas (#1 above); consolidation stays parked.
