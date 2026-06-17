# MCP server config

[`aseprite-live.json`](aseprite-live.json) declares the plugin's MCP server. It is
referenced from `.claude-plugin/plugin.json` via `"mcpServers": "./mcp/aseprite-live.json"`
(file-path form — avoids the inline-`mcpServers` parsing bug).

```json
{ "mcpServers": { "aseprite-live": {
    "command": "${CLAUDE_PLUGIN_ROOT}/target/release/aseprite_mcp.exe",
    "env": { "ASEPRITE_PATH": "...", "ASEPRITE_MCP_LIVE_PORT": "9876",
             "ASEPRITE_MCP_LIVE_CONTROL_PORT": "9877" } } } }
```

## Paths are parameterised, not hardcoded
- The binary is found via **`${CLAUDE_PLUGIN_ROOT}`** (the plugin's install dir) —
  no per-user absolute path is baked in. Build it first (`cargo build --release`).
- `ASEPRITE_PATH` is your Aseprite executable (used by batch/export tools); set it
  in your environment, e.g. `setx ASEPRITE_PATH "C:\Program Files\Aseprite\Aseprite.exe"`.
- Optional: `ASEPRITE_MCP_LIVE_TIMEOUT_MS` tunes the live request timeout
  (default `30000`, floor `1000`) for slow/long-running app commands.

## Cross-platform (the one OS-specific bit)
The `command` ends in **`.exe`** for Windows. On **mac/linux**, change it to drop
the extension:
```
"command": "${CLAUDE_PLUGIN_ROOT}/target/release/aseprite_mcp"
```
Everything else (ports, env) is identical. The bridge binary
(`aseprite-live-bridge`) is auto-spawned by the server from the same directory, so
it needs no separate entry.

> Ports: `9876` = Aseprite plugin ↔ bridge; `9877` = MCP server ↔ bridge (control).
> Both bind `127.0.0.1` only (localhost; see ADR-0003 / checklist 10.2).
