# SPEC-001 — Decoupled WebSocket bridge (standalone singleton)

- Status: Approved
- Owner: project
- Checklist items advanced: 2.4 (WS bridge decoupled), 2.5 (timeouts/queue), 3.1 (reconnect)
- Related ADRs: [ADR-0002](../docs/adr/0002-decouple-ws-bridge.md)

## Intent
Today the WebSocket bridge (port 9876, where the Aseprite Lua plugin connects)
runs **inside** the stdio MCP process. Claude Code owns that process's lifecycle
and may restart/duplicate it, which drops the plugin connection and causes
port contention (`os error 10048`) — the recurring dropped-bridge churn that
blocks iteration. This feature moves the bridge into a **standalone, long-lived
singleton process** so the plugin stays connected across MCP restarts.

## Inputs / Outputs
- **Inputs:** env `ASEPRITE_MCP_LIVE_PORT` (plugin port, default 9876),
  `ASEPRITE_MCP_LIVE_CONTROL_PORT` (MCP↔bridge port, default plugin+1 = 9877).
- **Outputs:** unchanged `live_*` tool behaviour and unchanged plugin protocol;
  the plugin requires **no changes**.

## Behaviour
### Components
1. **Bridge process** (`aseprite-live-bridge` binary) — owns both ports.
   - **Plugin port (9876):** speaks the *exact* current plugin protocol. It is a
     dumb relay: it does not interpret command semantics. At most one plugin
     connection (last wins).
   - **Control port (9877):** accepts MCP client connections (WebSocket JSON).
   - Relays client→plugin and plugin→client. Routes responses by **id
     namespacing**: rewrite a client's request `id` to `c{clientId}@@{origId}`
     before sending to the plugin; on the plugin's response, split the prefix and
     deliver to the originating client with `origId` restored.
   - Plugin `hello` / id-less frames → update bridge state, **broadcast** a
     `{"type":"bridge_state","pluginConnected":bool,"lastHello":...}` to all
     clients (also sent once on client connect).
   - Singleton via port ownership: if either bind fails, exit (another bridge
     already owns the ports).
   - Persists when all MCP clients disconnect → **keeps the plugin connected**
     across MCP restarts (the core win).

2. **MCP `LiveBridge`** (in-process) — becomes a **control client**.
   - On startup: connect to the control port; if refused, **spawn the bridge
     binary** (sibling of the current exe) and retry with backoff (spawn-if-absent).
   - Maintains the control connection with self-healing reconnect.
   - `command()` is unchanged in shape (id + pending map + oneshot + timeout); it
     now writes to the control socket and reads responses from `bridge_state`-aware
     read loop.
   - `connected`/`ready` = control socket present **AND** `pluginConnected` true.

### Idempotency / safety
- Two MCP processes racing to spawn: the loser bridge fails to bind and exits;
  both clients connect to the surviving bridge. No contention.
- No `live_*` ever falls back to batch (ADR-0001 preserved). Disconnected →
  loud `live_not_connected`.

## Acceptance criteria
- [ ] Bridge binary builds as a separate target; runs standalone.
- [ ] Plugin protocol byte-compatible (no plugin edit needed).
- [ ] Restarting/duplicating the MCP process does **not** drop the plugin (bridge
      survives; client reconnects).
- [ ] Two concurrent MCP clients can both issue commands; responses route to the
      correct caller (id namespacing).
- [ ] `live_preflight` reports `ready=true` only when plugin is actually present.
- [ ] No two processes bind 9876 (singleton); loser exits cleanly.
- [ ] Existing live unit tests still pass; new loopback relay test passes.

## Eval (how we grade it)
Loopback integration test (no Aseprite needed): start bridge on ephemeral ports,
connect a fake "plugin" WS and two "client" WS; assert: state broadcast on
connect; a client request reaches the plugin with a namespaced id; the plugin
response routes back to the right client with the original id; the other client
is unaffected; plugin disconnect broadcasts `pluginConnected=false`.

## Traceability
- Module(s): `src/bin/aseprite-live-bridge.rs` (relay), `src/live.rs` (client refactor)
- Test(s): `tests/bridge_loopback.rs`, `src/live.rs` unit tests
