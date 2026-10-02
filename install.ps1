#requires -Version 5.1
<#
.SYNOPSIS
Install an unsigned, verified native Windows release for the current user.
.DESCRIPTION
Uses stock PowerShell/.NET and a private verified locron helper. It never changes
execution policy or machine PATH. Dot-source this file only to load its functions.
#>
[CmdletBinding()]
param(
    [string]$Version,
    [string]$InstallDirectory = [IO.Path]::Combine($env:LOCALAPPDATA, 'Programs', 'locron'),
    [switch]$NoService,
    [switch]$Dashboard,
    [switch]$AddToPath,
    [ValidateSet('Prepare', 'Complete', 'Remove')][string]$Maintenance,
    [string]$Executable,
    [Guid]$Operation = [Guid]::Empty
)

function Get-LocronSid {
    [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
}

function ConvertTo-LocronPath([string]$Path) {
    if ($Path -notmatch '^[A-Za-z]:[\\/]' -or $Path -match '[\x00-\x1f]' -or $Path.Substring(2).Contains(':')) {
        throw 'locron requires an absolute local drive path without alternate streams'
    }
    $full = [IO.Path]::GetFullPath($Path)
    foreach ($part in $full.Substring(3).Split('\')) {
        if ($part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9\u00b9\u00b2\u00b3]|LPT[1-9\u00b9\u00b2\u00b3])(?:\..*)?$') {
            throw 'locron refuses ambiguous Windows path components'
        }
    }
    if ($full.Length -eq 3) { $full } else { $full.TrimEnd('\') }
}

function Assert-LocronDescriptor($Security, [bool]$Private, [bool]$Directory) {
    $sid = Get-LocronSid
    $owner = $Security.GetOwner([Security.Principal.SecurityIdentifier]).Value
    $trusted = @($sid, 'S-1-5-18', 'S-1-5-32-544',
        'S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
    if (($Private -and $owner -ne $sid) -or (-not $Private -and $owner -notin $trusted)) {
        throw 'locron refuses foreign-owned filesystem objects'
    }
    $user = $false
    $system = $false
    foreach ($rule in $Security.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
        if (-not $Private -and ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly)) { continue }
        if ($rule.AccessControlType -ne [Security.AccessControl.AccessControlType]::Allow) {
            if ($Private) { throw 'locron private files require a current-user/SYSTEM allow DACL' }
            continue
        }
        $principal = $rule.IdentityReference.Value
        $mask = [long]$rule.FileSystemRights
        if ($Private) {
            if ($principal -notin @($sid, 'S-1-5-18') -or ($mask -band 0x1f01ff) -ne 0x1f01ff -or
                ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly)) {
                throw 'locron private files require full control only for the current user and SYSTEM'
            }
            if ($Directory -and ($rule.InheritanceFlags -band 3) -ne 3) {
                throw 'locron private directory permissions must inherit onto files and directories'
            }
            $user = $user -or $principal -eq $sid
            $system = $system -or $principal -eq 'S-1-5-18'
        } elseif ($principal -notin $trusted -and ($mask -band 0x500d0152)) {
            # Active writes, deletion or permission changes could mutate an ancestor.
            # Directory CreateDirectories alone does not authorize replacing it.
            throw 'locron refuses an ancestry that another account can replace'
        }
    }
    if ($Private -and (-not $user -or -not $system -or ($Directory -and -not $Security.AreAccessRulesProtected))) {
        throw 'locron requires a protected private directory with current-user and SYSTEM access'
    }
}

function Assert-LocronDirectory([string]$Path, [bool]$Private = $false) {
    $full = ConvertTo-LocronPath $Path
    $directory = [IO.DirectoryInfo]::new($full)
    if (-not $directory.Exists -or ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'locron refuses missing directories and reparse points'
    }
    Assert-LocronDescriptor ($directory.GetAccessControl()) $Private $true
    if ($null -ne $directory.Parent) { $null = Assert-LocronDirectory $directory.Parent.FullName }
    $full
}

function New-LocronPrivateDirectory([string]$Path) {
    $full = ConvertTo-LocronPath $Path
    $directory = [IO.DirectoryInfo]::new($full)
    if ($directory.Exists) { return (Assert-LocronDirectory $full $true) }
    if ($null -eq $directory.Parent) { throw 'locron cannot use a drive root as a private directory' }
    if (-not $directory.Parent.Exists) { $null = New-LocronPrivateDirectory $directory.Parent.FullName }
    $null = Assert-LocronDirectory $directory.Parent.FullName
    $security = [Security.AccessControl.DirectorySecurity]::new()
    $sid = [Security.Principal.SecurityIdentifier]::new((Get-LocronSid))
    $security.SetOwner($sid)
    $security.SetAccessRuleProtection($true, $false)
    foreach ($principal in @($sid, [Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
        $security.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($principal,
            [Security.AccessControl.FileSystemRights]::FullControl,
            [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',
            [Security.AccessControl.PropagationFlags]::None, [Security.AccessControl.AccessControlType]::Allow))
    }
    # The descriptor is supplied to the creation call; existing/raced objects are
    # validated after creation, never repaired or taken over.
    $directory.Create($security)
    Assert-LocronDirectory $full $true
}

function Read-LocronPrivateFile([string]$Path) {
    $full = ConvertTo-LocronPath $Path
    $null = Assert-LocronDirectory ([IO.Path]::GetDirectoryName($full)) $true
    if (([IO.File]::GetAttributes($full) -band [IO.FileAttributes]::ReparsePoint)) { throw 'locron refuses reparse files' }
    $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Assert-LocronDescriptor ($stream.GetAccessControl()) $true $false
        if ($stream.Length -gt 67108864) { throw 'locron file exceeds the staging limit' }
        $buffer = [IO.MemoryStream]::new()
        try { $stream.CopyTo($buffer); return ,$buffer.ToArray() } finally { $buffer.Dispose() }
    } finally { $stream.Dispose() }
}

function Write-LocronPrivateFile([string]$Path, [byte[]]$Bytes) {
    $full = ConvertTo-LocronPath $Path
    $null = Assert-LocronDirectory ([IO.Path]::GetDirectoryName($full)) $true
    $stream = [IO.File]::Open($full, [IO.FileMode]::CreateNew, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        Assert-LocronDescriptor ($stream.GetAccessControl()) $true $false
        $stream.Write($Bytes, 0, $Bytes.Length)
        $stream.Flush($true)
    } finally { $stream.Dispose() }
}

function Get-LocronSha256([byte[]]$Bytes) {
    $hash = [Security.Cryptography.SHA256]::Create()
    try { ([BitConverter]::ToString($hash.ComputeHash($Bytes))).Replace('-', '').ToLowerInvariant() }
    finally { $hash.Dispose() }
}

function ConvertFrom-LocronJson([byte[]]$Bytes) {
    if ($Bytes.Length -gt 131072) { throw 'locron metadata exceeds its size limit' }
    $json = [Text.UTF8Encoding]::new($false, $true).GetString($Bytes)
    # ConvertFrom-Json silently collapses duplicate keys. Reject duplicates in
    # each object first, including escaped/case-variant spellings on PowerShell.
    $objects = [Collections.Generic.Stack[Collections.Generic.HashSet[string]]]::new()
    for ($offset = 0; $offset -lt $json.Length; $offset++) {
        $character = $json[$offset]
        if ($character -eq '{') {
            if ($objects.Count -ge 32) { throw 'locron JSON nesting exceeds its limit' }
            $objects.Push([Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase))
        } elseif ($character -eq '}') {
            if ($objects.Count -eq 0) { throw 'invalid locron JSON object' }
            $null = $objects.Pop()
        } elseif ($character -eq '/' -or $character -eq "'") {
            throw 'locron metadata requires standard JSON without comments or single quotes'
        } elseif ($character -cmatch '[A-Za-z_$]') {
            $tokenEnd = $offset + 1
            while ($tokenEnd -lt $json.Length -and $json[$tokenEnd] -cmatch '[A-Za-z0-9_$]') { $tokenEnd++ }
            $following = $tokenEnd
            while ($following -lt $json.Length -and [char]::IsWhiteSpace($json[$following])) { $following++ }
            if ($following -lt $json.Length -and $json[$following] -eq ':') { throw 'locron JSON fields must be quoted' }
            if ($json.Substring($offset, $tokenEnd - $offset) -cnotin @('true', 'false', 'null', 'e', 'E')) { throw 'invalid locron JSON token' }
            $offset = $tokenEnd - 1
        } elseif ($character -eq '"') {
            $key = [Text.StringBuilder]::new()
            $closed = $false
            while (++$offset -lt $json.Length) {
                $character = $json[$offset]
                if ($character -eq '"') { $closed = $true; break }
                if ([int]$character -lt 32) { throw 'invalid locron JSON string' }
                if ($character -eq '\') {
                    if (++$offset -ge $json.Length) { throw 'truncated locron JSON escape' }
                    $escape = $json[$offset]
                    switch -CaseSensitive ($escape) {
                        '"' { $null = $key.Append('"') }
                        '\' { $null = $key.Append('\') }
                        '/' { $null = $key.Append('/') }
                        'b' { $null = $key.Append([char]8) }
                        'f' { $null = $key.Append([char]12) }
                        'n' { $null = $key.Append([char]10) }
                        'r' { $null = $key.Append([char]13) }
                        't' { $null = $key.Append([char]9) }
                        'u' {
                            if ($offset + 4 -ge $json.Length -or $json.Substring($offset + 1, 4) -cnotmatch '^[a-fA-F0-9]{4}$') {
                                throw 'invalid locron JSON Unicode escape'
                            }
                            $null = $key.Append([char][Convert]::ToInt32($json.Substring($offset + 1, 4), 16))
                            $offset += 4
                        }
                        default { throw 'invalid locron JSON escape' }
                    }
                } else { $null = $key.Append($character) }
            }
            if (-not $closed) { throw 'unterminated locron JSON string' }
            $next = $offset + 1
            while ($next -lt $json.Length -and [char]::IsWhiteSpace($json[$next])) { $next++ }
            if ($next -lt $json.Length -and $json[$next] -eq ':') {
                if ($objects.Count -eq 0 -or -not $objects.Peek().Add($key.ToString())) { throw 'duplicate locron JSON field' }
            }
        }
    }
    if ($objects.Count -ne 0) { throw 'unterminated locron JSON object' }
    ConvertFrom-Json -InputObject $json
}

function Assert-LocronFields($Value, [string[]]$Fields) {
    if ($Value -isnot [Management.Automation.PSCustomObject]) { throw 'locron metadata must be a JSON object' }
    $actual = @($Value.PSObject.Properties.Name)
    if ($actual.Count -ne $Fields.Count) { throw 'locron metadata has missing or unknown fields' }
    foreach ($field in $Fields) {
        if (@($actual | Where-Object { $_ -ceq $field }).Count -ne 1) { throw 'locron metadata has missing or unknown fields' }
    }
}

function Read-LocronReceipt([string]$Directory) {
    $full = Assert-LocronDirectory $Directory $true
    $receipt = ConvertFrom-LocronJson (Read-LocronPrivateFile ([IO.Path]::Combine($full, '.locron-install-receipt-v1')))
    Assert-LocronFields $receipt @('schema', 'sid', 'channel', 'directory', 'executable', 'target', 'version',
        'archive_url', 'archive_sha256', 'binary_sha256', 'files', 'user_path')
    if ($receipt.schema -cne 'locron.install/windows-v1' -or $receipt.sid -cne (Get-LocronSid) -or
        $receipt.channel -cne 'standalone' -or $receipt.target -cne (Get-LocronTarget) -or
        $receipt.version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' -or
        [Version]$receipt.version -lt [Version]'0.10.0' -or
        $receipt.directory -isnot [string] -or $receipt.executable -isnot [string] -or
        -not [StringComparer]::OrdinalIgnoreCase.Equals((ConvertTo-LocronPath $receipt.directory), $full) -or
        -not [StringComparer]::OrdinalIgnoreCase.Equals((ConvertTo-LocronPath $receipt.executable), ([IO.Path]::Combine($full, 'locron.exe')))) {
        throw 'receipt does not authorize this user, channel, native version or exact installation'
    }
    $asset = "locron-v$($receipt.version)-$($receipt.target).zip"
    if ($receipt.archive_url -cne "https://github.com/WhiteKiwi/locron/releases/download/v$($receipt.version)/$asset" -or
        $receipt.archive_sha256 -isnot [string] -or $receipt.archive_sha256 -cnotmatch '^[a-f0-9]{64}$' -or
        $receipt.binary_sha256 -isnot [string] -or $receipt.binary_sha256 -cnotmatch '^[a-f0-9]{64}$') { throw 'invalid canonical receipt digests' }
    $names = @('locron.exe', 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'uninstall.ps1', '.locron-installer.ps1')
    Assert-LocronFields $receipt.files $names
    foreach ($name in $names) {
        if ($receipt.files.$name -isnot [string] -or $receipt.files.$name -cnotmatch '^[a-f0-9]{64}$') { throw 'invalid receipt file digest' }
    }
    if ($receipt.files.'locron.exe' -cne $receipt.binary_sha256) { throw 'receipt executable digests disagree' }
    if ($null -ne $receipt.user_path) {
        Assert-LocronFields $receipt.user_path @('before', 'after', 'before_kind', 'after_kind')
        if ($receipt.user_path.after -isnot [string] -or $receipt.user_path.after_kind -cnotin @('String', 'ExpandString') -or
            (($null -eq $receipt.user_path.before) -ne ($null -eq $receipt.user_path.before_kind)) -or
            ($null -ne $receipt.user_path.before -and ($receipt.user_path.before -isnot [string] -or
                $receipt.user_path.before_kind -cnotin @('String', 'ExpandString')))) { throw 'invalid receipt PATH record' }
    }
    $receipt
}

function Get-LocronHttps([string]$Url, [int]$Limit = 67108864) {
    $uri = [Uri]::new($Url)
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    for ($redirect = 0; $redirect -lt 6; $redirect++) {
        if ($deadline.Elapsed.TotalSeconds -gt 120) { throw 'locron download exceeded its time limit' }
        if ($uri.Scheme -ne 'https' -or -not $uri.IsDefaultPort -or $uri.UserInfo -or ($uri.Host -notin @('api.github.com', 'github.com',
            'release-assets.githubusercontent.com', 'objects.githubusercontent.com', 'github-releases.githubusercontent.com'))) {
            throw 'locron refuses a download outside the canonical HTTPS release transport'
        }
        $request = [Net.HttpWebRequest]::CreateHttp($uri)
        $request.AllowAutoRedirect = $false
        $request.Timeout = 30000
        $request.ReadWriteTimeout = 30000
        $request.UserAgent = 'locron Windows installer'
        $response = $request.GetResponse()
        try {
            if ([int]$response.StatusCode -in @(301, 302, 303, 307, 308)) {
                $uri = [Uri]::new($uri, $response.Headers['Location'])
                continue
            }
            if ([int]$response.StatusCode -ne 200 -or $response.ContentLength -gt $Limit) { throw 'locron download refused' }
            $downloadStream = $response.GetResponseStream()
            $output = [IO.MemoryStream]::new()
            try {
                $buffer = [byte[]]::new(32768)
                while (($count = $downloadStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    if ($deadline.Elapsed.TotalSeconds -gt 120) { throw 'locron download exceeded its time limit' }
                    if ($output.Length + $count -gt $Limit) { throw 'locron download exceeds its size limit' }
                    $output.Write($buffer, 0, $count)
                }
                return ,$output.ToArray()
            } finally { $downloadStream.Dispose(); $output.Dispose() }
        } finally { $response.Dispose() }
    }
    throw 'locron download exceeded the redirect limit'
}

function Get-LocronRelease([string]$SelectedVersion) {
    $suffix = 'latest'
    if ($SelectedVersion) {
        $number = $SelectedVersion -replace '^v', ''
        if ($number -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'expected a stable version' }
        $suffix = 'tags/v' + $number
    }
    $release = [Text.Encoding]::UTF8.GetString((Get-LocronHttps "https://api.github.com/repos/WhiteKiwi/locron/releases/$suffix" 1048576)) | ConvertFrom-Json
    if ($release.draft -or $release.prerelease -or $release.tag_name -notmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') {
        throw 'locron requires a published stable release'
    }
    if ($SelectedVersion -and $release.tag_name -ne 'v' + $number) { throw 'release version differs from the requested version' }
    if ([Version]$release.tag_name.Substring(1) -lt [Version]'0.10.0') { throw 'this release does not publish Windows support' }
    $expected = @((Get-LocronPayloadInventory $release.tag_name) + @('SHA256SUMS.txt', 'install.sh', 'install.ps1', 'uninstall.ps1'))
    $actual = @($release.assets | ForEach-Object { $_.name })
    if ($actual.Count -ne $expected.Count) { throw 'release assets differ from the exact Windows version inventory' }
    foreach ($name in $expected) {
        if (@($actual | Where-Object { $_ -ceq $name }).Count -ne 1) { throw 'release assets contain missing, duplicate or unexpected names' }
    }
    $release
}

function Get-LocronPayloadInventory([string]$Tag) {
    if ($Tag -cnotmatch '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'invalid stable release tag' }
    $number = $Tag.Substring(1)
    foreach ($target in @('aarch64-apple-darwin', 'x86_64-apple-darwin', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnu')) {
        "locron-$Tag-$target.tar.gz"
    }
    foreach ($arch in @('amd64', 'arm64')) { "locron_$number-1_$arch.deb" }
    foreach ($arch in @('aarch64', 'x86_64')) { "locron-$number-1.$arch.rpm" }
    foreach ($target in @('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')) { "locron-$Tag-$target.zip" }
}

function Get-LocronAsset($Release, [string]$Name) {
    $assets = @($Release.assets | Where-Object { $_.name -ceq $Name })
    if ($assets.Count -ne 1 -or $assets[0].digest -notmatch '^sha256:[a-fA-F0-9]{64}$') { throw "missing or ambiguous final release asset: $Name" }
    $url = 'https://github.com/WhiteKiwi/locron/releases/download/' + $Release.tag_name + '/' + $Name
    if ($assets[0].browser_download_url -cne $url) { throw 'release asset URL is not canonical' }
    $bytes = Get-LocronHttps $url
    if ((Get-LocronSha256 $bytes) -cne $assets[0].digest.Substring(7).ToLowerInvariant()) { throw 'release asset digest mismatch' }
    return ,$bytes
}

function Get-LocronChecksum([byte[]]$Sums, [string]$Name, [string]$Tag = '') {
    $found = $null
    $names = @{}
    foreach ($line in [Text.Encoding]::UTF8.GetString($Sums).Split("`n")) {
        $line = $line.TrimEnd("`r")
        if (-not $line) { continue }
        if ($line -cnotmatch '^([a-fA-F0-9]{64}) [ *]([^\\/:\s]+)$' -or $Matches[2] -in @('.', '..') -or
            $names.ContainsKey($Matches[2])) { throw 'invalid or duplicate release checksum entry' }
        $names[$Matches[2]] = $true
        if ($Matches[2] -ceq $Name) { $found = $Matches[1].ToLowerInvariant() }
    }
    if (-not $found) { throw 'no exact archive checksum entry' }
    if ($Tag) {
        $expected = @(Get-LocronPayloadInventory $Tag)
        if ($names.Count -ne $expected.Count) { throw 'checksum inventory is incomplete' }
        foreach ($entry in $expected) {
            if (@($names.Keys | Where-Object { $_ -ceq $entry }).Count -ne 1) { throw 'checksum inventory differs from the exact published payload names' }
        }
    }
    $found
}

function Read-LocronZipNumber([byte[]]$Bytes, [long]$Offset, [int]$Width) {
    if ($Offset -lt 0 -or $Offset + $Width -gt $Bytes.Length) { throw 'truncated Windows ZIP structure' }
    if ($Width -eq 2) { return [long][BitConverter]::ToUInt16($Bytes, [int]$Offset) }
    if ($Width -eq 4) { return [long][BitConverter]::ToUInt32($Bytes, [int]$Offset) }
    throw 'unsupported Windows ZIP field width'
}

function Assert-LocronZipCatalog([byte[]]$Bytes) {
    if ($Bytes.Length -lt 22 -or $Bytes.Length -gt 67108864) { throw 'invalid Windows ZIP size' }
    $end = $Bytes.Length - 22L
    if ((Read-LocronZipNumber $Bytes $end 4) -ne 0x06054b50 -or
        (Read-LocronZipNumber $Bytes ($end + 4) 2) -ne 0 -or
        (Read-LocronZipNumber $Bytes ($end + 6) 2) -ne 0 -or
        (Read-LocronZipNumber $Bytes ($end + 8) 2) -ne 4 -or
        (Read-LocronZipNumber $Bytes ($end + 10) 2) -ne 4 -or
        (Read-LocronZipNumber $Bytes ($end + 20) 2) -ne 0) { throw 'Windows ZIP must have one exact uncommented catalog' }
    $size = Read-LocronZipNumber $Bytes ($end + 12) 4
    $catalog = Read-LocronZipNumber $Bytes ($end + 16) 4
    if ($catalog + $size -ne $end) { throw 'Windows ZIP catalog has prefix/trailer or conflicting bounds' }
    $position = $catalog
    $ranges = @()
    $decoder = [Text.UTF8Encoding]::new($false, $true)
    for ($index = 0; $index -lt 4; $index++) {
        if ($position + 46 -gt $end -or (Read-LocronZipNumber $Bytes $position 4) -ne 0x02014b50) { throw 'invalid Windows ZIP catalog entry' }
        $flags = Read-LocronZipNumber $Bytes ($position + 8) 2
        $method = Read-LocronZipNumber $Bytes ($position + 10) 2
        $crc = Read-LocronZipNumber $Bytes ($position + 16) 4
        $compressed = Read-LocronZipNumber $Bytes ($position + 20) 4
        $expanded = Read-LocronZipNumber $Bytes ($position + 24) 4
        $length = Read-LocronZipNumber $Bytes ($position + 28) 2
        $local = Read-LocronZipNumber $Bytes ($position + 42) 4
        if ($flags -notin @(0, 0x800) -or $method -notin @(0, 8) -or $expanded -gt 67108864 -or
            $length -eq 0 -or $position + 46 + $length -gt $end -or
            (Read-LocronZipNumber $Bytes ($position + 30) 2) -ne 0 -or
            (Read-LocronZipNumber $Bytes ($position + 32) 2) -ne 0 -or
            (Read-LocronZipNumber $Bytes ($position + 34) 2) -ne 0 -or
            $local + 30 -gt $catalog -or (Read-LocronZipNumber $Bytes $local 4) -ne 0x04034b50) {
            throw 'Windows ZIP refuses encryption, descriptors, extra fields, comments and unsupported methods'
        }
        $name = $decoder.GetString($Bytes, [int]($position + 46), [int]$length)
        $localLength = Read-LocronZipNumber $Bytes ($local + 26) 2
        $start = $local + 30 + $localLength
        if ($localLength -ne $length -or $start + $compressed -gt $catalog -or
            (Read-LocronZipNumber $Bytes ($local + 28) 2) -ne 0 -or
            (Read-LocronZipNumber $Bytes ($local + 6) 2) -ne $flags -or
            (Read-LocronZipNumber $Bytes ($local + 8) 2) -ne $method -or
            (Read-LocronZipNumber $Bytes ($local + 14) 4) -ne $crc -or
            (Read-LocronZipNumber $Bytes ($local + 18) 4) -ne $compressed -or
            (Read-LocronZipNumber $Bytes ($local + 22) 4) -ne $expanded -or
            $decoder.GetString($Bytes, [int]($local + 30), [int]$localLength) -cne $name) {
            throw 'Windows ZIP local and central entries disagree'
        }
        $ranges += [ordered]@{ start = $local; end = $start + $compressed }
        $position += 46 + $length
    }
    if ($position -ne $end) { throw 'Windows ZIP has an unexpected catalog entry' }
    $next = 0L
    foreach ($range in @($ranges | Sort-Object { $_.start })) {
        if ($range.start -ne $next) { throw 'Windows ZIP data overlaps or contains an unexpected prefix/gap' }
        $next = $range.end
    }
    if ($next -ne $catalog) { throw 'Windows ZIP has data outside its four exact members' }
}

function Get-LocronArchiveFiles([byte[]]$Bytes, [string]$Tag, [string]$Target) {
    Assert-LocronZipCatalog $Bytes
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $memory = [IO.MemoryStream]::new($Bytes, $false)
    $archive = [IO.Compression.ZipArchive]::new($memory, [IO.Compression.ZipArchiveMode]::Read)
    try {
        $allowed = @('locron.exe', 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE')
        $root = "locron-$Tag-$Target/"
        $files = @{}
        $expanded = 0L
        if ($archive.Entries.Count -ne 4) { throw 'Windows ZIP must contain exactly four files' }
        foreach ($entry in $archive.Entries) {
            $name = $entry.FullName
            if (-not $name.StartsWith($root, [StringComparison]::Ordinal)) { throw 'invalid Windows ZIP member path' }
            $file = $name.Substring($root.Length)
            $attributes = [long]$entry.ExternalAttributes -band 0xffffffffL
            $kind = ($attributes -shr 16) -band 0xf000
            if ($file -cnotin $allowed -or $files.ContainsKey($file) -or $kind -notin @(0, 0x8000) -or ($attributes -band 0x400)) {
                throw 'Windows ZIP contains duplicate, unexpected, linked or reparse members'
            }
            $expanded += $entry.Length
            if ($expanded -gt 67108864) { throw 'Windows ZIP exceeds the expanded size limit' }
            $memberStream = $entry.Open()
            $output = [IO.MemoryStream]::new()
            try {
                $buffer = [byte[]]::new(32768)
                while (($count = $memberStream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    if ($output.Length + $count -gt $entry.Length) { throw 'ZIP member exceeds its declared size' }
                    $output.Write($buffer, 0, $count)
                }
                if ($output.Length -ne $entry.Length) { throw 'ZIP member is truncated' }
                $files[$file] = $output.ToArray()
            } finally { $memberStream.Dispose(); $output.Dispose() }
        }
        $files
    } finally { $archive.Dispose(); $memory.Dispose() }
}

function Assert-LocronPe([byte[]]$Bytes, [string]$Target) {
    if ($Bytes.Length -lt 64 -or $Bytes[0] -ne 0x4d -or $Bytes[1] -ne 0x5a) { throw 'missing PE header' }
    $pe = [long][BitConverter]::ToUInt32($Bytes, 60)
    if ($pe + 176 -gt $Bytes.Length -or [BitConverter]::ToUInt32($Bytes, [int]$pe) -ne 0x4550) { throw 'invalid PE header' }
    $machine = [BitConverter]::ToUInt16($Bytes, [int]$pe + 4)
    $expected = if ($Target -ceq 'x86_64-pc-windows-msvc') { 0x8664 } elseif ($Target -ceq 'aarch64-pc-windows-msvc') { 0xaa64 } else { throw 'unsupported native target' }
    $flags = [BitConverter]::ToUInt16($Bytes, [int]$pe + 22)
    if ($machine -ne $expected -or [BitConverter]::ToUInt16($Bytes, [int]$pe + 24) -ne 0x20b -or ($flags -band 0x2000) -or -not ($flags -band 2)) { throw 'wrong native executable architecture' }
    $sections = [int][BitConverter]::ToUInt16($Bytes, [int]$pe + 6)
    $optionalSize = [int][BitConverter]::ToUInt16($Bytes, [int]$pe + 20)
    $directories = [long][BitConverter]::ToUInt32($Bytes, [int]$pe + 132)
    if ($sections -lt 1 -or $sections -gt 96 -or $directories -lt 5 -or $directories -gt 16 -or
        $optionalSize -lt 112 + 8 * $directories -or $pe + 24 + $optionalSize + 40 * $sections -gt $Bytes.Length -or
        [BitConverter]::ToUInt64($Bytes, [int]$pe + 168) -ne 0) { throw 'initial Windows executable must have an unsigned complete PE header' }
    for ($index = 0; $index -lt $sections; $index++) {
        $row = [int]$pe + 24 + $optionalSize + 40 * $index
        $size = [long][BitConverter]::ToUInt32($Bytes, $row + 16)
        $raw = [long][BitConverter]::ToUInt32($Bytes, $row + 20)
        if ($raw + $size -gt $Bytes.Length) { throw 'Windows executable has a truncated section' }
    }
}

function Get-LocronTarget {
    switch ([Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
        'X64' { 'x86_64-pc-windows-msvc' }
        'Arm64' { 'aarch64-pc-windows-msvc' }
        default { throw 'locron publishes only native Windows x64 and ARM64' }
    }
}

function Get-LocronOperationDirectory([Guid]$Id, [bool]$Create = $true) {
    $root = [IO.Path]::Combine($env:LOCALAPPDATA, 'locron-distribution', 'operations', $Id.ToString('D'))
    if ($Create) { New-LocronPrivateDirectory $root } else { Assert-LocronDirectory $root $true }
}

function Invoke-LocronHelper([string]$Directory, $Request, [byte[]]$Helper, [bool]$Reuse = $false) {
    $path = [IO.Path]::Combine($Directory, 'locron-helper.exe')
    if ($Reuse) {
        if ((Get-LocronSha256 (Read-LocronPrivateFile $path)) -cne $Request.helper_sha256) { throw 'retained operation helper changed' }
    } else {
        Write-LocronPrivateFile $path $Helper
        $Request.helper_sha256 = Get-LocronSha256 $Helper
    }
    $requestName = if ($Reuse) { 'request-' + $Request.kind + '-' + [Guid]::NewGuid().ToString('D') + '.json' } else { 'request.json' }
    $requestPath = [IO.Path]::Combine($Directory, $requestName)
    Write-LocronPrivateFile $requestPath ([Text.UTF8Encoding]::new($false).GetBytes(($Request | ConvertTo-Json -Depth 15 -Compress)))
    # Hold the verified helper leaf without write/delete sharing while launching
    # and waiting. The Rust entrypoint independently guards its ancestry and all
    # operation identities before it changes an installation or registration.
    $helperGuard = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Assert-LocronDescriptor ($helperGuard.GetAccessControl()) $true $false
        if ($helperGuard.Length -gt 67108864) { throw 'operation helper exceeds its size limit' }
        $helperBytes = [IO.MemoryStream]::new()
        try {
            $helperGuard.CopyTo($helperBytes)
            if ((Get-LocronSha256 $helperBytes.ToArray()) -cne $Request.helper_sha256) { throw 'operation helper changed before execution' }
        } finally { $helperBytes.Dispose() }
        # The copied helper maps a different file; this external caller can wait
        # for confirmed completion. CLI self-update uses a pending handoff.
        & $path windows-update-helper --request $requestPath | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "locron operation failed; retained recovery request: $requestPath" }
    } finally { $helperGuard.Dispose() }
    $status = ConvertFrom-LocronJson (Read-LocronPrivateFile ([IO.Path]::Combine($Directory, 'status.json')))
    Assert-LocronFields $status @('schema', 'operation_id', 'sid', 'executable', 'phase', 'current_version', 'new_version', 'updated', 'prepared', 'warnings')
    if ($status.schema -cne 'locron.windows-status/v1' -or $status.operation_id -cne $Request.operation_id -or
        $status.sid -cne $Request.sid -or -not [StringComparer]::OrdinalIgnoreCase.Equals(
            (ConvertTo-LocronPath $status.executable), (ConvertTo-LocronPath $Request.executable)) -or
        $status.phase -notin @('completed', 'prepared', 'removed')) { throw 'locron helper did not confirm the requested operation' }
    $status
}

function Assert-LocronOwnedSourceDescriptor($Security) {
    if ($Security.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne (Get-LocronSid)) { throw 'package executable belongs to another account' }
    $trusted = @((Get-LocronSid), 'S-1-5-18', 'S-1-5-32-544')
    foreach ($rule in $Security.GetAccessRules($true, $true, [Security.Principal.SecurityIdentifier])) {
        if ($rule.PropagationFlags -band [Security.AccessControl.PropagationFlags]::InheritOnly) { continue }
        if ($rule.AccessControlType -eq [Security.AccessControl.AccessControlType]::Allow -and
            $rule.IdentityReference.Value -notin $trusted -and ([long]$rule.FileSystemRights -band 0x500d0156)) {
            throw 'package executable permits another account to mutate it'
        }
    }
}

function Read-LocronOwnedSource([string]$Path) {
    $full = ConvertTo-LocronPath $Path
    $null = Assert-LocronDirectory ([IO.Path]::GetDirectoryName($full))
    if (([IO.File]::GetAttributes($full) -band [IO.FileAttributes]::ReparsePoint)) { throw 'locron refuses reparse package executables' }
    $stream = [IO.File]::Open($full, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        Assert-LocronOwnedSourceDescriptor ($stream.GetAccessControl())
        if ($stream.Length -gt 67108864) { throw 'package executable exceeds its size limit' }
        $buffer = [IO.MemoryStream]::new()
        try { $stream.CopyTo($buffer); return ,$buffer.ToArray() } finally { $buffer.Dispose() }
    } finally { $stream.Dispose() }
}

function Select-LocronPackageBinding([string]$Path, [string]$Target, [object[]]$Entries) {
    $full = ConvertTo-LocronPath $Path
    $selected = @()
    foreach ($entry in $Entries) {
        if ($entry.package_id -cne 'WhiteKiwi.locron' -or $entry.installer_type -cne 'portable' -or
            -not $entry.source_id -or $entry.version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' -or
            -not $entry.install_location -or -not $entry.key -or $entry.key -match '[\\/\x00-\x1f]') { continue }
        $location = ConvertTo-LocronPath $entry.install_location
        $expected = [IO.Path]::Combine($location, "locron-v$($entry.version)-$Target", 'locron.exe')
        if ($entry.legacy_path) {
            $legacy = ConvertTo-LocronPath $entry.legacy_path
            if (-not [StringComparer]::OrdinalIgnoreCase.Equals($legacy, $expected) -and
                -not [StringComparer]::OrdinalIgnoreCase.Equals($legacy, ([IO.Path]::Combine($location, 'locron.exe')))) { continue }
            $expected = $legacy
        }
        if (-not [StringComparer]::OrdinalIgnoreCase.Equals($full, $expected)) { continue }
        $selected += [ordered]@{ key = $entry.key; package_id = 'WhiteKiwi.locron'; source_id = $entry.source_id
            install_location = $location; executable = $full; version = $entry.version; target = $Target }
    }
    if ($selected.Count -ne 1) { throw 'selected executable has no unique current-user portable WinGet binding' }
    $selected[0]
}

function Get-LocronPackage([string]$Path) {
    $full = ConvertTo-LocronPath $Path
    $target = Get-LocronTarget
    $entries = @()
    $hive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::CurrentUser,
        [Microsoft.Win32.RegistryView]::Registry64)
    try {
        $uninstall = $hive.OpenSubKey('Software\Microsoft\Windows\CurrentVersion\Uninstall', $false)
        if ($null -eq $uninstall) { throw 'no current-user WinGet registration' }
        try {
            foreach ($name in $uninstall.GetSubKeyNames()) {
                $key = $uninstall.OpenSubKey($name, $false)
                if ($null -eq $key) { continue }
                try {
                    $entries += [ordered]@{ key = $name
                        package_id = [string]$key.GetValue('WinGetPackageIdentifier')
                        installer_type = [string]$key.GetValue('WinGetInstallerType')
                        source_id = [string]$key.GetValue('WinGetSourceIdentifier')
                        version = [string]$key.GetValue('DisplayVersion')
                        install_location = [string]$key.GetValue('InstallLocation')
                        legacy_path = [string]$key.GetValue('TargetFullPath') }
                } finally { $key.Dispose() }
            }
        } finally { $uninstall.Dispose() }
    } finally { $hive.Dispose() }
    $binding = Select-LocronPackageBinding $full $target $entries
    $release = Get-LocronRelease $binding.version
    $asset = "locron-$($release.tag_name)-$target.zip"
    $archive = Get-LocronAsset $release $asset
    $sums = Get-LocronAsset $release 'SHA256SUMS.txt'
    $digest = Get-LocronSha256 $archive
    if ($digest -cne (Get-LocronChecksum $sums $asset $release.tag_name)) { throw 'WinGet canonical archive checksum mismatch' }
    $files = Get-LocronArchiveFiles $archive $release.tag_name $target
    Assert-LocronPe $files['locron.exe'] $target
    $source = Read-LocronOwnedSource $full
    if ((Get-LocronSha256 $source) -cne (Get-LocronSha256 $files['locron.exe'])) { throw 'WinGet executable differs from the canonical release' }
    # The binary performs authoritative retained-handle ancestry/leaf/registration
    # validation before lifecycle changes. Bootstrap checks and canonical bytes
    # authorize copying trusted code into a strictly private helper directory.
    $binding.binary_sha256 = Get-LocronSha256 $source
    $binding.archive_sha256 = $digest
    [ordered]@{ binding = $binding; helper = $source; archive = $archive }
}

function Invoke-LocronMaintenance([string[]]$ProvidedOptions = @()) {
    if (-not $Executable -or $Version -or $NoService -or $Dashboard -or $AddToPath -or
        @($ProvidedOptions | Where-Object { $_ -in @('Version', 'InstallDirectory', 'NoService', 'Dashboard', 'AddToPath') }).Count) {
        throw 'maintenance requires an exact package executable and accepts no standalone install options'
    }
    if ($Maintenance -ceq 'Complete') {
        if ($Operation -eq [Guid]::Empty) { throw 'Complete requires the prepared operation UUID' }
        $directory = Get-LocronOperationDirectory $Operation $false
        $initial = ConvertFrom-LocronJson (Read-LocronPrivateFile ([IO.Path]::Combine($directory, 'request.json')))
        if ($initial.schema -cne 'locron.windows-operation/v1' -or $initial.kind -cne 'maintenance_prepare' -or
            $initial.operation_id -cne $Operation.ToString('D') -or $initial.sid -cne (Get-LocronSid)) { throw 'operation is not an owned prepared package transaction' }
        $package = Get-LocronPackage $Executable
        if ($package.binding.package_id -cne $initial.package.package_id -or $package.binding.source_id -cne $initial.package.source_id) { throw 'package source changed during maintenance' }
        $request = [ordered]@{
            schema = $initial.schema; operation_id = $initial.operation_id; kind = 'maintenance_complete'
            sid = $initial.sid; executable = $package.binding.executable; target = $package.binding.target; version = $package.binding.version
            archive_sha256 = $package.binding.archive_sha256; helper_sha256 = $initial.helper_sha256; caller_pid = $null
            no_service = $false; dashboard = $false; add_to_path = $false; state_root = $null; package = $package.binding
        }
        $stage = 'archive-' + [Guid]::NewGuid().ToString('D') + '.zip'
        Write-LocronPrivateFile ([IO.Path]::Combine($directory, $stage)) $package.archive
        $request.archive_file = $stage
        Invoke-LocronHelper $directory $request $null $true
    } else {
        if ($Operation -ne [Guid]::Empty) { throw 'Prepare and Remove create a fresh operation UUID' }
        $package = Get-LocronPackage $Executable
        $id = [Guid]::NewGuid()
        $directory = Get-LocronOperationDirectory $id
        Write-LocronPrivateFile ([IO.Path]::Combine($directory, 'archive.zip')) $package.archive
        $request = [ordered]@{
            schema = 'locron.windows-operation/v1'; operation_id = $id.ToString('D')
            kind = if ($Maintenance -ceq 'Prepare') { 'maintenance_prepare' } else { 'maintenance_remove' }
            sid = Get-LocronSid; executable = $package.binding.executable; target = $package.binding.target; version = $package.binding.version
            archive_sha256 = $package.binding.archive_sha256; helper_sha256 = ''; caller_pid = $null
            no_service = $false; dashboard = $false; add_to_path = $false; state_root = $null; package = $package.binding
        }
        Invoke-LocronHelper $directory $request $package.helper
    }
}

function Invoke-LocronUninstall([string]$Directory) {
    $receipt = Read-LocronReceipt $Directory
    $source = Read-LocronPrivateFile $receipt.executable
    if ((Get-LocronSha256 $source) -cne $receipt.binary_sha256) { throw 'owned executable changed; removal is refused' }
    Assert-LocronPe $source $receipt.target
    # Removal is offline. The verified retained executable supplies the helper,
    # and the Rust entrypoint repeats receipt/identity checks before any effect.
    $id = [Guid]::NewGuid()
    $operationDirectory = Get-LocronOperationDirectory $id
    $request = [ordered]@{
        schema = 'locron.windows-operation/v1'; operation_id = $id.ToString('D'); kind = 'uninstall'
        sid = Get-LocronSid; executable = $receipt.executable; target = $receipt.target; version = $receipt.version
        archive_sha256 = $receipt.archive_sha256; helper_sha256 = ''; caller_pid = $null
        no_service = $false; dashboard = $false; add_to_path = $false; state_root = $null
    }
    $status = Invoke-LocronHelper $operationDirectory $request $source
    foreach ($warning in $status.warnings) { Write-Warning $warning }
    $status
}

function Invoke-LocronRecovery([string[]]$ProvidedOptions = @()) {
    if ($Version -or $Executable -or $NoService -or $Dashboard -or $AddToPath -or
        @($ProvidedOptions | Where-Object { $_ -in @('Version', 'InstallDirectory', 'NoService', 'Dashboard', 'AddToPath', 'Executable', 'Maintenance') }).Count) {
        throw 'recovery accepts only the owned operation UUID'
    }
    $directory = Get-LocronOperationDirectory $Operation $false
    $initial = ConvertFrom-LocronJson (Read-LocronPrivateFile ([IO.Path]::Combine($directory, 'request.json')))
    if ($initial.schema -cne 'locron.windows-operation/v1' -or $initial.operation_id -cne $Operation.ToString('D') -or
        $initial.sid -cne (Get-LocronSid) -or $initial.kind -cnotin @('install', 'self_update', 'uninstall')) {
        throw 'operation is not an owned standalone transaction'
    }
    $request = [ordered]@{
        schema = $initial.schema; operation_id = $initial.operation_id; kind = 'recover'
        sid = $initial.sid; executable = $initial.executable; target = $initial.target; version = $initial.version
        archive_sha256 = $initial.archive_sha256; helper_sha256 = $initial.helper_sha256; caller_pid = $null
        no_service = $false; dashboard = $false; add_to_path = $false; state_root = $null
    }
    Invoke-LocronHelper $directory $request $null $true
}

function Invoke-LocronInstall([string[]]$ProvidedOptions = @()) {
    $productType = [Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Control\ProductOptions', 'ProductType', '')
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or
        [Environment]::OSVersion.Version.Build -lt 22000 -or $productType -cne 'WinNT') { throw 'locron requires Windows 11 workstation x64 or ARM64' }
    if ($ProvidedOptions -contains 'Operation' -and $Operation -eq [Guid]::Empty) { throw 'operation UUID must be nonzero' }
    if ($Maintenance) { return (Invoke-LocronMaintenance $ProvidedOptions) }
    if ($Operation -ne [Guid]::Empty) { return (Invoke-LocronRecovery $ProvidedOptions) }
    if ($Executable) { throw 'an exact package executable requires the explicit maintenance flow' }
    if ($NoService -and $Dashboard) { throw '-Dashboard conflicts with -NoService' }
    if ($AddToPath -and $InstallDirectory.Contains(';')) { throw 'a semicolon-bearing directory cannot be inserted in PATH unambiguously' }
    $destination = ConvertTo-LocronPath ([IO.Path]::Combine((ConvertTo-LocronPath $InstallDirectory), 'locron.exe'))
    $target = Get-LocronTarget
    $release = Get-LocronRelease $Version
    $name = "locron-$($release.tag_name)-$target.zip"
    $archive = Get-LocronAsset $release $name
    $sums = Get-LocronAsset $release 'SHA256SUMS.txt'
    $digest = Get-LocronSha256 $archive
    if ($digest -cne (Get-LocronChecksum $sums $name $release.tag_name)) { throw 'Windows archive checksum mismatch' }
    $files = Get-LocronArchiveFiles $archive $release.tag_name $target
    Assert-LocronPe $files['locron.exe'] $target
    $id = [Guid]::NewGuid()
    $directory = Get-LocronOperationDirectory $id
    Write-LocronPrivateFile ([IO.Path]::Combine($directory, 'archive.zip')) $archive
    Write-LocronPrivateFile ([IO.Path]::Combine($directory, 'installer.ps1')) (Get-LocronAsset $release 'install.ps1')
    Write-LocronPrivateFile ([IO.Path]::Combine($directory, 'uninstall.ps1')) (Get-LocronAsset $release 'uninstall.ps1')
    $request = [ordered]@{
        schema = 'locron.windows-operation/v1'; operation_id = $id.ToString('D'); kind = 'install'
        sid = Get-LocronSid; executable = $destination; target = $target; version = $release.tag_name.Substring(1)
        archive_sha256 = $digest; helper_sha256 = ''; caller_pid = $null
        no_service = [bool]$NoService; dashboard = [bool]$Dashboard; add_to_path = [bool]$AddToPath
        state_root = $env:LOCRON_STATE_DIR
    }
    $status = Invoke-LocronHelper $directory $request $files['locron.exe']
    foreach ($warning in $status.warnings) { Write-Warning $warning }
    $status
}

if ($MyInvocation.InvocationName -ne '.') {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    Invoke-LocronInstall @($PSBoundParameters.Keys)
}
