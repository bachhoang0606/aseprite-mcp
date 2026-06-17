<#
.SYNOPSIS
  Install the Aseprite Pixel-Art plugin locally (Windows).

.DESCRIPTION
  One command to go from a clean checkout to a working live plugin:
    1. Build the Rust MCP server + standalone bridge in release mode.
    2. Install/refresh the Aseprite Lua extension.
    3. Verify the binaries the plugin manifest points at exist.
    4. Print the remaining (manual) steps: set ASEPRITE_PATH, load the plugin,
       open Aseprite, run live_preflight.

  Idempotent: safe to re-run after pulling changes. Checklist 1.2 / 1.4 / 1.5.
#>
param(
  [string]$RepoRoot = "$PSScriptRoot\..",
  [string]$AsepriteExtensionsDir = "$env:APPDATA\Aseprite\extensions"
)

$ErrorActionPreference = "Stop"
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path

Write-Host "== Aseprite Pixel-Art plugin install ==" -ForegroundColor Cyan
Write-Host "Repo: $RepoRoot"

# 1. Build release binaries (MCP server + standalone bridge).
Write-Host "`n[1/4] Building release binaries (cargo build --release)..." -ForegroundColor Cyan
Push-Location $RepoRoot
try {
  cargo build --release
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit $LASTEXITCODE)" }
} finally {
  Pop-Location
}

# 2. Install the Aseprite Lua extension.
Write-Host "`n[2/4] Installing the Aseprite extension..." -ForegroundColor Cyan
$extSource = Join-Path $RepoRoot "scripts\aseprite-mcp-plugin"
$extTarget = Join-Path $AsepriteExtensionsDir "aseprite-mcp-plugin"
New-Item -ItemType Directory -Force -Path $extTarget | Out-Null
Copy-Item -LiteralPath (Join-Path $extSource "package.json") -Destination $extTarget -Force
Copy-Item -LiteralPath (Join-Path $extSource "plugin.lua")   -Destination $extTarget -Force
Write-Host "  Extension -> $extTarget"

# 3. Verify the binaries the manifest references.
Write-Host "`n[3/4] Verifying binaries..." -ForegroundColor Cyan
$server = Join-Path $RepoRoot "target\release\aseprite_mcp.exe"
$bridge = Join-Path $RepoRoot "target\release\aseprite-live-bridge.exe"
foreach ($bin in @($server, $bridge)) {
  if (Test-Path -LiteralPath $bin) { Write-Host "  OK  $bin" }
  else { throw "Missing expected binary: $bin" }
}

# 4. Next steps (manual).
Write-Host "`n[4/4] Done. Remaining steps:" -ForegroundColor Green
Write-Host "  a) Set ASEPRITE_PATH to your Aseprite.exe (used by batch tools), e.g.:"
Write-Host '       setx ASEPRITE_PATH "C:\Program Files\Aseprite\Aseprite.exe"'
Write-Host "  b) Load the plugin in Claude Code:"
Write-Host "       - project/personal: this repo's .claude-plugin/plugin.json is picked up"
Write-Host "         when the repo is your workspace (run /reload-plugins), or"
Write-Host "       - install from a marketplace once published (claude plugin install ...)."
Write-Host "  c) Open Aseprite (extension auto-connects to the bridge on port 9876)."
Write-Host "  d) In Claude Code: run live_preflight until ready=true, then draw with live_* / /pixel-* skills."
Write-Host "`nNote: the standalone bridge (aseprite-live-bridge) auto-spawns from the MCP server; it owns ports 9876/9877."
