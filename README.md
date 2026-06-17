# Aseprite MCP Server

A **live-first** Model Context Protocol (MCP) server, written in Rust, that lets an
AI agent drive a **running Aseprite** session — drawing, animating, managing
palettes and tilemaps directly in the open editor, with perception tools so the
agent can actually *see and verify* its own pixel work.

Unlike file-in/file-out tooling, the primary path edits the **live** sprite over a
WebSocket bridge, so every change appears immediately in the Aseprite window. A small
set of offline CLI tools (export, colour-mode, and a gated scripting escape hatch)
covers deliberate file-level operations.

## What's inside

- **Live editing** (`live_*`): sprites, layers, frames, cels, tags, slices,
  selections, palettes, drawing (`live_draw_pixels`, `live_use_tool`), and raw
  `live_run_app_command` — all against the open editor.
- **Perception** (research-backed): `live_save_preview` (nearest-neighbour upscale so
  small sprites are legible to a vision model), `live_ascii_view` (one-glyph-per-pixel
  text grid for exact readback), `live_save_filmstrip` (review an animation in one
  image).
- **Constrained / semantic colour**: `live_palette_snap`, `live_adjust_pixels`,
  `live_snap_colors` — real CIELAB/CIEDE2000 snapping and intent-based shading that
  bake in a hue-shift rule (shadows cooler, highlights warmer) and stay palette-legal.
- **Tilemaps**: `live_create_tilemap_layer`, `live_list_tilesets`, `live_get_tileset`,
  `live_stamp_tiles`, `live_set_tile_data`, `live_pack_similar_tiles`, and
  `live_export_tileset` (Tiled `.tsj` with a blob-47 wangset, Godot `.tres`, or JSON).
- **Offline tools**: `export_sprite`, `export_spritesheet`, `change_color_mode`, and
  the **gated** `run_lua_script` / `execute_cli` escape hatch (off by default).

See `docs/live-tools.md` and `docs/live-command-matrix.md` for the full surface.

## Architecture

```
┌──────────────┐   stdio (MCP)   ┌───────────────────┐   WebSocket    ┌───────────────────┐
│  AI agent    │◄───────────────►│   aseprite_mcp    │◄──────────────►│  Aseprite (open)  │
│ (Claude etc) │                 │  (Rust server)    │  live-edit     │  + Lua plugin     │
└──────────────┘                 └─────────┬─────────┘  protocol v1   └───────────────────┘
                                           │ CLI (offline, optional)
                                           ▼
                                    aseprite --batch
```

The server speaks MCP over stdio. For live work it connects to a standalone
`aseprite-live-bridge` process (auto-spawned) that owns `127.0.0.1:9876`, where the
in-editor Lua plugin connects; decoupling the bridge from the server's lifecycle keeps
the plugin connection alive across server restarts (SPEC-001 / ADR-0002). The few
offline tools shell out to `aseprite --batch`.

## Installation

### Prerequisites
- **Rust** (stable toolchain).
- **Aseprite** installed.

### Build
```bash
cargo build --release
```
Produces `target/release/aseprite_mcp[.exe]` and `target/release/aseprite-live-bridge[.exe]`.
Keep the bridge binary next to the server so it can be auto-spawned.

### Aseprite path
The server checks `ASEPRITE_PATH` first, then common install locations (Program Files
/ Steam on Windows, `/Applications` on macOS, `PATH` / Steam on Linux). Override with:
```bash
export ASEPRITE_PATH="/path/to/aseprite"
```

### Install the live plugin
1. Copy `scripts/aseprite-mcp-plugin/` into your Aseprite extensions directory:
   - Windows: `%APPDATA%\Aseprite\extensions\`
   - macOS: `~/Library/Application Support/Aseprite/extensions/`
   - Linux: `~/.config/aseprite/extensions/`
2. Restart Aseprite.
3. `Help → MCP Server → Connect to MCP Server`.

## MCP client configuration

Register the server under a **distinct** name (`aseprite-live`) so it can't be
shadowed by another server also named `aseprite`:

```json
{
  "mcpServers": {
    "aseprite-live": {
      "command": "path/to/aseprite_mcp",
      "env": { "ASEPRITE_PATH": "/path/to/aseprite" }
    }
  }
}
```
(VS Code Copilot uses `"servers"` with `"type": "stdio"`; Cursor uses the same
`mcpServers` shape.)

## Live-first workflow (read this first)

The value is editing the *running* window, not writing files. To keep a live workflow
from silently degrading into invisible disk edits:

1. **Preflight.** Before any drawing/editing, call **`live_preflight`** (or
   `live_status`) and only proceed with `live_*` tools when `ready: true`.
2. **Fail loud.** If `ready: false`, stop and tell the user the live session isn't
   connected — do **not** quietly fall back to file tools (they won't show in the open
   window, and the next `Ctrl+S` can overwrite them).
3. **Offline is opt-in.** The CLI tools are for explicit, deliberate file operations.

This is enforced at three layers: the MCP server instructions, `live_preflight`
(returns `ready` + a `directive` + `remediation`), and every live mutating tool
failing loud with `live_not_connected` when disconnected.

## Live protocol

JSON over WebSocket, protocol `aseprite-live-edit` v1:
```json
{
  "protocol": "aseprite-live-edit",
  "version": 1,
  "id": "live-1",
  "type": "draw_pixels",
  "target": { "layer": "AI Draft", "frame": "active" },
  "payload": { "pixels": [ { "x": 10, "y": 10, "color": "#ff0000ff" } ] }
}
```
Responses are `{ "ok": true, "result": … }` or `{ "ok": false, "error": … }`.

## Project structure

```
aseprite_mcp/
├── Cargo.toml
├── src/
│   ├── main.rs            # entry point / MCP stdio transport
│   ├── server.rs          # MCP server, tool routing, ServerHandler
│   ├── live.rs            # live WebSocket bridge client + live_* logic
│   ├── aseprite.rs        # offline Aseprite CLI runner
│   ├── preview.rs         # nearest-neighbour preview upscale (perception)
│   ├── ascii_view.rs      # pixels → text-grid readback (perception)
│   ├── filmstrip.rs       # frames → single review image (perception)
│   ├── color_ops.rs       # CIELAB/CIEDE2000 + semantic colour ops
│   ├── autotile.rs        # blob-47 bitmask
│   ├── tileset_export.rs  # Tiled / Godot / JSON tileset export
│   ├── utils.rs           # hex-colour + clamp helpers
│   └── tools/             # offline tools: export.rs, scripting.rs
└── scripts/
    └── aseprite-mcp-plugin/   # the in-editor Lua plugin (plugin.lua)
```

## Environment variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `ASEPRITE_PATH` | Aseprite executable | auto-detected |
| `ASEPRITE_OUTPUT_DIR` | base dir for generated files (relative paths) | working dir |
| `ASEPRITE_MCP_LIVE_PORT` | plugin WebSocket port | `9876` |
| `ASEPRITE_MCP_LIVE_CONTROL_PORT` | server↔bridge control port | plugin + 1 |
| `ASEPRITE_MCP_LIVE_TIMEOUT_MS` | live request timeout (min 1000) | `30000` |
| `ASEPRITE_MCP_ALLOW_LUA` | enable `run_lua_script` / `execute_cli` | off |
| `RUST_LOG` | log level | `info` |

## Troubleshooting

- **A different "aseprite" server answers / tools missing.** MCP names must be unique
  per client; register this one as `aseprite-live` and confirm `live_*` tools appear
  after restarting the client.
- **Live edits don't show in the open window.** Make sure you're on the live path
  (`live_*`), not an offline tool — file edits land on disk, not in the GUI's
  in-memory copy. The live sprite is left unsaved on purpose; `Ctrl+S` when ready.
- **Live edits only apply when Aseprite is focused.** A connected session draws fine
  while unfocused. Persistent `live_not_connected` usually means a stale pre-bridge
  binary — rebuild and point the MCP command at the new `aseprite_mcp` +
  `aseprite-live-bridge` pair. After a *true* disconnect, the plugin's reconnect timer
  may need one window focus to tick.

## Acknowledgements

This project began as a rework of the (unlicensed) `Dizzd/aseprite_mcp`; the current
codebase is an independent clean-room reimplementation.

## License

MIT — see [LICENSE](LICENSE).
