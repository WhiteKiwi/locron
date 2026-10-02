# Run after native tests so these observations do not warm or replace their cold-start gates.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$stock = [IO.Path]::Combine($env:SystemRoot, 'System32\WindowsPowerShell\v1.0\powershell.exe')
if (-not [IO.Path]::IsPathRooted($stock) -or -not [IO.File]::Exists($stock)) {
    throw 'The absolute stock Windows PowerShell executable is unavailable.'
}
$header = [IO.File]::ReadAllBytes($stock)
$peOffset = [BitConverter]::ToInt32($header, 0x3c)
$machine = [BitConverter]::ToUInt16($header, $peOffset + 4)
@{
    diagnostic = 'stock-adapter-host'
    stock_path = $stock
    stock_machine = ('0x{0:X4}' -f $machine)
    stock_file_version = [Diagnostics.FileVersionInfo]::GetVersionInfo($stock).FileVersion
    host_ps_version = $PSVersionTable.PSVersion.ToString()
    host_process_architecture = [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
    host_os_architecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    host_is64bit = [Environment]::Is64BitProcess
} | ConvertTo-Json -Compress

function Invoke-StockProbe([string]$Name, [string]$Source, [byte[]]$InputBytes) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $stock
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($Source))
    $start.Arguments = '-NoLogo -NoProfile -NonInteractive -EncodedCommand ' + $encoded
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
    $start.StandardErrorEncoding = [Text.UTF8Encoding]::new($false)
    $start.EnvironmentVariables.Remove('PSModulePath')
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    $inputClosed = $false
    $timedOut = $false
    $cleanupConfirmed = $false
    $started = $false
    $pidValue = $null
    $failure = $null
    $write = $null
    $output = $null
    $errors = $null
    try {
        if (-not $process.Start()) { throw 'Stock child did not start.' }
        $started = $true
        $pidValue = $process.Id
        $output = $process.StandardOutput.ReadToEndAsync()
        $errors = $process.StandardError.ReadToEndAsync()
        $write = $process.StandardInput.BaseStream.WriteAsync($InputBytes, 0, $InputBytes.Length)
        do {
            if ($write.IsCompleted -and -not $inputClosed) {
                if ($write.IsFaulted) { throw $write.Exception }
                $process.StandardInput.Close()
                $inputClosed = $true
            }
            if ($process.HasExited -and $output.IsCompleted -and $errors.IsCompleted) { break }
            if ($clock.Elapsed.TotalSeconds -ge 30) { $timedOut = $true; break }
            [Threading.Thread]::Sleep(25)
        } while ($true)
    } catch {
        $failure = $_.Exception.ToString()
    } finally {
        if ($started) {
            if (-not $process.HasExited) { $process.Kill() }
            $cleanupConfirmed = $process.WaitForExit(3000)
        }
        if ($started -and -not $inputClosed) { $process.StandardInput.Close() }
    }
    $outText = if ($null -ne $output -and $output.IsCompletedSuccessfully) { $output.Result } else { '' }
    $errText = if ($null -ne $errors -and $errors.IsCompletedSuccessfully) { $errors.Result } else { '' }
    $parsed = $null
    try { if ($outText.Length -le 131072 -and $outText.Length -gt 0) { $parsed = $outText | ConvertFrom-Json } } catch { $failure = $_.Exception.Message }
    $passed = (-not $timedOut) -and $cleanupConfirmed -and ($null -eq $failure) -and ($process.ExitCode -eq 0) -and ($null -ne $parsed) -and $parsed.sid.StartsWith('S-1-') -and $parsed.ps_version.StartsWith('5.1.')
    if ($passed -and $Name.StartsWith('stock-structured')) {
        $expected = [Text.Encoding]::UTF8.GetString($InputBytes) | ConvertFrom-Json
        $passed = ($parsed.echo -ceq $expected.echo) -and ($parsed.payload_length -eq $expected.payload.Length)
    }
    $record = @{
        diagnostic = $Name
        pid = $pidValue
        elapsed_ms = [int]$clock.Elapsed.TotalMilliseconds
        input_bytes = $InputBytes.Length
        input_write_completed = ($null -ne $write -and $write.IsCompletedSuccessfully)
        input_closed = $inputClosed
        stdout_completed = ($null -ne $output -and $output.IsCompleted)
        stderr_completed = ($null -ne $errors -and $errors.IsCompleted)
        timed_out = $timedOut
        cleanup_confirmed = $cleanupConfirmed
        exit_code = if ($cleanupConfirmed) { $process.ExitCode } else { $null }
        stderr = $errText.Substring(0, [Math]::Min($errText.Length, 4096))
        error = $failure
        child_facts = $parsed
        passed = $passed
    }
    [Console]::Out.WriteLine(($record | ConvertTo-Json -Depth 6 -Compress))
    $process.Dispose()
    return $passed
}

$direct = @'
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
@{stage='entered'; ps_version=$PSVersionTable.PSVersion.ToString(); is64bit=[Environment]::Is64BitProcess; process_architecture=$env:PROCESSOR_ARCHITECTURE; sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value} | ConvertTo-Json -Compress
'@
$structured = @'
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
try {
    $request = [Console]::In.ReadToEnd() | ConvertFrom-Json
    @{stage='stdin-parsed'; echo=$request.echo; payload_length=$request.payload.Length; sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value; ps_version=$PSVersionTable.PSVersion.ToString(); is64bit=[Environment]::Is64BitProcess; process_architecture=$env:PROCESSOR_ARCHITECTURE} | ConvertTo-Json -Compress
} catch { [Console]::Error.WriteLine($_.Exception.ToString()); exit 1 }
'@
$results = @()
$results += Invoke-StockProbe 'stock-bootstrap-no-stdin' $direct ([byte[]]@())
$small = [Text.Encoding]::UTF8.GetBytes((@{echo='ARM64 한글 δοκιμή'; payload='small'} | ConvertTo-Json -Compress))
$results += Invoke-StockProbe 'stock-structured-stdin-small' $structured $small
$large = [Text.Encoding]::UTF8.GetBytes((@{echo='ARM64 한글 δοκιμή'; payload=('x' * 60000)} | ConvertTo-Json -Compress))
$results += Invoke-StockProbe 'stock-structured-stdin-60k' $structured $large
if ($results -contains $false) { throw 'A bounded stock adapter diagnostic failed; inspect the recorded child facts.' }
