param(
  [string]$ExtensionSource = "$PSScriptRoot\..\aseprite-mcp-plugin",
  [string]$AsepriteExtensionsDir = "$env:APPDATA\Aseprite\extensions"
)

$ErrorActionPreference = "Stop"

if (!(Test-Path -LiteralPath $ExtensionSource)) {
  throw "Extension source not found: $ExtensionSource"
}

$target = Join-Path $AsepriteExtensionsDir "aseprite-mcp-plugin"
New-Item -ItemType Directory -Force -Path $target | Out-Null

Copy-Item -LiteralPath (Join-Path $ExtensionSource "package.json") -Destination (Join-Path $target "package.json") -Force
Copy-Item -LiteralPath (Join-Path $ExtensionSource "plugin.lua") -Destination (Join-Path $target "plugin.lua") -Force

$manifest = Get-Content -LiteralPath (Join-Path $target "package.json") -Raw | ConvertFrom-Json

Write-Host "Installed Aseprite MCP plugin"
Write-Host "Target: $target"
Write-Host "Version: $($manifest.version)"
Write-Host ""
Write-Host "Next steps:"
Write-Host "1. Restart Aseprite so the extension reloads."
Write-Host "2. Start your MCP client/server."
Write-Host "3. In Aseprite, use Help > MCP Server > Connect to MCP Server if needed."
Write-Host "4. Run live_status and live_get_capabilities from the MCP client."

