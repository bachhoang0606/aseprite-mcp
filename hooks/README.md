# Hooks

Automation that enforces the live-first workflow and surfaces infra problems
early. Wired in [`hooks.json`](hooks.json). Checklist pillar **7. Hooks**.

| Hook | Event | Script / type | Checklist |
|------|-------|--------|-----------|
| Batch-draw guard | `PreToolUse` | [`guard_batch_draw.py`](guard_batch_draw.py) | 7.1, 10.4 |
| Auto-preview export | `PostToolUse` (draw tools) | `mcp_tool` → `live_save_preview` | 7.2 |
| Palette-lint on save | `PostToolUse` (save/export) | [`palette_lint_hook.py`](palette_lint_hook.py) | 7.3 |
| Session health check | `SessionStart` | [`health_check.py`](health_check.py) | 7.4 |

## What they do
### Batch-draw guard (7.1, 10.4)
Blocks **batch/headless** canvas-mutating tools (`draw*`, `fill*`, `use_tool`,
`create_canvas/sprite/cel`, `new_cel`, `add_layer/frame(s)`, gradient) on the
Aseprite MCP servers, because they edit files on disk and never appear in the
open window — the silent fallback ADR-0001 forbids. (`create_tag`/`create_slice`
stay allowed: metadata, not pixels.)
Also blocks **destructive** batch tools (`clear_*`, `remove_*`, `delete_*`), which
erase file content with no undo (ADR-0003). `live_*` tools (undoable in-app) and
read-only/`export_*` batch tools are allowed. Exits `2` with a reason that tells
Claude to `live_preflight` + use the `live_*` equivalent instead.

Opt out for a deliberate offline-generation task:
```
set ASEPRITE_MCP_ALLOW_BATCH=1     # Windows (cmd)
$env:ASEPRITE_MCP_ALLOW_BATCH=1    # Windows (PowerShell)
export ASEPRITE_MCP_ALLOW_BATCH=1  # mac/linux
```

### Session health check (7.4)
On session start, probes the plugin port (`9876`) and the standalone bridge
control port (`9877`) and injects a one-line status + remediation into context, so
churn/port problems are visible before you try to draw.

## Interpreter note (read if a hook doesn't fire)
The scripts are **stdlib-only Python 3** and `hooks.json` invokes them with
`python`. On systems where Python 3 is `python3`, change the two commands in
`hooks.json` accordingly. (On Windows, avoid the Microsoft Store `python3` alias —
use a real install, e.g. `C:\...\Python3xx\python.exe`.)

## Verified (automated)
All four hooks have end-to-end contract tests in
[`tests/test_hooks.py`](../tests/test_hooks.py), run in CI (the `quality` job):
- `guard_batch_draw.py`: batch `draw*`/`fill*`/`create_*`/`add_*` and destructive
  `clear_*`/`remove_*`/`delete_*` → exit 2 (blocked); `live_*` (incl. live
  destructive — undoable), `export_sprite`, read-only → exit 0;
  `ASEPRITE_MCP_ALLOW_BATCH` opt-out → exit 0; malformed stdin → exit 0
  (never crashes/blocks).
- `palette_lint_hook.py`: defective PNG → `additionalContext` warning; clean PNG →
  silent; non-PNG/missing path → exit 0, no output.
- `health_check.py`: emits valid `SessionStart` `additionalContext` JSON.
- `hooks.json`: lifecycle events present; guard wired on the aseprite matcher; the
  7.2 auto-preview `mcp_tool` hook targets `live_save_preview` on the draw matcher.

## Auto-preview export (7.2)
A `PostToolUse` hook of type **`mcp_tool`** calls `live_save_preview` after draw
tools (`live_draw_pixels`/`live_use_tool`/`new_cel`/`clear_cel`), writing
`${cwd}/.aseprite-preview.png` so a preview stays current on each change.
`live_save_preview` saves a faithful 1× copy then **nearest-neighbor upscales it**
(in the Rust server, so the live document is untouched) to land the sprite's long
edge near ~1024px — a raw 1× preview of a 16–64px sprite is below the resolution a
vision model can read, so the upscale is what lets the agent actually *see* its
own work. The transient 1× source is written to the system temp dir and removed.
- Requires the live session connected (true during drawing).
- One-time caveat: Aseprite's "PNG doesn't support layers" alert is modal; tick
  **"Don't show this alert again"** once so the export never blocks. Add
  `.aseprite-preview.png` to `.gitignore`.

## Palette-lint on save (7.3)
`palette_lint_hook.py` runs the deterministic linter (`tools/lint_sprite.py`) on a
saved/exported PNG and surfaces structural findings (orphan/stray pixels;
off-palette if `ASEPRITE_MCP_PALETTE` points at a palette JSON). Non-blocking.
Skips `live_save_preview` output: that PNG is nearest-neighbor upscaled, so every
source pixel is an N×N block and stray/orphan-pixel detection would misfire (real
1× saves/exports are still linted).
