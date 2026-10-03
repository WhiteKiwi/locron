if ($locronInput.Length -eq 0 -or $null -eq $request) { throw 'structured diagnostic did not receive its input' }
$current = [Diagnostics.Process]::GetCurrentProcess()
$image = [IO.FileStream]::new($current.MainModule.FileName, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::ReadWrite)
try {
    $header = [byte[]]::new(64)
    if ($image.Read($header, 0, 64) -ne 64 -or $header[0] -ne 77 -or $header[1] -ne 90) { throw 'stock image lacks DOS header' }
    $offset = [BitConverter]::ToInt32($header, 60)
    if ($offset -lt 64 -or $offset -gt ($image.Length - 6)) { throw 'stock image has an invalid PE offset' }
    [void]$image.Seek($offset, [IO.SeekOrigin]::Begin)
    $pe = [byte[]]::new(6)
    if ($image.Read($pe, 0, 6) -ne 6 -or $pe[0] -ne 80 -or $pe[1] -ne 69 -or $pe[2] -ne 0 -or $pe[3] -ne 0) { throw 'stock image lacks PE header' }
    $machine = [BitConverter]::ToUInt16($pe, 4)
} finally { $image.Dispose(); $current.Dispose() }
@{stage='stdin-parsed'; pid=[uint32]$PID; input_bytes=[Text.Encoding]::UTF8.GetByteCount($locronInput); echo=[string]$request.echo; payload_length=[int]$request.payload.Length; ps_version=$PSVersionTable.PSVersion.ToString(); is64bit=[Environment]::Is64BitProcess; process_architecture=$env:PROCESSOR_ARCHITECTURE; machine=[int]$machine; sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value} | & $locronToJson -Compress
