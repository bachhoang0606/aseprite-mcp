# Quickstart — draw your first sprite live (5 minutes)

Get from a clean checkout to drawing **live** in the open Aseprite window.

## Prerequisites
- Aseprite 1.3+ installed.
- Rust toolchain (`cargo`) to build the MCP server + bridge.
- Python 3 (real install, not the Windows Store stub) for the hooks.

## 1. Install (Windows)
```powershell
scripts/install-plugin.ps1
```
This builds the release binaries (`aseprite_mcp.exe` + `aseprite-live-bridge.exe`),
installs the Aseprite Lua extension, and verifies everything.

Then set your Aseprite path (used by batch/export tools):
```powershell
setx ASEPRITE_PATH "C:\Program Files\Aseprite\Aseprite.exe"
```

> **mac/linux:** build with `cargo build --release`, copy
> `scripts/aseprite-mcp-plugin/` into your Aseprite extensions dir
> (`~/Library/Application Support/Aseprite/extensions/` or
> `~/.config/aseprite/extensions/`), and in `mcp/aseprite-live.json` change the
> command from `.../aseprite_mcp.exe` to `.../aseprite_mcp` (no `.exe`).

## 2. Load the plugin in Claude Code
Pick one:
- **Local/dev:** load this repo directly — `claude --plugin-dir ./` (run
  `/reload-plugins` after edits). Fastest iteration; no install.
- **Workspace:** open the repo as your workspace; Claude Code discovers
  `.claude-plugin/plugin.json` automatically.
- **Marketplace (published):** `claude plugin install aseprite-pixel-art@<marketplace>`.

Validate the manifest/components before sharing (the community review runs the
same check):
```bash
claude plugin validate ./
```

It contributes: the `aseprite-live` MCP server, the `/pixel-*` skills, the
pixel-critic / palette-smith / rig-builder / animation-director agents, and the
live-first guard + auto-preview + palette-lint + session health-check hooks.

> **Skill names are namespaced** once installed: `/aseprite-pixel-art:pixel-new`,
> `…:pixel-review`, etc. (shown as `/pixel-*` below for brevity).

## 3. Open Aseprite
Launch Aseprite (with a sprite open). The extension auto-connects to the standalone
bridge on port `9876`. The window can stay **unfocused** while Claude draws —
see [Focus & reconnect](#focus--reconnect) for the one case that still needs a
click.

## 4. Verify + draw
In Claude Code:
1. Run **`live_preflight`** → wait for `ready: true`.
2. Scaffold a sprite: **`/pixel-new`** (e.g. "new 32×32 goblin, goblin-default palette").
3. Shade it: **`/pixel-shade`**. Animate: **`/pixel-animate`**. Review: **`/pixel-review`**.

If `live_preflight` is not ready, the plugin will tell you why and how to fix it —
**never** fall back to batch/file tools (they edit disk, not the open window).

## Focus & reconnect
The Aseprite extension lives on Aseprite's **single UI thread**. Empirical
verification (2026-06-11, Windows 11, Aseprite 1.3.17.2) corrected an earlier
belief: a **connected** session keeps working while the window is
visible-but-unfocused — incoming WebSocket commands are received, executed,
**and repainted** in the background, with normal latency (7–100 ms). The
upstream basis is [aseprite#3009](https://github.com/aseprite/aseprite/pull/3009),
which made socket messages process without focus. Idle background sessions stay
connected (verified through a 5-minute unfocused idle).

What still depends on focus is the plugin's **Timer-driven bookkeeping**
(reconnect attempts, ping-miss counting) — Aseprite may not tick UI timers in
the background. Consequences:

- **After a real disconnect** (Aseprite restarted, bridge killed/updated,
  OS sleep/resume), the reconnect timer may not run until the Aseprite window
  is focused **once**. Click into Aseprite and the plugin self-heals (no
  Aseprite restart needed).
- **Ping tolerance** (`ping_max_misses = 8`, ~16 s) keeps short throttled
  stretches from tearing the connection down.
- **Minimized** (not just unfocused) windows are untested and may defer work.

**If commands fail with `live_not_connected` on every fresh session** and a
click into Aseprite "fixes" it, your registered MCP binary is a stale
pre-standalone-bridge build — rebuild (`cargo build --release`) and point the
registration at the new `aseprite_mcp` (the `aseprite-live-bridge` singleton
must sit next to it).

Tuning lives in `scripts/aseprite-mcp-plugin/plugin.lua` (`config` block:
`reconnect_tick`, `ping_max_misses`, `connect_max_ticks`) and
`ASEPRITE_MCP_LIVE_TIMEOUT_MS` on the server.

## Uninstall
```powershell
scripts/uninstall-plugin.ps1
```
Stops the server+bridge, frees ports 9876/9877, removes the Aseprite extension.
Then remove the plugin from Claude Code (`/plugin`).

## Troubleshooting
- **`ready:false`** → ensure Aseprite is open with the extension enabled (focus it once if it just restarted);
  the session health-check hook prints port status at startup.
- **Ports busy** → run `scripts/uninstall-plugin.ps1` to free 9876/9877, then retry.
- See [README](../README.md#troubleshooting) and
  [docs/ARCHITECTURE.md](ARCHITECTURE.md).
