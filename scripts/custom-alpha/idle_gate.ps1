param([Parameter(Mandatory)][string]$SocketPath)
$ErrorActionPreference = 'Stop'

# Windows PowerShell 7 has AF_UNIX through .NET; use the known local-control
# WebSocket transport instead of Python socket.AF_UNIX (which Windows lacks).
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Net.Http;
using System.Net.Sockets;
using System.Threading;
using System.Threading.Tasks;
public sealed class CustomAlphaUnixSocketConnector
{
  private readonly string path;
  public CustomAlphaUnixSocketConnector(string path) => this.path = path;
  public Func<SocketsHttpConnectionContext, CancellationToken, ValueTask<Stream>> Callback => ConnectAsync;
  private async ValueTask<Stream> ConnectAsync(SocketsHttpConnectionContext context, CancellationToken token)
  {
    var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
    try { await socket.ConnectAsync(new UnixDomainSocketEndPoint(path), token).ConfigureAwait(false); return new NetworkStream(socket, true); }
    catch { socket.Dispose(); throw; }
  }
}
'@

function New-RpcConnection([string]$Path) {
  $handler = [Net.Http.SocketsHttpHandler]::new()
  $handler.UseProxy = $false
  $handler.ConnectCallback = [CustomAlphaUnixSocketConnector]::new($Path).Callback
  $invoker = [Net.Http.HttpMessageInvoker]::new($handler)
  $webSocket = [Net.WebSockets.ClientWebSocket]::new()
  [void]$webSocket.ConnectAsync([Uri]'ws://codex.local/', $invoker, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
  [pscustomobject]@{ webSocket = $webSocket; invoker = $invoker; nextId = 1 }
}

function Send-RpcMessage($Connection, [object]$Message) {
  $bytes = [Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $Message -Depth 16 -Compress))
  $segment = [ArraySegment[byte]]::new($bytes)
  [void]$Connection.webSocket.SendAsync($segment, [Net.WebSockets.WebSocketMessageType]::Text, $true, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
}

function Receive-RpcMessage($Connection) {
  $buffer = [byte[]]::new(65536)
  $memory = [IO.MemoryStream]::new()
  $cancel = [Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(20))
  try {
    do {
      $segment = [ArraySegment[byte]]::new($buffer)
      $frame = $Connection.webSocket.ReceiveAsync($segment, $cancel.Token).GetAwaiter().GetResult()
      if ($frame.MessageType -eq [Net.WebSockets.WebSocketMessageType]::Close) { throw 'websocket closed' }
      if ($frame.Count) { $memory.Write($buffer, 0, $frame.Count) }
      if ($memory.Length -gt 2097152) { throw 'RPC reply exceeded limit' }
    } while (-not $frame.EndOfMessage)
    ConvertFrom-Json -InputObject ([Text.Encoding]::UTF8.GetString($memory.ToArray())) -AsHashtable
  } finally { $cancel.Dispose(); $memory.Dispose() }
}

function Invoke-Rpc($Connection, [string]$Method, [hashtable]$Parameters) {
  $id = $Connection.nextId
  $Connection.nextId = $id + 1
  Send-RpcMessage $Connection @{ jsonrpc = '2.0'; id = $id; method = $Method; params = $Parameters }
  while ($true) {
    $message = Receive-RpcMessage $Connection
    if (-not $message.ContainsKey('id') -or [string]$message.id -ne [string]$id) { continue }
    if ($message.ContainsKey('error') -and $message.error) { throw "RPC $Method returned an error" }
    return $message.result
  }
}

$connection = $null
$state = 'UNKNOWN'
$exitCode = 1
try {
  $connection = New-RpcConnection $SocketPath
  [void](Invoke-Rpc $connection 'initialize' @{
    clientInfo = @{ name = 'custom-alpha-updater'; title = 'Custom alpha updater'; version = '1' }
    capabilities = @{ experimentalApi = $true; requestAttestation = $false; mcpServerOpenaiFormElicitation = $false; extensions = @{} }
  })
  Send-RpcMessage $connection @{ jsonrpc = '2.0'; method = 'initialized'; params = @{} }
  $cursor = $null
  $seen = @{}
  for ($page = 0; $page -lt 100; $page++) {
    $params = @{ limit = 100 }
    if ($cursor) { $params.cursor = $cursor }
    $result = Invoke-Rpc $connection 'thread/list' $params
    if (-not $result.ContainsKey('data') -or $result.data -isnot [array]) { throw 'thread/list shape is unknown' }
    foreach ($thread in $result.data) {
      $status = $thread.status
      if ($status -is [System.Collections.IDictionary]) { $status = $status.type }
      if ([string]$status -notin @('notLoaded', 'not_loaded')) { throw 'a loaded thread or unknown status exists' }
    }
    $cursor = $result.nextCursor
    if (-not $cursor) { $state = 'IDLE'; $exitCode = 0; break }
    if ($seen.ContainsKey([string]$cursor)) { throw 'thread/list cursor repeated' }
    $seen[[string]$cursor] = $true
  }
} catch {
  $state = 'UNKNOWN'
  $exitCode = 1
} finally {
  if ($connection) {
    try { $connection.webSocket.Abort() } catch { }
    try { $connection.webSocket.Dispose() } catch { }
    try { $connection.invoker.Dispose() } catch { }
  }
}
[Console]::Out.WriteLine($state)
exit $exitCode
