$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$PSModuleAutoLoadingPreference = 'None'
# LOCRON_STOCK_JSON_BOOTSTRAP

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
            default { throw 'unknown filesystem operation' }
        }
        [Console]::Out.WriteLine((@{version=1; id=$request.id; pid=$PID; result=$result} | & $locronToJson -Compress -Depth 6))
        [Console]::Out.Flush()
    } catch {
        [Console]::Error.WriteLine($_.Exception.Message)
        exit 1
    }
}
