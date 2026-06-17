# ADR 0003 — Security posture for `run_lua_script` and destructive ops

- Status: Accepted
- Date: 2026-06-10
- Checklist: 10.1, 10.4
- Related: [ADR-0001](0001-batch-vs-live-tools.md)

## Context
The MCP server exposes `run_lua_script` (and `execute_cli`), which run **arbitrary
Aseprite Lua** via a headless `aseprite --batch --script` process. Aseprite Lua
can touch the filesystem and shell out, so this tool is effectively
**arbitrary code execution** on the user's machine — the single largest security
surface in the project. Separately, several live tools are **destructive**
(`live_clear_cel`, `live_delete_layer`, `live_delete_frame`, `live_delete_tag`,
`live_delete_slice`).

## Decision
**`run_lua_script` / `execute_cli`:**
- **Opt-in gate (shipped):** both tools are **disabled by default** and return an
  actionable error unless `ASEPRITE_MCP_ALLOW_LUA` is set truthy
  (`1`/`true`/`yes`/`on`) in the server environment. Implemented in
  `src/tools/scripting.rs` (`lua_execution_allowed`), unit-tested, documented in
  [SECURITY.md](../../SECURITY.md). The tool descriptions also state the danger.
- Even when enabled: only run scripts the user supplied or explicitly reviewed;
  never synthesize-and-run unreviewed Lua to "work around" a limitation.
- It is **batch/headless** (ADR-0001): it edits files on disk, not the live
  window, so it is never a substitute for `live_*` drawing.
- Network exposure is localhost-only (see 10.2); the bridge binds `127.0.0.1`
  (`loopback_addr`, regression-tested).
- The batch-draw guard hook (7.1) intentionally does **not** auto-allow it — Lua
  that draws is still arbitrary code; the human stays in the loop.

**Destructive ops (10.4):**
- All live edits go through Aseprite's own **undo history** — they are
  **reversible with Ctrl+Z** in the open document, which satisfies the
  "guarded or reversible" bar (10.4).
- **Batch** destructive tools (`clear_*`, `remove_*`, `delete_*`) erase file
  content with **no undo**, so the batch-draw guard hook blocks them by default
  (same `ASEPRITE_MCP_ALLOW_BATCH` opt-out); the live equivalents stay allowed.
- Structure-mutating batch ops (`flatten_layers`, `merge_down_layer`, crops,
  `replace_color`, color-mode changes) are **deliberately unblocked**: they are
  legitimate offline-pipeline verbs, covered by the confirm-intent guidance
  below rather than the guard — the boundary is "deletes content" vs "transforms it".
- Agents must still confirm before clearing/deleting user content that they did
  not create, and prefer non-destructive alternatives (hide vs delete a layer).
- Canvas-level operations (resize/crop) and **save/overwrite** are the genuinely
  hard-to-reverse actions: confirm intent before overwriting an existing file;
  prefer `live_save_copy_as` to a new path over `live_save_sprite` when unsure.

## Consequences
- The dangerous surface is named, documented, **and gated off by default**;
  enabling it is a deliberate operator action (`ASEPRITE_MCP_ALLOW_LUA=1`).
- `run_lua_script` is arbitrary code, not a sandbox — the gate reduces blast radius
  (no execution at all unless opted in) but does not sandbox once enabled.
- Destructive drawing stays safe via Aseprite undo; the irreversible actions
  (overwrite/export to an existing path) get explicit-confirm guidance.
