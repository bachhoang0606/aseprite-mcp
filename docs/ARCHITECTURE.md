# Architecture

How the pieces fit after the bridge decoupling (ADR-0002 / SPEC-001). Checklist 11.3.
Symbols below name the actual files/functions so the diagram tracks the code.

## Components & data flow
```
                 Claude Code (plugin host)
   ┌───────────────────────────────────────────────────────────┐
   │ skills /pixel-*   agents (pixel-critic, rig-builder, …)    │
   │ hooks (guard, health, preview, palette-lint)               │
   │ rules/  +  knowledge/   (read by skills & agents)          │
   └───────────────────────────────┬───────────────────────────┘
                                    │ stdio (MCP JSON-RPC: tools/list, tools/call)
                                    ▼
   ┌───────────────────────────────────────────┐   one per Claude Code window;
   │  aseprite_mcp (MCP server)                 │   lifecycle owned by the host,
   │  src/main.rs → src/server.rs (tool dispatch)│  may restart / duplicate.
   │  src/live.rs  LiveBridge  =  CONTROL CLIENT │
   └───────────────────────────────┬───────────┘
                                    │ WebSocket control link  ws://127.0.0.1:9877
                                    │ • spawn-if-absent: if connect refused, launch the
                                    │   sibling bridge binary, then retry with backoff
                                    │ • self-healing reconnect loop (run_client_loop)
                                    │ • reads bridge_state → plugin_connected flag
                                    ▼
   ┌───────────────────────────────────────────────────────────┐
   │  aseprite-live-bridge   (src/bin/aseprite-live-bridge.rs)  │
   │  SINGLETON via port ownership · owns 9876 + 9877           │
   │  dumb relay, persists across MCP restarts                  │
   │   handle_client()  ── namespaces req id ──▶ plugin         │
   │   route_from_plugin() ── splits id ──▶ originating client  │
   │   forward_from_client(): no plugin ⇒ live_not_connected    │
   └───────────────────────────────┬───────────────────────────┘
                                    │ WebSocket plugin link  ws://127.0.0.1:9876
                                    ▼
   ┌───────────────────────────────────────────────────────────┐
   │  Aseprite 1.3 + Lua extension                             │
   │  scripts/aseprite-mcp-plugin/plugin.lua  (client-only WS) │
   │  executes commands, draws into the OPEN window            │
   └───────────────────────────────────────────────────────────┘
```

The bridge accepts **N control clients** but **at most one plugin** (last
connection wins). That is what lets a second MCP process attach without dropping
the first — and what makes id-namespacing necessary.

## Request routing (id-namespacing)
A single `live_*` call, e.g. `live_draw_pixels`, traced end to end:

```
 MCP client                    bridge                         plugin
 ──────────                    ──────                         ──────
 {id:"live-42", …}  ───────▶  forward_from_client():
                              rewrite id → "c3@@live-42"  ───▶  {id:"c3@@live-42"}
                                                                 (draws pixels)
 {id:"live-42", …}  ◀──────  route_from_plugin():        ◀───  {id:"c3@@live-42",
                              split "c3@@live-42" →                ok:true, …}
                              client 3, orig "live-42"
```

- `make_namespaced_id(client_id, orig)` = `c{client_id}@@{orig}`;
  `split_namespaced_id()` reverses it. Separator `@@` (`ID_SEP`).
- Plugin `hello` / id-less frames update bridge state and **broadcast**
  `{"type":"bridge_state","pluginConnected":…,"lastHello":…}` to every client
  (also pushed once on client connect). `LiveBridge` uses this for `ready`.
- `connected` / `ready` (`live_preflight`) = control socket present **AND**
  `pluginConnected` true. No plugin ⇒ the bridge answers the client directly with
  `live_not_connected` (`doNotFallBackToBatch:true`) so a call never silently
  degrades to disk writes.

## Why the bridge is a separate process
Claude Code owns the MCP server's lifecycle and may restart/duplicate it. When the
bridge lived *inside* the MCP process, every restart dropped the plugin connection
and duplicates fought over port 9876 (`os error 10048`) — the dropped-bridge
churn. As a **standalone singleton** (loser of the bind race exits cleanly) the
bridge keeps the plugin connected across MCP restarts. See
[ADR-0002](adr/0002-decouple-ws-bridge.md),
[SPEC-001](../specs/SPEC-001-decoupled-ws-bridge.md).

## Two tool families (ADR-0001)
- **`live_*`** → MCP → bridge → plugin → the **open Aseprite window**. The
  production path. Always `live_preflight` first.
- **Batch/file tools** → a headless Aseprite editing files on **disk**. Only for
  explicit offline generation/export the user asked for. The `PreToolUse` guard
  hook (`hooks/guard_batch_draw.py`) blocks batch *drawing* tools so a
  disconnected session never silently degrades.

## Ports
| Port | Owner | Purpose | Env override |
|------|-------|---------|--------------|
| 9876 | bridge | Aseprite Lua plugin connects here | `ASEPRITE_MCP_LIVE_PORT` |
| 9877 | bridge | MCP server(s) connect here as control clients | `ASEPRITE_MCP_LIVE_CONTROL_PORT` (default = plugin port + 1) |

## Known limits
- **Reconnect needs one focus:** background (unfocused) live editing works —
  WebSocket receive/execute/repaint were verified while Aseprite sat unfocused
  (2026-06-11; upstream aseprite#3009). What still depends on focus is the
  plugin's Timer-driven *reconnect* bookkeeping: after a real disconnect
  (Aseprite/bridge restart), reconnection may wait until the window is focused
  once. Minimized windows are untested and may defer work — see
  [QUICKSTART](QUICKSTART.md#focus--reconnect).
- **Binary path is OS-specific** in `mcp/aseprite-live.json` (`.exe` on Windows);
  see [`mcp/README.md`](../mcp/README.md) and [QUICKSTART](QUICKSTART.md).
