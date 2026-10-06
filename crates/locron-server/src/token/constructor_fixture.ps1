# Fixed test-only constructor. Core owns the actual adapter Child/Job and original deadline.
$ErrorActionPreference = 'Stop'
$client = $null
$file = $null
$watch = [Diagnostics.Stopwatch]::StartNew()
function Remaining {
    $left = [long]$request.remaining_ms - $watch.ElapsedMilliseconds
    if ($left -le 0) { throw 'constructor fixture clock elapsed' }
    return [int][Math]::Min($left, [int]::MaxValue)
}
try {
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
                if ($request.path -isnot [string]) { throw 'missing file path' }
                $acl = [Security.AccessControl.FileSecurity]::new()
                $acl.SetOwner($sid)
                $acl.SetAccessRuleProtection($true, $false)
                foreach ($principal in @($sid, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
                    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal, [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow))
                }
                try {
                    $file = [IO.FileStream]::new($request.path, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::FullControl, [IO.FileShare]::ReadWrite, 4096, [IO.FileOptions]::None, $acl)
                    # The exact constructor stays live until the bounded release.
                    $client = [Net.Sockets.TcpClient]::new()
                    $connect = $client.ConnectAsync([string]$request.address, [int]$request.port)
                    if (-not $connect.Wait((Remaining))) { throw 'constructor fixture connect elapsed' }
                    $stream = $client.GetStream()
                    $stream.WriteTimeout = Remaining
                    $stats = @{entered=0; attempts=0; sleeps=0; busy=0; acquired=0; token_io=0; decision=0; scratch=0; writes=0; syncs=0; closed=0; renames=0; cleanup=0; cleanup_raw=$null; cleanup_fault=$null; publish=0; candidate_attempts=0; candidate_created=0; candidate_closed=0; candidate_cleanup=0; collision=0; published=0; expired=0}
                    $frame = @{event='Constructor'; sequence=1; pid=$PID; native_pid=$null; nonce=$request.nonce; stats=$stats; ok=$false; kind=''; raw=$null; fault=$null; digest=''; length=$file.Length; elapsed_ms=$watch.ElapsedMilliseconds}
                    $bytes = [Text.Encoding]::UTF8.GetBytes(($frame | & $locronToJson -Compress -Depth 5) + "`n")
                    if ($bytes.Length -gt 1024) { throw 'constructor fixture frame too large' }
                    $stream.Write($bytes, 0, $bytes.Length)
                    $stream.Flush()
                    $command = [Collections.Generic.List[byte]]::new()
                    while ($true) {
                        $stream.ReadTimeout = Remaining
                        $byte = $stream.ReadByte()
                        if ($byte -lt 0) { throw 'constructor fixture peer closed' }
                        if ($byte -eq 10) { break }
                        $command.Add([byte]$byte)
                        if ($command.Count -ge 1024) { throw 'constructor fixture command too large' }
                    }
                    if ([Text.Encoding]::UTF8.GetString($command.ToArray()) -cne '"Release"') { throw 'constructor fixture expected release' }
                    $file.Dispose()
                    $file = $null
                    [Console]::Out.WriteLine((@{disposed=$true; pid=$PID} | & $locronToJson -Compress -Depth 5))
                } finally {
                    if ($null -ne $file) { $file.Dispose() }
                }
} finally {
    if ($null -ne $client) { $client.Dispose() }
}
