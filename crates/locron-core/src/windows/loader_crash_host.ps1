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
$first = $true
$clock = [Diagnostics.Stopwatch]::StartNew()
while ($clock.ElapsedMilliseconds -lt 30000) {
    $counter++
    Publish-LocronHeartbeatSnapshot (New-LocronHeartbeatSnapshot $heartbeat $counter) $heartbeat $first
    $first = $false
    [Threading.Thread]::Sleep(25)
}
throw 'crash host was not terminated inside its owned fixture budget'
