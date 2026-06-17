<#
.SYNOPSIS
  Cleanly uninstall the Aseprite Pixel-Art plugin's local footprint (Windows).

.DESCRIPTION
  Restores the pre-install state (checklist 1.5):
    1. Stop the MCP server + standalone bridge processes.
    2. Free the live ports (9876 plugin, 9877 control) if still held.
    3. Remove the Aseprite Lua extension.
  Does NOT delete your sprites or the repo. After running, also remove the plugin
  from Claude Code (/plugin, or `claude plugin uninstall aseprite-pixel-art`).
#>
param(
  [int]$PluginPort = 9876,
  [int]$ControlPort = 9877,
  [string]$AsepriteExtensionsDir = "$env:APPDATA\Aseprite\extensions"
)

$ErrorActionPreference = "Continue"

Write-Host "== Aseprite Pixel-Art plugin uninstall ==" -ForegroundColor Cyan

# 1. Stop processes.
Write-Host "`n[1/3] Stopping MCP server + bridge processes..."
foreach ($name in @("aseprite_mcp", "aseprite-live-bridge")) {
  $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
  if ($procs) {
    $procs | Stop-Process -Force -ErrorAction SilentlyContinue
    Write-Host "  Stopped $name ($($procs.Count) process(es))"
  } else {
    Write-Host "  $name not running"
  }
}

# 2. Free ports if anything still holds them.
Write-Host "`n[2/3] Freeing ports $PluginPort / $ControlPort..."
foreach ($port in @($PluginPort, $ControlPort)) {
  try {
    $conns = Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction Stop
    foreach ($c in $conns) {
      try {
        Stop-Process -Id $c.OwningProcess -Force -ErrorAction Stop
        Write-Host "  Freed port $port (pid $($c.OwningProcess))"
      } catch {}
    }
  } catch {
    Write-Host "  Port $port already free"
  }
}

# 3. Remove the Aseprite extension.
Write-Host "`n[3/3] Removing the Aseprite extension..."
$extTarget = Join-Path $AsepriteExtensionsDir "aseprite-mcp-plugin"
if (Test-Path -LiteralPath $extTarget) {
  Remove-Item -LiteralPath $extTarget -Recurse -Force
  Write-Host "  Removed $extTarget"
} else {
  Write-Host "  Extension not present at $extTarget"
}

Write-Host "`nDone. Final manual step:" -ForegroundColor Green
Write-Host "  Remove the plugin from Claude Code (/plugin menu, or"
Write-Host "  'claude plugin uninstall aseprite-pixel-art'), and restart Aseprite."
