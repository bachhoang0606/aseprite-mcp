# Testing

## Rust Tests

```powershell
cargo test
```

If Windows reports `link.exe` missing, install the Visual C++ Build Tools C++ toolchain component and run from a Developer Command Prompt.

## Live Integration Smoke

The live smoke test is intended for Windows with Aseprite installed.

1. Make sure no MCP server or demo controller is already using port `9876`.
2. Run:

```powershell
scripts/smoke/live-smoke.ps1
```

3. Open Aseprite with the plugin installed, or use `Help > MCP Server > Connect to MCP Server`.

The smoke script acts as a temporary WebSocket server and validates the Lua plugin protocol directly. Use a temporary sprite or disposable active sprite; the script creates and cleans temporary layers/tags/slices/frames.

## Manual Restart Scenarios

- Start MCP first, then Aseprite: plugin should connect.
- Start Aseprite first, then MCP: plugin should reconnect.
- Restart MCP while Aseprite remains open: plugin should reconnect within a few seconds.
- Restart Aseprite while MCP remains open: plugin should reconnect after startup.
