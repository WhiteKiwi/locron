# Query-only CI diagnostics. Paths arrive privately on stdin and never enter output.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = [Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)

function Get-ObservationErrorClass($Exception) {
    if ($Exception -is [UnauthorizedAccessException]) { return 'permission_denied' }
    if ($Exception -is [IO.FileNotFoundException] -or $Exception -is [IO.DirectoryNotFoundException]) { return 'not_found' }
    if ($Exception -is [ArgumentException]) { return 'invalid_argument' }
    if ($Exception -is [IO.IOException]) { return 'io' }
    return 'other'
}

function Get-OwnerObservation($Role, $Path) {
    $record = [ordered]@{
        schema = 'locron.windows-owner-observation/v1'
        role = $Role
        status = 'unobserved'
        error_class = $null
        token_owner_matches_user = $null
        token_owner_is_system = $null
        token_owner_is_builtin_admin = $null
        token_impersonation = $null
        file_owner_present = $null
        file_owner_matches_user = $null
        file_owner_matches_token_owner = $null
        file_owner_is_system = $null
        file_owner_is_builtin_admin = $null
        dacl_protected = $null
    }
    $identity = $null
    try {
        $identity = [Security.Principal.WindowsIdentity]::GetCurrent([Security.Principal.TokenAccessLevels]::Query)
        $user = $identity.User
        $owner = $identity.Owner
        if ($null -eq $user -or $null -eq $owner) { throw [InvalidOperationException]::new() }
        $record.token_owner_matches_user = $owner.Equals($user)
        $record.token_owner_is_system = $owner.IsWellKnown([Security.Principal.WellKnownSidType]::LocalSystemSid)
        $record.token_owner_is_builtin_admin = $owner.IsWellKnown([Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid)
        $record.token_impersonation = switch ($identity.ImpersonationLevel) {
            'None' { 'none' }
            'Anonymous' { 'anonymous' }
            'Identification' { 'identification' }
            'Impersonation' { 'impersonation' }
            'Delegation' { 'delegation' }
            default { 'unobserved' }
        }
        if ($null -ne $Path) {
            # A path-based metadata observation never replaces guarded artifact admission.
            $acl = Get-Acl -LiteralPath $Path -ErrorAction Stop
            $fileOwner = $acl.GetOwner([Security.Principal.SecurityIdentifier])
            $record.file_owner_present = $null -ne $fileOwner
            if ($null -ne $fileOwner) {
                $record.file_owner_matches_user = $fileOwner.Equals($user)
                $record.file_owner_matches_token_owner = $fileOwner.Equals($owner)
                $record.file_owner_is_system = $fileOwner.IsWellKnown([Security.Principal.WellKnownSidType]::LocalSystemSid)
                $record.file_owner_is_builtin_admin = $fileOwner.IsWellKnown([Security.Principal.WellKnownSidType]::BuiltinAdministratorsSid)
            }
            $record.dacl_protected = $acl.AreAccessRulesProtected
        }
        $record.status = 'observed'
    } catch {
        $record.error_class = Get-ObservationErrorClass $_.Exception
    } finally {
        if ($null -ne $identity) { $identity.Dispose() }
    }
    return $record
}

try {
    $inputBuffer = [char[]]::new(16385)
    $inputCount = 0
    while ($inputCount -lt $inputBuffer.Length) {
        $read = [Console]::In.Read($inputBuffer, $inputCount, $inputBuffer.Length - $inputCount)
        if ($read -eq 0) { break }
        $inputCount += $read
    }
    if ($inputCount -eq $inputBuffer.Length) { throw [ArgumentException]::new() }
    $inputText = [string]::new($inputBuffer, 0, $inputCount)
    if ([Text.Encoding]::UTF8.GetByteCount($inputText) -gt 16384) { throw [ArgumentException]::new() }
    $request = $inputText | ConvertFrom-Json -ErrorAction Stop
    $names = @($request.PSObject.Properties.Name)
    if ($names.Count -ne 3 -or ($names | Where-Object { $_ -cnotin @('version', 'phase', 'objects') })) { throw [ArgumentException]::new() }
    if (($request.version -isnot [int] -and $request.version -isnot [long]) -or $request.version -ne 1 -or $request.phase -isnot [string] -or $request.phase -cnotin @('native_toolchain', 'producer')) { throw [ArgumentException]::new() }
    $objects = @($request.objects)
    if ($objects.Count -gt 4 -or ($request.phase -ceq 'native_toolchain' -and $objects.Count -ne 0)) { throw [ArgumentException]::new() }
    $seen = [Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    $tokenRole = if ($request.phase -ceq 'native_toolchain') { 'native_toolchain' } else { 'producer_observer_token' }
    $records = @(Get-OwnerObservation $tokenRole $null)
    foreach ($object in $objects) {
        $names = @($object.PSObject.Properties.Name)
        if ($names.Count -ne 2 -or ($names | Where-Object { $_ -cnotin @('role', 'path') })) { throw [ArgumentException]::new() }
        if ($object.role -cnotin @('pr144_namespace', 'pr144_controller', 'pr144_store') -or -not $seen.Add($object.role)) { throw [ArgumentException]::new() }
        if ($object.path -isnot [string] -or -not [IO.Path]::IsPathRooted($object.path)) { throw [ArgumentException]::new() }
        $records += Get-OwnerObservation $object.role $object.path
    }
    $output = @{version = 1; records = $records} | ConvertTo-Json -Compress -Depth 4
    # Include the Windows line terminator in the output allowance.
    if ([Text.Encoding]::UTF8.GetByteCount($output) + 2 -gt 8192) { throw [InvalidOperationException]::new() }
    [Console]::Out.WriteLine($output)
} catch {
    # The caller records a fixed unavailable class; exception streams stay private.
    exit 2
}
