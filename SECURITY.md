# Security

The aseprite-live MCP server edits pixel art on your machine. This document states
its security posture and the one knob you may need. Checklist pillar **10**.

## Threat model in one line
The server runs locally and talks to a local Aseprite. The only ways it can affect
anything beyond your sprites are (a) running arbitrary Aseprite Lua/CLI and (b) the
local WebSocket ports — both are addressed below.

## Arbitrary code execution — `run_lua_script` / `execute_cli` (10.1)
`run_lua_script` and `execute_cli` run an unrestricted Aseprite `--batch --script`
process. Aseprite Lua can read/write files and shell out, so these tools are
**effectively code execution on your machine** — the largest attack surface here.

- **Disabled by default.** Both tools return an error unless you opt in by setting
  **`ASEPRITE_MCP_ALLOW_LUA=1`** in the MCP server's environment. (Accepted truthy
  values: `1`, `true`, `yes`, `on`.)
- Enable it only if you understand that an agent could then run Lua you didn't
  review. Never enable it to let an agent "work around" a limitation by
  synthesizing and running unreviewed scripts.
- Drawing never needs this gate: the `live_*` tools are the production path and are
  always available. See [ADR-0003](docs/adr/0003-run-lua-script-security.md).

Enable (example, in `mcp/aseprite-live.json` `env`):
```json
"env": { "ASEPRITE_MCP_ALLOW_LUA": "1" }
```

## Network exposure (10.2)
- The standalone bridge binds **`127.0.0.1` only** — the Aseprite plugin port
  (default 9876) and the MCP control port (default 9877). It is **not reachable
  off-host**; there is no setting that binds a routable/all-interfaces address.
- Enforced by `loopback_addr()` in `src/bin/aseprite-live-bridge.rs` and guarded by
  the `bind_address_is_loopback_only` unit test.

## Telemetry & secrets (10.3)
- **No telemetry.** The server sends nothing to any remote service; it speaks MCP
  over stdio to your client and WebSocket to your local Aseprite. There is no
  analytics, no phone-home, nothing to opt out of.
- **No secrets required or stored.** The only configuration is local paths/ports
  via env vars (`ASEPRITE_PATH`, `ASEPRITE_MCP_LIVE_PORT`,
  `ASEPRITE_MCP_LIVE_CONTROL_PORT`, and the opt-in `ASEPRITE_MCP_ALLOW_LUA`). No
  tokens or credentials are read or written.
- Logs go to **stderr** only (never stdout, which carries the MCP protocol); set
  `RUST_LOG` to control verbosity. Logs contain tool names and errors, not secrets.

## Destructive operations (10.4)
- **Live edits are reversible.** Every `live_*` edit (`live_clear_cel`,
  `live_delete_layer`, draw, offset, …) goes through Aseprite's own **undo history**
  — `Ctrl+Z` in the open document reverts it. This satisfies the "guarded or
  reversible" bar.
- **Agent guidance:** confirm before clearing/deleting user content you did not
  create; prefer non-destructive alternatives (hide vs delete a layer).
- **Genuinely hard-to-reverse** actions are overwrite/export to an *existing* path
  and canvas resize/crop. Confirm intent before overwriting; prefer
  `live_save_copy_as` to a new path over `live_save_sprite` when unsure.

## Reporting
This is a local dev tool with no network service. If you find a way for it to reach
off-host or execute code without the `ASEPRITE_MCP_ALLOW_LUA` opt-in, that's a bug —
open an issue describing the reproduction.
