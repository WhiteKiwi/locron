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
$clock = [Diagnostics.Stopwatch]::StartNew()
while ($clock.ElapsedMilliseconds -lt 30000) {
    $counter++
    [IO.File]::WriteAllText([IO.Path]::Combine($directory, 'generic-heartbeat'), $counter.ToString([Globalization.CultureInfo]::InvariantCulture))
    [Threading.Thread]::Sleep(25)
}
throw 'crash host was not terminated inside its owned fixture budget'
