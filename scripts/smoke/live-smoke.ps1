param(
  [string]$HostName = "127.0.0.1",
  [int]$Port = 9876,
  [int]$TimeoutSeconds = 20
)

$ErrorActionPreference = "Stop"

function ConvertTo-ProtocolJson($obj) {
  $obj | ConvertTo-Json -Depth 20 -Compress
}

function Receive-TextMessage($socket, [int]$timeoutSeconds) {
  $buffer = New-Object byte[] 65536
  $segment = [ArraySegment[byte]]::new($buffer)
  $cts = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds($timeoutSeconds))
  $result = $socket.ReceiveAsync($segment, $cts.Token).GetAwaiter().GetResult()
  if ($result.MessageType -eq [System.Net.WebSockets.WebSocketMessageType]::Close) {
    throw "WebSocket closed during smoke test"
  }
  [Text.Encoding]::UTF8.GetString($buffer, 0, $result.Count)
}

function Send-LiveCommand($socket, [string]$type, $payload = $null, $target = $null) {
  $script:NextId += 1
  $request = [ordered]@{
    protocol = "aseprite-live-edit"
    version = 1
    id = "smoke-$script:NextId"
    type = $type
  }
  if ($null -ne $target) { $request.target = $target }
  if ($null -ne $payload) { $request.payload = $payload }

  $json = ConvertTo-ProtocolJson $request
  $bytes = [Text.Encoding]::UTF8.GetBytes($json)
  $segment = [ArraySegment[byte]]::new($bytes)
  $socket.SendAsync($segment, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()

  while ($true) {
    $text = Receive-TextMessage $socket $TimeoutSeconds
    $message = $text | ConvertFrom-Json
    if ($message.type -eq "hello") {
      Write-Host "Plugin hello: $($message.result | ConvertTo-Json -Depth 10 -Compress)"
      continue
    }
    if ($message.id -ne $request.id) {
      Write-Host "Ignoring unrelated live message: $text"
      continue
    }
    if ($message.ok -ne $true) {
      throw "Live command failed: $type :: $($message.error | ConvertTo-Json -Depth 10 -Compress)"
    }
    return $message.result
  }
}

$prefix = "http://$HostName`:$Port/"
$listener = [System.Net.HttpListener]::new()
$listener.Prefixes.Add($prefix)
$listener.Start()
Write-Host "Live smoke WebSocket server listening on ws://$HostName`:$Port"
Write-Host "Open Aseprite with the MCP plugin installed. The plugin should connect automatically, or use Help > MCP Server > Connect to MCP Server."

$socket = $null
try {
  $contextTask = $listener.GetContextAsync()
  if (!$contextTask.Wait([TimeSpan]::FromSeconds($TimeoutSeconds))) {
    throw "Timed out waiting for Aseprite plugin connection"
  }
  $context = $contextTask.Result
  if (!$context.Request.IsWebSocketRequest) {
    $context.Response.StatusCode = 400
    $context.Response.Close()
    throw "Received non-WebSocket request"
  }

  $wsContext = $context.AcceptWebSocketAsync([System.Management.Automation.Language.NullString]::Value).GetAwaiter().GetResult()
  $socket = $wsContext.WebSocket
  Write-Host "Aseprite plugin connected."

  $script:NextId = 0

  Send-LiveCommand $socket "get_capabilities" | Out-Null
  Send-LiveCommand $socket "get_sprite_info" | Out-Null
  Send-LiveCommand $socket "ensure_layer" @{ name = "AI Smoke Layer" } | Out-Null
  Send-LiveCommand $socket "rename_layer" @{ name = "AI Smoke Layer"; newName = "AI Smoke Layer Renamed" } | Out-Null
  Send-LiveCommand $socket "set_layer_properties" @{ name = "AI Smoke Layer Renamed"; visible = $true; editable = $true; opacity = 180 } | Out-Null
  Send-LiveCommand $socket "ensure_frames" @{ count = 2; duration = 0.12 } | Out-Null
  Send-LiveCommand $socket "new_cel" @{ layer = "AI Smoke Layer Renamed"; frame = 2; replace = $true } | Out-Null
  Send-LiveCommand $socket "draw_pixels" @{ pixels = @(@{ x = 0; y = 0; color = "#ff00ffff" }) } @{ layer = "AI Smoke Layer Renamed"; frame = 2 } | Out-Null
  Send-LiveCommand $socket "set_cel_properties" @{ layer = "AI Smoke Layer Renamed"; frame = 2; opacity = 120; data = "smoke" } | Out-Null
  Send-LiveCommand $socket "new_tag" @{ name = "AI Smoke Tag"; fromFrame = 1; toFrame = 2; color = "#00ffffff" } | Out-Null
  Send-LiveCommand $socket "set_tag_properties" @{ name = "AI Smoke Tag"; newName = "AI Smoke Tag Renamed"; repeats = 1 } | Out-Null
  Send-LiveCommand $socket "new_slice" @{ name = "AI Smoke Slice"; x = 0; y = 0; width = 8; height = 8; replace = $true } | Out-Null
  Send-LiveCommand $socket "set_slice_properties" @{ name = "AI Smoke Slice"; newName = "AI Smoke Slice Renamed"; x = 1; y = 1; width = 8; height = 8 } | Out-Null
  Send-LiveCommand $socket "set_selection" @{ mode = "replace"; x = 0; y = 0; width = 4; height = 4 } | Out-Null
  Send-LiveCommand $socket "set_selection" @{ mode = "deselect" } | Out-Null
  Send-LiveCommand $socket "list_palette" @{ from = 0; limit = 4 } | Out-Null
  Send-LiveCommand $socket "delete_slice" @{ name = "AI Smoke Slice Renamed" } | Out-Null
  Send-LiveCommand $socket "delete_tag" @{ name = "AI Smoke Tag Renamed" } | Out-Null
  Send-LiveCommand $socket "delete_cel" @{ layer = "AI Smoke Layer Renamed"; frame = 2 } | Out-Null
  Send-LiveCommand $socket "delete_frame" @{ frame = 2 } | Out-Null
  Send-LiveCommand $socket "delete_layer" @{ name = "AI Smoke Layer Renamed" } | Out-Null
  Send-LiveCommand $socket "save_sprite" | Out-Null

  Write-Host "Live smoke test passed."
}
finally {
  if ($socket -and $socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
    $socket.CloseAsync([System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure, "smoke complete", [Threading.CancellationToken]::None).GetAwaiter().GetResult()
  }
  if ($socket) { $socket.Dispose() }
  $listener.Stop()
  $listener.Close()
}
