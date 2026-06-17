# ADR 0002 — Decouple the WebSocket bridge from the stdio MCP process

- Status: Accepted — Implemented 2026-06-09 (see [SPEC-001](../../specs/SPEC-001-decoupled-ws-bridge.md))
- Date: 2026-06-09
- Checklist: 2.4, 2.5, 3.1

## Context
The WS bridge (port 9876, where the Aseprite Lua plugin connects) currently runs
**inside** the stdio MCP server process. Claude Code controls that process's
lifecycle and may restart/duplicate it, which kills/contends the bridge:
- multiple `aseprite_mcp.exe` instances spawn; only one binds 9876 (`os error 10048`),
- the plugin connects to one instance while tool calls route to another,
- result: `live_preflight` reports disconnected despite a live TCP connection.
This churn repeatedly blocked live iteration during development.

## Decision (proposed)
Run the WS bridge as a **standalone, long-lived singleton process** (owns 9876).
- The Aseprite plugin connects to the bridge (unchanged).
- Each MCP stdio server connects to the bridge as a **client** (control port).
- The bridge relays commands↔responses (per-process id namespacing) and reports
  plugin presence to clients.

## Consequences
- MCP server restarts/duplicates no longer drop the plugin connection.
- Removes port-contention and the "wrong instance" failure mode.
- Does **not** fix focus-throttle (Aseprite only services the plugin when its
  window runs its loop) — that is an inherent GUI limitation, documented separately.
- Adds a relay component + lifecycle management (spawn-if-absent, promotion on owner death).

## Implementation notes (2026-06-09)
- Bridge binary: `src/bin/aseprite-live-bridge.rs` — self-contained dumb relay,
  singleton via port ownership (loser exits on bind failure). Persists across MCP
  restarts, keeping the plugin connected.
- MCP client: `src/live.rs` `LiveBridge` now connects to the control port and
  **spawns the bridge if absent**, with self-healing reconnect (capped backoff).
  On Windows it spawns detached (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`,
  attempting `CREATE_BREAKAWAY_FROM_JOB`) so the bridge outlives the MCP process.
- Id routing: client request ids are namespaced `c{clientId}@@{origId}` so the
  plugin's echoed id routes the response back to the right MCP client.
- **Deferred (future work):** true *promotion on owner death* (a client promoting
  itself to bridge). Current resilience comes from spawn-if-absent on reconnect,
  which covers the bridge dying mid-session.
- Verified by `tests/bridge_loopback.rs` (fake plugin + 2 clients) + existing
  `src/live.rs` unit tests. Live end-to-end requires rebuild + MCP re-register.
