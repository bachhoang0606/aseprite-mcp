# Live Protocol v1

The production live path is Rust MCP server -> WebSocket bridge -> Aseprite Lua plugin.

Node demo controllers are dev/demo only and are not part of the production live protocol.

## Request

```json
{
  "protocol": "aseprite-live-edit",
  "version": 1,
  "id": "live-1",
  "type": "draw_pixels",
  "target": {
    "layer": "AI Draft",
    "frame": 1
  },
  "payload": {
    "pixels": [
      { "x": 1, "y": 1, "color": "#ff0000ff" }
    ]
  }
}
```

`target` and `payload` are optional and command-specific.

## Success Response

```json
{
  "protocol": "aseprite-live-edit",
  "version": 1,
  "id": "live-1",
  "ok": true,
  "result": {}
}
```

## Error Response

```json
{
  "protocol": "aseprite-live-edit",
  "version": 1,
  "id": "live-1",
  "ok": false,
  "error": {
    "code": "layer_not_found",
    "message": "Layer was not found",
    "details": {
      "layer": "AI Head"
    }
  }
}
```

## Required Error Codes

- `live_not_connected`: Rust MCP server has no active Aseprite plugin connection.
- `live_timeout`: A live request timed out.
- `live_connection_lost`: the WebSocket connection closed while a request was active.
- `invalid_payload`: payload shape or value is invalid.
- `missing_field`: a required field is empty or missing.
- `layer_not_found`: requested layer does not exist.
- `frame_not_found` or `invalid_frame`: requested frame is missing or invalid.
- `cel_not_found`: requested cel does not exist.
- `unsupported_command`: plugin does not support the requested command.

## Capabilities

`get_capabilities` returns:

```json
{
  "protocol": "aseprite-live-edit",
  "protocolVersion": 1,
  "pluginVersion": "0.1.0",
  "appVersion": "1.3.17.2",
  "apiVersion": 40,
  "commands": ["get_sprite_info", "draw_pixels"]
}
```

Rust exposes this as the MCP tool `live_get_capabilities`.

