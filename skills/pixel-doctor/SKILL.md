---
name: pixel-doctor
description: Diagnose why the live Aseprite session won't connect (or edits don't appear) and give the exact fix — bridge down, SAC-blocked exe, wrong ~/.claude.json server path, orphan/stale server. Use when live_* tools fail with live_not_connected / the /mcp reconnect shows EUNKNOWN, or after a rebuild/reconnect.
argument-hint: "[optional: paste live_preflight output as a file]"
---

# /pixel-doctor — diagnose the live-infra dance

Run this BEFORE retrying blindly — and never "work around" a disconnected live session with the
offline file tools (file edits won't appear in the open editor; the disconnected hint forbids it).

## 0. Static checks first (deterministic, no live call)
Run `python scripts/pixel_doctor.py` (add `--json` for machine output). It inspects
`~/.claude.json` + the server/bridge exes + the Aseprite path + Windows SAC + ports 9876/9877 and
prints `OK/WARN/FAIL` with a fix per line; exit non-zero on any `FAIL`. Fix every `FAIL`, then
re-run. It can't see the plugin handshake — that's step 1.

## 1. The live probe — `live_preflight`, branch on `bridgeLinked`
Call `live_preflight`. `connected = bridgeLinked && plugin_connected`, so the fields decide it:
- **`ready:true`** → connected. Done; proceed with `live_*` tools.
- **`bridgeLinked:false`** → the BRIDGE layer is down (the server can't reach its bridge) → §A.
- **`bridgeLinked:true` + `lastHello:null`** → the PLUGIN layer (bridge up, no Aseprite plugin
  attached) → §B.

(You can also pipe it in: save the JSON and `python scripts/pixel_doctor.py --preflight pf.json`.)

## A. `bridgeLinked:false` — bridge/server won't start
In order of likelihood:
1. **Missing sibling bridge.** The server spawns `aseprite-live-bridge[.exe]` from its OWN
   directory (`src/live.rs` `spawn_bridge`); if it isn't beside the server exe, `bridgeLinked`
   stays false forever (a silent `warn!`). `pixel_doctor` → `sibling-bridge: FAIL`. **Fix:** copy a
   SAC-approved `aseprite-live-bridge.exe` next to the server exe that `~/.claude.json` names.
2. **SAC-blocked fresh build (Windows).** A just-built exe is blocked by Smart App Control →
   `/mcp` reconnect = `EUNKNOWN`; `cargo test` first run = `OS error 4551`. `pixel_doctor` →
   `smart-app-control: WARN (ENFORCE)`. **Fix:** don't keep retrying — restore a previously-approved
   exe over the fresh one (server **and** bridge together), OR launch the new exe once
   interactively so SAC evaluates it; for `cargo test`, just re-run (prefer `cargo test --bins`).
   NEVER rebuild the release server while live-connected.
3. **Orphan / wrong-path server** holding 9876/9877 → §C / §D.

## B. `bridgeLinked:true`, `lastHello:null` — plugin not connected
The bridge is up but no Aseprite plugin dialed in. **Fix:** (1) launch Aseprite; (2) install +
enable the `aseprite-mcp-plugin` extension (Edit ▸ Preferences ▸ Extensions) so it connects out to
`ws://127.0.0.1:9876`; (3) **focus the Aseprite window once** — its reconnect timer ticks on focus,
which fixes post-sleep/resume stalls; (4) re-run `live_preflight` until `ready:true`. Do **not** set
`ASEPRITE_PATH` for this — the LIVE path never reads it (that's offline-only, see §E).

## C. Orphan / stale registered binary
After rebuilding while connected, a `/mcp` reconnect can leave an orphan old server holding the
port while your edits hit old code. `serverVersion` is **useless** here (hardwired `0.1.0`) — use
the exe path + mtime.
- **Detect:** `pixel_doctor` → `stale-binary: WARN` (registered exe older than this repo's build);
  PowerShell `Get-CimInstance Win32_Process -Filter "Name='aseprite_mcp.exe'" | Select Id,ExecutablePath,CreationDate` (more than one row = orphan).
- **Fix:** toggle the MCP server off in `/mcp` so it stops respawning, `Stop-Process` the orphan(s)
  and any stray `aseprite-live-bridge.exe` (frees 9876/9877), then reconnect once.

## D. Wrong `~/.claude.json` path (underscore vs hyphen repo, or a shadow server)
Both `...\aseprite_mcp\` (underscore) and `...\aseprite-mcp\` (hyphen) can exist with full
binaries, so exe-existence alone won't catch a wrong pointer. `pixel_doctor` → `wrong-repo: WARN`
when the command points into the underscore repo while you edit the hyphen checkout, and
`shadow-server: WARN` if a second stdio server literally named `aseprite` can shadow `aseprite-live`.
**Fix:** repoint the `aseprite-live` `command` at the checkout you're editing — **any co-located
build dir is fine** (this machine's healthy config uses `target\rmcp1\`, not `target\release`),
keeping the sibling bridge beside it.

## E. Offline tools: "Aseprite executable not found"
That error is from the OFFLINE escape-hatch tools (`export_sprite` / `change_color_mode` /
`run_lua_script`), not the live path. **Fix:** set `ASEPRITE_PATH` to Aseprite's full path
(resolution order: `ASEPRITE_PATH` if on disk → OS install dirs → `aseprite` on PATH). `pixel_doctor`
reports this as `aseprite-exe (offline tools)`.

## Definition of done
A specific failure mode named with the exact one-line fix, **verified** by re-running
`live_preflight` to `ready:true` (or `pixel_doctor` to no `FAIL`). Never substitute offline file
tools to "work around" a disconnected live session.

## Eval prompts
- "`live_draw_pixels` says `live_not_connected`" → run `pixel_doctor` + `live_preflight`; branch on
  `bridgeLinked` (false → §A sibling-bridge/SAC/orphan; true+`lastHello:null` → §B launch+enable plugin).
- "`/mcp` reconnect threw `EUNKNOWN` after I rebuilt" → SAC-blocked fresh exe (§A.2): restore/approve
  the exe; never rebuild while connected.
- "my fix isn't taking effect after reconnect" → orphan / stale binary (§C): kill the orphan, repoint, reconnect.
- "offline `export_sprite` can't find Aseprite" → §E, set `ASEPRITE_PATH` (live path is unaffected).
