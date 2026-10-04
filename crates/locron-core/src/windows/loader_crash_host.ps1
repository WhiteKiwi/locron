$directory = [string]$request.directory
$heartbeatSource = @'
function New-LocronHeartbeatSnapshot([string]$path, [uint64]$value) {
    $candidate = [IO.Path]::Combine([IO.Path]::GetDirectoryName($path), [Guid]::NewGuid().ToString('N') + '.heartbeat-snapshot')
    $stream = [IO.FileStream]::new($candidate, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $bytes = [Text.Encoding]::ASCII.GetBytes($value.ToString([Globalization.CultureInfo]::InvariantCulture))
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush()
    } finally {
        $stream.Dispose()
    }
    return $candidate
}
function Publish-LocronHeartbeatSnapshot([string]$candidate, [string]$path, [bool]$first) {
    if ($first) {
        [IO.File]::Move($candidate, $path)
    } else {
        [IO.File]::Replace($candidate, $path, [System.Management.Automation.Language.NullString]::Value, $false)
    }
}
'@
. ([scriptblock]::Create($heartbeatSource))

# locron-heartbeat-proof-body
if ([string]$request.operation -eq 'heartbeat-publication-proof') {
    $budget = [int]$request.budget_ms
    if ($budget -le 0 -or $budget -gt 30000) { throw 'invalid heartbeat proof budget' }
    $clock = [Diagnostics.Stopwatch]::StartNew()
    function Remaining-HeartbeatProof {
        $remaining = $budget - $clock.ElapsedMilliseconds
        if ($remaining -le 0) { throw 'heartbeat proof deadline elapsed' }
        return [int]$remaining
    }
    function Read-HeartbeatProof([string]$path) {
        $null = Remaining-HeartbeatProof
        $stream = [IO.FileStream]::new($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
        try {
            $bytes = [byte[]]::new(65)
            $count = 0
            while ($count -lt $bytes.Length) {
                $null = Remaining-HeartbeatProof
                $read = $stream.Read($bytes, $count, $bytes.Length - $count)
                if ($read -eq 0) { break }
                $count += $read
            }
            $null = Remaining-HeartbeatProof
            if ($count -gt 64) { throw 'oversized heartbeat proof counter' }
            return [Text.Encoding]::ASCII.GetString($bytes, 0, $count)
        } finally {
            $stream.Dispose()
        }
    }
    $path = [IO.Path]::Combine($directory, 'powershell-publication-heartbeat')
    $null = Remaining-HeartbeatProof
    Publish-LocronHeartbeatSnapshot (New-LocronHeartbeatSnapshot $path 7) $path $true
    if ((Read-HeartbeatProof $path) -cne '7') { throw 'incorrect first PowerShell counter' }
    $collisionCandidate = New-LocronHeartbeatSnapshot $path 99
    $collision = $false
    try {
        $null = Remaining-HeartbeatProof
        Publish-LocronHeartbeatSnapshot $collisionCandidate $path $true
    } catch {
        $failure = $_.Exception
        while ($null -ne $failure.InnerException) { $failure = $failure.InnerException }
        $code = $failure.HResult -band 0xffff
        if ($failure -isnot [IO.IOException] -or ($code -ne 80 -and $code -ne 183)) { throw }
        $collision = $true
    }
    if (-not $collision -or (Read-HeartbeatProof $path) -cne '7') { throw 'initial PowerShell collision did not refuse' }
    $null = Remaining-HeartbeatProof
    [IO.File]::Delete($collisionCandidate)
    $held = [IO.FileStream]::new($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $shareRefusal = $false
    $blockedCandidate = New-LocronHeartbeatSnapshot $path 8
    try {
        try {
            $null = Remaining-HeartbeatProof
            Publish-LocronHeartbeatSnapshot $blockedCandidate $path $false
        } catch {
            $failure = $_.Exception
            while ($null -ne $failure.InnerException) { $failure = $failure.InnerException }
            if ($failure -isnot [IO.IOException] -or ($failure.HResult -band 0xffff) -ne 32) { throw }
            $shareRefusal = $true
        }
        if (-not $shareRefusal -or (Read-HeartbeatProof $path) -cne '7') { throw 'PowerShell replacement did not refuse incompatible sharing' }
    } finally {
        $held.Dispose()
    }
    $null = Remaining-HeartbeatProof
    [IO.File]::Delete($blockedCandidate)
    $publisherSource = $heartbeatSource + @'

$ErrorActionPreference = 'Stop'
$directory = [Environment]::GetEnvironmentVariable('LOCRON_LOADER_CRASH_DIRECTORY')
$budget = [int]::Parse([Environment]::GetEnvironmentVariable('LOCRON_HEARTBEAT_PROBE_BUDGET_MS'), [Globalization.CultureInfo]::InvariantCulture)
if ($budget -le 0 -or $budget -gt 30000) { throw 'invalid staged publisher budget' }
$clock = [Diagnostics.Stopwatch]::StartNew()
$path = [IO.Path]::Combine($directory, 'powershell-publication-heartbeat')
$unpublished = New-LocronHeartbeatSnapshot $path 8
if ($clock.ElapsedMilliseconds -ge $budget) { throw 'publisher expired before staging' }
$ready = [IO.Path]::Combine($directory, 'powershell-stage-ready')
Publish-LocronHeartbeatSnapshot (New-LocronHeartbeatSnapshot $ready ([uint64]$PID)) $ready $true
while ($clock.ElapsedMilliseconds -lt $budget) { [Threading.Thread]::Sleep(5) }
throw 'staged publisher was not terminated'
'@
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
    $info.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($publisherSource))
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardInput = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.EnvironmentVariables['LOCRON_LOADER_CRASH_DIRECTORY'] = $directory
    $info.EnvironmentVariables['LOCRON_HEARTBEAT_PROBE_BUDGET_MS'] = (Remaining-HeartbeatProof).ToString([Globalization.CultureInfo]::InvariantCulture)
    $publisher = [Diagnostics.Process]::Start($info)
    try {
        $publisherPid = [uint32]$publisher.Id
        if ($publisherPid -eq 0 -or $publisher.Handle -eq [IntPtr]::Zero -or $publisher.HasExited) { throw 'direct PowerShell publisher is not live' }
        $ready = [IO.Path]::Combine($directory, 'powershell-stage-ready')
        while ($true) {
            $null = Remaining-HeartbeatProof
            if ([IO.File]::Exists($ready)) { break }
            if ($publisher.HasExited) { throw 'PowerShell publisher exited before staging' }
            [Threading.Thread]::Sleep(5)
        }
        if ((Read-HeartbeatProof $ready) -cne $publisherPid.ToString([Globalization.CultureInfo]::InvariantCulture)) { throw 'staged publisher identity mismatch' }
        if ($publisher.HasExited -or (Read-HeartbeatProof $path) -cne '7') { throw 'staged snapshot changed the published counter' }
        $null = Remaining-HeartbeatProof
        $publisher.Kill()
        if (-not $publisher.WaitForExit((Remaining-HeartbeatProof))) { throw 'direct PowerShell publisher did not exit' }
        $null = Remaining-HeartbeatProof
        if ($publisher.ExitCode -eq 0) { throw 'staged publisher was not hard-stopped' }
        if ((Read-HeartbeatProof $path) -cne '7') { throw 'hard stop lost the complete PowerShell counter' }
        $null = Remaining-HeartbeatProof
        Publish-LocronHeartbeatSnapshot (New-LocronHeartbeatSnapshot $path 8) $path $false
        if ((Read-HeartbeatProof $path) -cne '8') { throw 'next PowerShell snapshot was not published' }
        @{root_pid=[uint32]$PID; publisher_pid=$publisherPid; initial_collision=$collision; replacement_share_refusal=$shareRefusal; staged=$true; killed=$true; reaped=$true; old=7; new=8} | & $locronToJson -Compress
    } finally {
        $publisher.Dispose()
    }
    return
}

# locron-heartbeat-normal-body
$directory = [string]$request.directory
$info = [Diagnostics.ProcessStartInfo]::new()
$info.FileName = [string]$request.executable
$info.Arguments = '--exact windows::loader_tests::owned_loader_fixture_child --nocapture'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.EnvironmentVariables['LOCRON_STOCK_LOADER_FIXTURE'] = 'native-heartbeat'
$info.EnvironmentVariables['LOCRON_LOADER_CRASH_DIRECTORY'] = $directory
$descendant = [Diagnostics.Process]::Start($info)
$handle = $descendant.Handle
if ($handle -eq [IntPtr]::Zero -or $descendant.HasExited) { throw 'native descendant is not alive' }
$facts = @{generic=[uint32]$PID; descendant=[uint32]$descendant.Id} | & $locronToJson -Compress
$pending = [IO.Path]::Combine($directory, 'generic-pids.pending')
[IO.File]::WriteAllText($pending, $facts)
[IO.File]::Move($pending, [IO.Path]::Combine($directory, 'generic-pids.json'))
$counter = [uint64]0
$heartbeat = [IO.Path]::Combine($directory, 'generic-heartbeat')
$clock = [Diagnostics.Stopwatch]::StartNew()
$file = New-Beat $heartbeat $clock 30000
while ($clock.ElapsedMilliseconds -lt 30000) {
    $counter++
    Add-Beat $file $counter 21 $clock 30000
    [Threading.Thread]::Sleep(25)
}
throw 'crash host was not terminated inside its owned fixture budget'

# locron-heartbeat-append-common
$beatSource = @'
function Check-Time($clock, [int]$limit) {
if ($clock.ElapsedMilliseconds -ge $limit) { throw 'append deadline elapsed' }
}
function New-Beat([string]$path, $clock, [int]$limit) {
Check-Time $clock $limit
$file = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
Check-Time $clock $limit
return $file
}
function Add-Beat($file, [uint64]$seq, [int]$size, $clock, [int]$limit) {
if ($seq -lt 1 -or $seq -gt 2048 -or $size -lt 1 -or $size -gt 21) { throw 'append bound' }
$bytes = [Text.Encoding]::ASCII.GetBytes($seq.ToString('D20', [Globalization.CultureInfo]::InvariantCulture) + "`n")
Check-Time $clock $limit
$file.Write($bytes, 0, $size)
Check-Time $clock $limit
$file.Flush()
Check-Time $clock $limit
}
'@
. ([scriptblock]::Create($beatSource))

# locron-heartbeat-append-proof-body
$dir = [string]$request.directory
$limit = [int]$request.budget_ms
if ($limit -le 0 -or $limit -gt 30000) { throw 'append proof budget' }
$clock = [Diagnostics.Stopwatch]::StartNew()
function Left {
$left = $limit - $clock.ElapsedMilliseconds
if ($left -le 0) { throw 'append deadline elapsed' }
return [int]$left
}
function Read-Bytes($file) {
$null = Left
$null = $file.Seek(0, [IO.SeekOrigin]::Begin)
$null = Left
$bytes = [byte[]]::new(43009)
$count = 0
while ($count -lt $bytes.Length) {
$null = Left
$read = $file.Read($bytes, $count, $bytes.Length - $count)
$null = Left
if ($read -eq 0) { break }
$count += $read
}
if ($count -eq 0) { return ,([byte[]]::new(0)) }
return ,([byte[]]$bytes[0..($count - 1)])
}
function Require($file) {
$bytes = Read-Bytes $file
$exact = [Text.Encoding]::ASCII.GetBytes('00000000000000000001' + "`n" + '00000000000000000002')
if ($bytes.Length -ne 41) { throw 'partial append length' }
for ($i = 0; $i -lt 41; $i++) {
if ($bytes[$i] -ne $exact[$i]) { throw 'partial append bytes' }
}
}
$move = [IO.Path]::Combine($dir, 'powershell-append-rename')
$moved = [IO.Path]::Combine($dir, 'powershell-append-renamed')
$drop = [IO.Path]::Combine($dir, 'powershell-append-delete')
function Refuse {
$noMove = $false
$noDrop = $false
try { $null = Left; [IO.File]::Move($move, $moved) } catch { $noMove = $true }
$null = Left
try { $null = Left; [IO.File]::Delete($drop) } catch { $noDrop = $true }
$null = Left
if (-not $noMove -or -not $noDrop) { throw 'append mutation unexpectedly admitted' }
}
$code = $beatSource + @'
# locron-heartbeat-append-child-begin
$ErrorActionPreference = 'Stop'
$dir = [Environment]::GetEnvironmentVariable('LOCRON_LOADER_CRASH_DIRECTORY')
$limit = [int]::Parse([Environment]::GetEnvironmentVariable('LOCRON_HEARTBEAT_PROBE_BUDGET_MS'), [Globalization.CultureInfo]::InvariantCulture)
if ($limit -le 0 -or $limit -gt 30000) { throw 'append child budget' }
$clock = [Diagnostics.Stopwatch]::StartNew()
$writers = @()
try {
foreach ($name in @('powershell-append-rename', 'powershell-append-delete')) {
$writer = New-Beat ([IO.Path]::Combine($dir, $name)) $clock $limit
$writers += $writer
Add-Beat $writer 1 21 $clock $limit
Add-Beat $writer 2 20 $clock $limit
}
$ready = New-Beat ([IO.Path]::Combine($dir, 'powershell-append-ready')) $clock $limit
try {
$bytes = [Text.Encoding]::ASCII.GetBytes($PID.ToString([Globalization.CultureInfo]::InvariantCulture))
Check-Time $clock $limit
$ready.Write($bytes, 0, $bytes.Length)
Check-Time $clock $limit
$ready.Flush()
Check-Time $clock $limit
} finally { $ready.Dispose() }
while ($clock.ElapsedMilliseconds -lt $limit) { [Threading.Thread]::Sleep(5) }
throw 'append child was not terminated'
} finally { foreach ($writer in $writers) { $writer.Dispose() } }
# locron-heartbeat-append-child-end
'@
$info = [Diagnostics.ProcessStartInfo]::new()
$null = Left
$info.FileName = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$null = Left
$info.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($code))
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.EnvironmentVariables['LOCRON_LOADER_CRASH_DIRECTORY'] = $dir
$info.EnvironmentVariables['LOCRON_HEARTBEAT_PROBE_BUDGET_MS'] = (Left).ToString([Globalization.CultureInfo]::InvariantCulture)
$null = Left
$command = '"' + $info.FileName + '" ' + $info.Arguments + [char]0
if ($command.Length -gt 31743) { throw 'append command bound' }
$null = Left
$child = [Diagnostics.Process]::Start($info)
$readMove = $null
$readDrop = $null
try {
$null = Left
$childPid = [uint32]$child.Id
$null = Left
$handle = $child.Handle
$null = Left
$alive = -not $child.HasExited
$null = Left
if ($childPid -eq 0 -or $handle -eq [IntPtr]::Zero -or -not $alive) { throw 'append publisher not live' }
$pidText = $childPid.ToString([Globalization.CultureInfo]::InvariantCulture)
$ready = [IO.Path]::Combine($dir, 'powershell-append-ready')
while ($true) {
$null = Left
if ([IO.File]::Exists($ready)) {
$null = Left
$receipt = [IO.FileStream]::new($ready, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
try { $bytes = Read-Bytes $receipt } finally { $receipt.Dispose() }
if ($bytes.Length -gt 0) {
$text = [Text.Encoding]::ASCII.GetString($bytes)
if (-not $pidText.StartsWith($text, [StringComparison]::Ordinal)) { throw 'append publisher identity mismatch' }
if ($text -ceq $pidText) { break }
}
}
$null = Left
if ($child.HasExited) { throw 'append publisher exited before ready' }
$null = Left
[Threading.Thread]::Sleep(5)
}
$null = Left
$readMove = [IO.FileStream]::new($move, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
$readDrop = [IO.FileStream]::new($drop, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
Require $readMove
Require $readDrop
$null = Left
if ($child.HasExited) { throw 'append publisher not live before kill' }
$null = Left
Refuse
$null = Left
$child.Kill()
$null = Left
if (-not $child.WaitForExit((Left))) { throw 'append publisher not reaped' }
$null = Left
if ($child.ExitCode -eq 0) { throw 'append publisher not hard stopped' }
$null = Left
if ($child.StandardOutput.ReadToEnd().Length -ne 0) { throw 'unexpected append child output' }
$null = Left
if ($child.StandardError.ReadToEnd().Length -ne 0) { throw 'unexpected append child error' }
$null = Left
Require $readMove
Require $readDrop
Refuse
$readMove.Dispose(); $readMove = $null
$readDrop.Dispose(); $readDrop = $null
$null = Left
[IO.File]::Move($move, $moved)
$null = Left
$file = [IO.FileStream]::new($moved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
try { Require $file } finally { $file.Dispose() }
$null = Left
[IO.File]::Delete($drop)
$null = Left
$absent = $false
try { $null = [IO.File]::GetAttributes($drop) } catch {
$e = $_.Exception
for ($i = 0; $i -lt 5 -and $null -ne $e; $i++) {
if ($e -is [IO.FileNotFoundException]) { $absent = $true; break }
$e = $e.InnerException
}
}
$null = Left
if (-not $absent) { throw 'append deletion unknown' }
$hit = $false
try { $unexpected = New-Beat $moved $clock $limit; $unexpected.Dispose() } catch { $hit = $true }
$null = Left
if (-not $hit) { throw 'append CreateNew collision admitted' }
$file = [IO.FileStream]::new($moved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
try { Require $file } finally { $file.Dispose() }
$null = Left
[IO.File]::Delete($moved)
$null = Left
$next = New-Beat $moved $clock $limit
try { Add-Beat $next 1 21 $clock $limit } finally { $next.Dispose() }
$null = Left
$file = [IO.FileStream]::new($moved, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = Left
try {
$bytes = Read-Bytes $file
if ($bytes.Length -ne 21 -or [Text.Encoding]::ASCII.GetString($bytes) -cne ('00000000000000000001' + "`n")) { throw 'append collision continuation bytes' }
} finally { $file.Dispose() }
$null = Left
@{root_pid=[uint32]$PID; publisher_pid=$childPid; staged=$true; killed=$true; reaped=$true; reader_refused=$true; rename_released=$true; delete_released=$true; collision=$hit; count=1; tail_len=20} | & $locronToJson -Compress
} finally {
if ($null -ne $readMove) { $readMove.Dispose() }
if ($null -ne $readDrop) { $readDrop.Dispose() }
$child.Dispose()
}
