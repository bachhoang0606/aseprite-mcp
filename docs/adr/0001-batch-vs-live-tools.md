# ADR 0001 — Separate batch tools from live tools

- Status: Accepted (documents existing design)
- Date: 2026-06-09
- Checklist: 2.1, 2.2, 10.x

## Context
The MCP server exposes two families: `live_*` tools that draw into the **open
Aseprite window** via the WebSocket plugin, and batch/file tools (and
`run_lua_script`) that run a **headless Aseprite** and edit files on disk.
Silently substituting batch for live defeats the live-first workflow and
confuses users (changes don't appear on screen).

## Decision
- `live_*` tools require `live_preflight` ready=true; never fall back to batch.
- Batch tools are for explicit, deterministic offline file generation only.
- `live_preflight`/`live_status` are the mandatory first call before live edits.

## Consequences
- Predictable behaviour; no "silent disk edits".
- A preflight guard hook should enforce this (checklist 7.1).
- `run_lua_script` is batch/headless — see ADR-0003 (security) for its risks.
