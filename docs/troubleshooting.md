# Troubleshooting

## `live_not_connected`

The Rust MCP server has no active plugin connection.

- Make sure Aseprite is open.
- Make sure the plugin is installed.
- Check `Help > MCP Server > Connect to MCP Server`.
- Confirm the server is listening on `127.0.0.1:9876`.

## Plugin Does Not Load

- Verify the extension folder exists under `%APPDATA%\Aseprite\extensions\aseprite-mcp-plugin`.
- Verify `package.json` and `plugin.lua` are inside that folder.
- Restart Aseprite after plugin updates.

## Restart MCP Does Not Reconnect

- Wait a few seconds for the plugin reconnect timer.
- Use `Help > MCP Server > MCP Connection Status`.
- Confirm no other process occupies port `9876`.

## Opaque Command Failure

Live errors should have `code`, `message`, and optional `details`. If a raw Lua stack trace appears, treat it as a plugin bug and add a regression smoke case.

## `link.exe` Missing

Visual Studio Build Tools can be installed without the C++ linker. Install the C++ toolchain component, then run cargo from a Developer Command Prompt.

