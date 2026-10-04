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
$file = lc-N $heartbeat $clock 30000
while ($clock.ElapsedMilliseconds -lt 30000) {
    $counter++
    lc-A $file $counter 21 $clock 30000
    [Threading.Thread]::Sleep(25)
}
throw 'crash host was not terminated inside its owned fixture budget'

# locron-heartbeat-append-common
$a3 = @'
function lc-C($a7, [int]$c1) {
if ($a7.ElapsedMilliseconds -ge $c1) { throw 'append deadline elapsed' }
}
function lc-N([string]$d0, $a7, [int]$c1) {
lc-C $a7 $c1
$b5 = [IO.FileStream]::new($d0, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::ReadWrite)
lc-C $a7 $c1
return $b5
}
function lc-A($b5, [uint64]$d9, [int]$e0, $a7, [int]$c1) {
if ($d9 -lt 1 -or $d9 -gt 2048 -or $e0 -lt 1 -or $e0 -gt 21) { throw 'append bound' }
$a4 = [Text.Encoding]::ASCII.GetBytes($d9.ToString('D20', [Globalization.CultureInfo]::InvariantCulture) + "`n")
lc-C $a7 $c1
$b5.Write($a4, 0, $e0)
lc-C $a7 $c1
$b5.Flush()
lc-C $a7 $c1
}
'@
. ([scriptblock]::Create($a3))

# locron-heartbeat-append-proof-body
$b1 = [string]$request.directory
$c1 = [int]$request.budget_ms
if ($c1 -le 0 -or $c1 -gt 30000) { throw 'append proof budget' }
$a7 = [Diagnostics.Stopwatch]::StartNew()
function lc-L {
$c0 = $c1 - $a7.ElapsedMilliseconds
if ($c0 -le 0) { throw 'append deadline elapsed' }
return [int]$c0
}
function lc-B($b5) {
$null = lc-L
$null = $b5.Seek(0, [IO.SeekOrigin]::Begin)
$null = lc-L
$a4 = [byte[]]::new(43009)
$b0 = 0
while ($b0 -lt $a4.Length) {
$null = lc-L
$d3 = $b5.Read($a4, $b0, $a4.Length - $b0)
$null = lc-L
if ($d3 -eq 0) { break }
$b0 += $d3
}
if ($b0 -eq 0) { return ,([byte[]]::new(0)) }
return ,([byte[]]$a4[0..($b0 - 1)])
}
function lc-Q($b5) {
$a4 = lc-B $b5
$b3 = [Text.Encoding]::ASCII.GetBytes('00000000000000000001' + "`n" + '00000000000000000002')
if ($a4.Length -ne 41) { throw 'partial append length' }
for ($i = 0; $i -lt 41; $i++) {
if ($a4[$i] -ne $b3[$i]) { throw 'partial append bytes' }
}
}
$c2 = [IO.Path]::Combine($b1, 'powershell-append-rename')
$c3 = [IO.Path]::Combine($b1, 'powershell-append-renamed')
$b2 = [IO.Path]::Combine($b1, 'powershell-append-delete')
$d8 = @('?', '?', '?', '?')
function lc-F($b4) {
try {
if ($null -eq $b4 -or $b4 -isnot [Exception]) { return '?' }
$c9 = @()
for ($c8 = 0; $c8 -lt 5 -and $null -ne $b4; $c8++) {
$b9 = 'other'
if ($b4 -is [IO.IOException]) { $b9 = 'io' }
elseif ($b4 -is [UnauthorizedAccessException]) { $b9 = 'access' }
$c9 += $b9 + '=' + ([int]$b4.HResult).ToString([Globalization.CultureInfo]::InvariantCulture)
$b4 = $b4.InnerException
}
$e1 = '='
if ($null -ne $b4) { $e1 = '~' }
return ($e1 + ($c9 -join ','))
} catch { return '?' }
}
function lc-R([int]$d1) {
$c7 = $false
$c6 = $false
$a1 = $false
try { $null = lc-L; $a1 = $true; [IO.File]::Move($c2, $c3) } catch {
$c7 = $true
if ($a1) { $d8[2 * $d1] = lc-F $_.Exception }
}
$null = lc-L
$a1 = $false
try { $null = lc-L; $a1 = $true; [IO.File]::Delete($b2) } catch {
$c6 = $true
if ($a1) { $d8[2 * $d1 + 1] = lc-F $_.Exception }
}
$null = lc-L
if (-not $c7 -or -not $c6) { throw 'append mutation unexpectedly admitted' }
}
$a8 = $a3 + @'
# locron-heartbeat-append-child-begin
$ErrorActionPreference = 'Stop'
$b1 = [Environment]::GetEnvironmentVariable('LOCRON_LOADER_CRASH_DIRECTORY')
$c1 = [int]::Parse([Environment]::GetEnvironmentVariable('LOCRON_HEARTBEAT_PROBE_BUDGET_MS'), [Globalization.CultureInfo]::InvariantCulture)
if ($c1 -le 0 -or $c1 -gt 30000) { throw 'append child budget' }
$a7 = [Diagnostics.Stopwatch]::StartNew()
$e5 = @()
try {
foreach ($c4 in @('powershell-append-rename', 'powershell-append-delete')) {
$e4 = lc-N ([IO.Path]::Combine($b1, $c4)) $a7 $c1
$e5 += $e4
lc-A $e4 1 21 $a7 $c1
lc-A $e4 2 20 $a7 $c1
}
$d6 = lc-N ([IO.Path]::Combine($b1, 'powershell-append-ready')) $a7 $c1
try {
$a4 = [Text.Encoding]::ASCII.GetBytes($PID.ToString([Globalization.CultureInfo]::InvariantCulture))
lc-C $a7 $c1
$d6.Write($a4, 0, $a4.Length)
lc-C $a7 $c1
$d6.Flush()
lc-C $a7 $c1
} finally { $d6.Dispose() }
while ($a7.ElapsedMilliseconds -lt $c1) { [Threading.Thread]::Sleep(5) }
throw 'append child was not terminated'
} finally { foreach ($e4 in $e5) { $e4.Dispose() } }
# locron-heartbeat-append-child-end
'@
$b8 = [Diagnostics.ProcessStartInfo]::new()
$null = lc-L
$b8.FileName = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$null = lc-L
$b8.Arguments = '-NoProfile -NonInteractive -EncodedCommand ' + [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($a8))
$b8.UseShellExecute = $false
$b8.CreateNoWindow = $true
$b8.RedirectStandardInput = $true
$b8.RedirectStandardOutput = $true
$b8.RedirectStandardError = $true
$b8.EnvironmentVariables['LOCRON_LOADER_CRASH_DIRECTORY'] = $b1
$b8.EnvironmentVariables['LOCRON_HEARTBEAT_PROBE_BUDGET_MS'] = (lc-L).ToString([Globalization.CultureInfo]::InvariantCulture)
$null = lc-L
$a9 = '"' + $b8.FileName + '" ' + $b8.Arguments + [char]0
if ($a9.Length -gt 31743) { throw 'append command bound' }
$null = lc-L
$a5 = [Diagnostics.Process]::Start($b8)
$d5 = $null
$d4 = $null
try {
$null = lc-L
$a6 = [uint32]$a5.Id
$null = lc-L
$b6 = $a5.Handle
$null = lc-L
$a2 = -not $a5.HasExited
$null = lc-L
if ($a6 -eq 0 -or $b6 -eq [IntPtr]::Zero -or -not $a2) { throw 'append publisher not live' }
$d2 = $a6.ToString([Globalization.CultureInfo]::InvariantCulture)
$d6 = [IO.Path]::Combine($b1, 'powershell-append-ready')
while ($true) {
$null = lc-L
if ([IO.File]::Exists($d6)) {
$null = lc-L
$d7 = [IO.FileStream]::new($d6, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
try { $a4 = lc-B $d7 } finally { $d7.Dispose() }
if ($a4.Length -gt 0) {
$e2 = [Text.Encoding]::ASCII.GetString($a4)
if (-not $d2.StartsWith($e2, [StringComparison]::Ordinal)) { throw 'append publisher identity mismatch' }
if ($e2 -ceq $d2) { break }
}
}
$null = lc-L
if ($a5.HasExited) { throw 'append publisher exited before ready' }
$null = lc-L
[Threading.Thread]::Sleep(5)
}
$null = lc-L
$d5 = [IO.FileStream]::new($c2, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
$d4 = [IO.FileStream]::new($b2, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
lc-Q $d5
lc-Q $d4
$null = lc-L
if ($a5.HasExited) { throw 'append publisher not live before kill' }
$null = lc-L
lc-R 0
$null = lc-L
$a5.Kill()
$null = lc-L
if (-not $a5.WaitForExit((lc-L))) { throw 'append publisher not reaped' }
$null = lc-L
if ($a5.ExitCode -eq 0) { throw 'append publisher not hard stopped' }
$null = lc-L
if ($a5.StandardOutput.ReadToEnd().Length -ne 0) { throw 'unexpected append child output' }
$null = lc-L
if ($a5.StandardError.ReadToEnd().Length -ne 0) { throw 'unexpected append child error' }
$null = lc-L
lc-Q $d5
lc-Q $d4
lc-R 1
$d5.Dispose(); $d5 = $null
$d4.Dispose(); $d4 = $null
$null = lc-L
[IO.File]::Move($c2, $c3)
$null = lc-L
$b5 = [IO.FileStream]::new($c3, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
try { lc-Q $b5 } finally { $b5.Dispose() }
$null = lc-L
[IO.File]::Delete($b2)
$null = lc-L
$a0 = $false
try { $null = [IO.File]::GetAttributes($b2) } catch {
$e = $_.Exception
for ($i = 0; $i -lt 5 -and $null -ne $e; $i++) {
if ($e -is [IO.FileNotFoundException]) { $a0 = $true; break }
$e = $e.InnerException
}
}
$null = lc-L
if (-not $a0) { throw 'append deletion unknown' }
$b7 = $false
try { $e3 = lc-N $c3 $a7 $c1; $e3.Dispose() } catch { $b7 = $true }
$null = lc-L
if (-not $b7) { throw 'append CreateNew collision admitted' }
$b5 = [IO.FileStream]::new($c3, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
try { lc-Q $b5 } finally { $b5.Dispose() }
$null = lc-L
[IO.File]::Delete($c3)
$null = lc-L
$c5 = lc-N $c3 $a7 $c1
try { lc-A $c5 1 21 $a7 $c1 } finally { $c5.Dispose() }
$null = lc-L
$b5 = [IO.FileStream]::new($c3, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
$null = lc-L
try {
$a4 = lc-B $b5
if ($a4.Length -ne 21 -or [Text.Encoding]::ASCII.GetString($a4) -cne ('00000000000000000001' + "`n")) { throw 'append collision continuation bytes' }
} finally { $b5.Dispose() }
$null = lc-L
@{root_pid=[uint32]$PID; publisher_pid=$a6; staged=$true; killed=$true; reaped=$true; reader_refused=$true; rename_released=$true; delete_released=$true; collision=$b7; count=1; tail_len=20; refusals=$d8} | & $locronToJson -Compress
} finally {
if ($null -ne $d5) { $d5.Dispose() }
if ($null -ne $d4) { $d4.Dispose() }
$a5.Dispose()
}
