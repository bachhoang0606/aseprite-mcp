---
name: pixel-doctor
description: Diagnose and fix the live Aseprite bridge when live_* tools won't connect — live_preflight not ready, "no sprite open", or edits don't appear in the Aseprite window. Use whenever live_preflight returns ready:false or drawing seems to go nowhere.
---

# pixel-doctor — fix the live bridge

## Diagnose
1. `live_preflight` → read `ready`, `bridgeLinked`, `lastHello`.
2. Shell:
   - `netstat -ano | findstr "9876 9877"` — is anything **LISTENING** on 9876 (bridge)?
   - `tasklist | findstr aseprite` — how many `aseprite_mcp.exe`? is `aseprite-live-bridge.exe` running? is Aseprite up?

## Common causes → fix
- **`bridgeLinked:false` + `lastHello:null` + no 9876 listener** → the bridge isn't running:
  - **Smart App Control blocking it** (most likely on Win11): the bridge is an unsigned exe. Test:
    try to run `...\target\release\aseprite-live-bridge.exe` — if it errors *"An Application Control
    policy has blocked this file"*, SAC is enforced. **Fix:** Windows Security → App & browser control →
    Smart App Control → **Off** (⚠️ irreversible — needs Windows reset to re-enable). Reboot, reconnect.
  - **Orphan / duplicate servers** → kill all `aseprite_mcp.exe` (`taskkill /F /IM aseprite_mcp.exe`; in
    Git-Bash use `//F //IM` or PowerShell `Stop-Process -Name aseprite_mcp -Force`), then reconnect so ONE
    clean server spawns the bridge.
  - **Version mismatch** (server newer than bridge) → restore the matched approved pair (`.approved-bak`).
- **Extension off / needs focus** → Aseprite open with `aseprite-mcp-plugin` enabled; **click the Aseprite
  window** (the reconnect timer only fires when focused) and open a document.
- **"no sprite open"** → the doc was closed (unsaved live work is lost) — re-open and `live_save_sprite_as` early.

## Recover
After fixes: **`/mcp`** reconnect (Claude Code) or **restart Codex**; re-run `live_preflight` until
`ready:true` and `lastHello` is populated + 9876 has a listener. Then resume drawing.
