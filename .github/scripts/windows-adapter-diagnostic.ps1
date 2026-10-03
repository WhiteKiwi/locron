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

# The Core helpers retain the guarded bootstrap, actual child Job and pipe cleanup.
# Each exact mode owns its independent 45-second helper and original 30/3-second bounds.
$PSNativeCommandUseErrorActionPreference = $false
$previousMode = $env:LOCRON_STOCK_LOADER_FIXTURE
$failedModes = @()
try {
    foreach ($probeMode in @('diagnostic-bootstrap', 'diagnostic-small', 'diagnostic-60k')) {
        $env:LOCRON_STOCK_LOADER_FIXTURE = $probeMode
        Write-Output "Guarded stock adapter diagnostic: $probeMode"
        cargo test -p locron-core --lib --locked windows::loader_tests::owned_loader_fixture_child -- --exact --nocapture --test-threads=1
        $probeExitCode = $LASTEXITCODE
        @{
            diagnostic = 'stock-adapter-selection'
            mode = $probeMode
            fixture_exit_code = $probeExitCode
            passed = ($probeExitCode -eq 0)
        } | ConvertTo-Json -Compress
        if ($probeExitCode -ne 0) { $failedModes += $probeMode }
    }
} finally {
    $env:LOCRON_STOCK_LOADER_FIXTURE = $previousMode
}
if ($failedModes.Count -gt 0) {
    throw "A bounded guarded stock adapter diagnostic failed: $($failedModes -join ', '). Inspect the recorded Core child facts."
}
