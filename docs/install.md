# Install

## Prerequisites

- Aseprite installed.
- Rust toolchain.
- On Windows, Visual C++ Build Tools with the C++ toolchain component installed.

## Build MCP Server

```powershell
cargo build --release
```

The binary is created under `target/release/`.

## Install Aseprite Plugin

Windows:

```powershell
scripts/windows/install-extension.ps1
```

Manual install:

1. Copy `scripts/aseprite-mcp-plugin` to `%APPDATA%\Aseprite\extensions\aseprite-mcp-plugin`.
2. Restart Aseprite.
3. Use `Help > MCP Server > Connect to MCP Server` if auto-connect is not already active.

## MCP Client

Configure your MCP client to run the Rust binary over stdio. Set `ASEPRITE_PATH` if the server cannot locate Aseprite automatically.

