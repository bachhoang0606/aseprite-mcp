#!/usr/bin/env bash
# Install the Aseprite Pixel-Art plugin locally (macOS / Linux).
# Mirror of install-plugin.ps1 (checklist 1.2 / 1.4 / 1.5). Idempotent.
#   1. Build the Rust MCP server + standalone bridge in release mode.
#   2. Install/refresh the Aseprite Lua extension.
#   3. Verify the binaries the plugin manifest points at exist.
#   4. Print the remaining (manual) steps.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Aseprite user-extensions dir per OS (override with ASEPRITE_EXTENSIONS_DIR).
if [[ "${OSTYPE:-}" == darwin* ]]; then
  DEFAULT_EXT_DIR="$HOME/Library/Application Support/Aseprite/extensions"
else
  DEFAULT_EXT_DIR="$HOME/.config/aseprite/extensions"
fi
EXT_DIR="${ASEPRITE_EXTENSIONS_DIR:-$DEFAULT_EXT_DIR}"

echo "== Aseprite Pixel-Art plugin install =="
echo "Repo: $REPO_ROOT"

echo
echo "[1/4] Building release binaries (cargo build --release)..."
(cd "$REPO_ROOT" && cargo build --release)

echo
echo "[2/4] Installing the Aseprite extension..."
TARGET="$EXT_DIR/aseprite-mcp-plugin"
mkdir -p "$TARGET"
cp "$REPO_ROOT/scripts/aseprite-mcp-plugin/package.json" "$TARGET/"
cp "$REPO_ROOT/scripts/aseprite-mcp-plugin/plugin.lua" "$TARGET/"
echo "  Extension -> $TARGET"

echo
echo "[3/4] Verifying binaries..."
for bin in aseprite_mcp aseprite-live-bridge; do
  path="$REPO_ROOT/target/release/$bin"
  if [[ -f "$path" ]]; then echo "  OK  $path"; else
    echo "  MISSING expected binary: $path" >&2; exit 1
  fi
done

echo
echo "[4/4] Done. Remaining steps:"
echo "  a) export ASEPRITE_PATH=/path/to/aseprite   (used by batch tools)"
echo "  b) Load the plugin in Claude Code (workspace .claude-plugin/plugin.json"
echo "     is picked up — run /reload-plugins — or install from a marketplace)."
echo "  c) Open Aseprite (extension auto-connects to the bridge on port 9876)."
echo "  d) In Claude Code: run live_preflight until ready=true, then use live_* / /pixel-* skills."
echo
echo "Note: the standalone bridge (aseprite-live-bridge) auto-spawns from the MCP server; it owns ports 9876/9877."
