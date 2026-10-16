$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$PSModuleAutoLoadingPreference = 'None'
# LOCRON_STOCK_JSON_BOOTSTRAP

function Invoke-LocronGuardedMutation([string]$operation, $path) {
    try {
        if ($operation -ceq 'remove_file') {
            if ($path -isnot [string]) { throw 'invalid removal path' }
            [IO.File]::Delete($path)
        } elseif ($operation -ceq 'move_empty_directory_new') {
            if ($path -isnot [pscustomobject]) { throw 'invalid move paths' }
            $names = @($path.PSObject.Properties.Name)
            if ($names.Count -ne 2 -or ($names | Where-Object { $_ -notin @('source', 'destination') }) -or $path.source -isnot [string] -or $path.destination -isnot [string]) { throw 'invalid move paths' }
            [IO.DirectoryInfo]::new([string]$path.source).MoveTo([string]$path.destination)
        } else { throw 'unknown guarded mutation' }
        return @{outcome='completed';hresult=$null;win32_error=$null}
    } catch {
        $cause = $_.Exception
        for ($depth = 0; $depth -lt 8 -and $null -ne $cause.InnerException; $depth++) { $cause = $cause.InnerException }
        if ($null -ne $cause.InnerException) { return @{outcome='adapter_refusal';hresult=$null;win32_error=$null} }
        $hr = [int64]$cause.HResult
        $code = $hr -band 65535
        $kind = $cause.GetType().FullName
        # Only exact known Framework native-I/O conversions may supply an OS code.
        $native = ($kind -ceq 'System.IO.IOException') -or
            ($kind -ceq 'System.UnauthorizedAccessException' -and $code -eq 5) -or
            ($kind -ceq 'System.IO.DirectoryNotFoundException' -and $code -eq 3) -or
            ($kind -ceq 'System.IO.FileNotFoundException' -and $code -eq 2) -or
            ($kind -ceq 'System.IO.DriveNotFoundException' -and $code -eq 15)
        if ($native -and ($hr -band 4294901760) -eq 2147942400 -and $code -gt 0) {
            return @{outcome='native_refusal';hresult=[int]$hr;win32_error=[int]$code}
        }
        return @{outcome='adapter_refusal';hresult=$null;win32_error=$null}
    }
}

while ($null -ne ($line = [Console]::In.ReadLine())) {
    try {
        if ([Text.Encoding]::UTF8.GetByteCount($line) -gt 65536) { throw 'filesystem request too large' }
        $request = $line | & $locronFromJson
        $names = @($request.PSObject.Properties.Name)
        if ($names.Count -ne 4 -or ($names | Where-Object { $_ -notin @('version', 'id', 'operation', 'path') })) {
            throw 'invalid filesystem request fields'
        }
        if ($request.version -ne 1 -or $request.id -isnot [string] -or $request.id -notmatch '^[1-9][0-9]{0,19}$') {
            throw 'invalid filesystem request identity'
        }
        $sid = [Security.Principal.WindowsIdentity]::GetCurrent().User
        switch -Exact ([string]$request.operation) {
            'sid' {
                if ($null -ne $request.path) { throw 'SID request has a path' }
                $result = $sid.Value
            }
            'create_directory' {
                if ($request.path -isnot [string]) { throw 'missing directory path' }
                $acl = [Security.AccessControl.DirectorySecurity]::new()
                $acl.SetOwner($sid)
                $acl.SetAccessRuleProtection($true, $false)
                $inherit = [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit'
                foreach ($principal in @($sid, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
                    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal, [Security.AccessControl.FileSystemRights]::FullControl, $inherit, [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow))
                }
                [IO.DirectoryInfo]::new($request.path).Create($acl)
                $result = @{created=$true}
            }
            'create_file' {
                if ($request.path -isnot [string]) { throw 'missing file path' }
                $acl = [Security.AccessControl.FileSecurity]::new()
                $acl.SetOwner($sid)
                $acl.SetAccessRuleProtection($true, $false)
                foreach ($principal in @($sid, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
                    $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal, [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow))
                }
                try {
                    $file = [IO.FileStream]::new($request.path, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::FullControl, [IO.FileShare]::ReadWrite, 4096, [IO.FileOptions]::None, $acl)
                    $file.Dispose()
                    $result = @{created=$true}
                } catch {
                    $cause = $_.Exception
                    while ($cause.InnerException) { $cause = $cause.InnerException }
                    $code = $cause.HResult -band 65535
                    if (($cause -is [IO.IOException]) -and ($code -in @(80, 183))) {
                        $result = @{created=$false; win32_error=$code}
                    } else { throw }
                }
            }
            'remove_file' { $result = Invoke-LocronGuardedMutation 'remove_file' $request.path }
            'move_empty_directory_new' { $result = Invoke-LocronGuardedMutation 'move_empty_directory_new' $request.path }
            default { throw 'unknown filesystem operation' }
        }
        [Console]::Out.WriteLine((@{version=1; id=$request.id; pid=$PID; result=$result} | & $locronToJson -Compress -Depth 6))
        [Console]::Out.Flush()
    } catch {
        [Console]::Error.WriteLine($_.Exception.Message)
        exit 1
    }
}
