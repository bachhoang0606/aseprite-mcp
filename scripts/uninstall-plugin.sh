#!/usr/bin/env bash
# Cleanly uninstall the Aseprite Pixel-Art plugin's local footprint (macOS / Linux).
# Mirror of uninstall-plugin.ps1 (checklist 1.5). Restores pre-install state:
#   1. Stop the MCP server + standalone bridge processes.
#   2. Verify the live ports (9876 plugin, 9877 control) are free.
#   3. Remove the Aseprite Lua extension.
# Does NOT delete your sprites or the repo. Afterwards also remove the plugin
# from Claude Code (/plugin, or `claude plugin uninstall aseprite-pixel-art`).
set -uo pipefail

PLUGIN_PORT="${1:-9876}"
CONTROL_PORT="${2:-9877}"

if [[ "${OSTYPE:-}" == darwin* ]]; then
  DEFAULT_EXT_DIR="$HOME/Library/Application Support/Aseprite/extensions"
else
  DEFAULT_EXT_DIR="$HOME/.config/aseprite/extensions"
fi
EXT_DIR="${ASEPRITE_EXTENSIONS_DIR:-$DEFAULT_EXT_DIR}"

echo "== Aseprite Pixel-Art plugin uninstall =="

echo
echo "[1/3] Stopping MCP server + bridge processes..."
# pkill -x matches the kernel comm name, truncated to 15/16 chars on
# linux/macOS — too short for "aseprite-live-bridge" — so fall back to a
# full-cmdline match anchored on the executable path.
for name in aseprite_mcp aseprite-live-bridge; do
  if pkill -x "$name" 2>/dev/null || pkill -f "(^|/)${name}( |\$)" 2>/dev/null; then
    echo "  Stopped $name"
  else
    echo "  $name not running"
  fi
done

echo
echo "[2/3] Freeing ports $PLUGIN_PORT / $CONTROL_PORT..."
for port in "$PLUGIN_PORT" "$CONTROL_PORT"; do
  if ! command -v lsof >/dev/null 2>&1; then
    echo "  lsof not found — cannot verify port $port (try: ss -ltn | grep \":$port \")"
  elif pids=$(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null) && [[ -n "$pids" ]]; then
    kill $pids 2>/dev/null || true
    echo "  Freed port $port (pid(s): $pids)"
  else
    echo "  Port $port free"
  fi
done

echo
echo "[3/3] Removing the Aseprite extension..."
TARGET="$EXT_DIR/aseprite-mcp-plugin"
if [[ -d "$TARGET" ]]; then
  rm -rf "$TARGET"
  echo "  Removed $TARGET"
else
  echo "  Not installed ($TARGET)"
fi

echo
echo "Done. Pre-install state restored (binaries remain in target/release; 'cargo clean' removes them)."
