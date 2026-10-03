$directory = [string]$request.directory
$clock = [Diagnostics.Stopwatch]::StartNew()
$fixedPath = [IO.Path]::Combine($directory, 'fixed-pid')
$genericPath = [IO.Path]::Combine($directory, 'generic-pids.json')
$spawnPath = [IO.Path]::Combine($directory, 'generic-spawn-pid')
while (-not [IO.File]::Exists($fixedPath) -or -not [IO.File]::Exists($genericPath) -or -not [IO.File]::Exists($spawnPath)) {
    if ($clock.ElapsedMilliseconds -ge 30000) { throw 'owned target facts did not arrive' }
    [Threading.Thread]::Sleep(5)
}
$fixed = [uint32]::Parse([IO.File]::ReadAllText($fixedPath), [Globalization.CultureInfo]::InvariantCulture)
$facts = [IO.File]::ReadAllText($genericPath) | & $locronFromJson
$spawned = [uint32]::Parse([IO.File]::ReadAllText($spawnPath), [Globalization.CultureInfo]::InvariantCulture)
if ($spawned -eq 0 -or $spawned -ne [uint32]$facts.generic) { throw 'generic child does not match its actual spawn receipt' }
$identities = [uint32[]]@($fixed, [uint32]$facts.generic, [uint32]$facts.descendant)
if ($identities.Count -ne 3 -or $identities[0] -eq 0 -or $identities[1] -eq 0 -or $identities[2] -eq 0 -or $identities[0] -eq $identities[1] -or $identities[0] -eq $identities[2] -or $identities[1] -eq $identities[2]) {
    throw 'invalid live target identities'
}
$processes = @()
foreach ($identity in $identities) {
    $process = [Diagnostics.Process]::GetProcessById([int]$identity)
    $handle = $process.Handle
    if ($handle -eq [IntPtr]::Zero -or $process.HasExited -or $process.WaitForExit(0)) { throw 'target exited before handle binding' }
    $processes += $process
}
$pending = [IO.Path]::Combine($directory, 'handles-bound.pending')
[IO.File]::WriteAllText($pending, 'three-live-handles')
[IO.File]::Move($pending, [IO.Path]::Combine($directory, 'handles-bound'))
foreach ($process in $processes) {
    $remaining = 30000 - $clock.ElapsedMilliseconds
    if ($remaining -le 0 -or -not $process.WaitForExit([int]$remaining) -or $clock.ElapsedMilliseconds -ge 30000) {
        throw 'associated target handle did not signal exit before deadline'
    }
}
$pending = [IO.Path]::Combine($directory, 'exits-confirmed.pending')
[IO.File]::WriteAllText($pending, 'three-associated-exits')
[IO.File]::Move($pending, [IO.Path]::Combine($directory, 'exits-confirmed'))
while (-not [IO.File]::Exists([IO.Path]::Combine($directory, 'observer-release'))) {
    if ($clock.ElapsedMilliseconds -ge 30000) { throw 'observer release did not arrive' }
    [Threading.Thread]::Sleep(5)
}
foreach ($process in $processes) { $process.Dispose() }
@{exits=3} | & $locronToJson -Compress
