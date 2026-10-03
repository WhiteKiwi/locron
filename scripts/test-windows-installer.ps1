#requires -Version 5.1
<# Test-owned fixtures only: no network, live registrations, installs or PATH edits. #>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
. (Join-Path $repository 'install.ps1')

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw "fixture failed: $Message" }
}
function Assert-Refused([ScriptBlock]$Action, [string]$Message) {
    $refused = $false
    try { $null = & $Action } catch { $refused = $true }
    Assert-True $refused $Message
}
function Copy-FixtureMetadata($Source) {
    $copy = [ordered]@{}
    foreach ($entry in $Source.GetEnumerator()) { $copy[$entry.Key] = $entry.Value }
    $copy
}
function New-PeFixture([int]$Machine = 0x8664, [bool]$Signed = $false) {
    $bytes = [byte[]]::new(512)
    $bytes[0] = 0x4d; $bytes[1] = 0x5a
    [BitConverter]::GetBytes([uint32]128).CopyTo($bytes, 60)
    [BitConverter]::GetBytes([uint32]0x4550).CopyTo($bytes, 128)
    [BitConverter]::GetBytes([uint16]$Machine).CopyTo($bytes, 132)
    [BitConverter]::GetBytes([uint16]1).CopyTo($bytes, 134)
    [BitConverter]::GetBytes([uint16]240).CopyTo($bytes, 148)
    [BitConverter]::GetBytes([uint16]0x22).CopyTo($bytes, 150)
    [BitConverter]::GetBytes([uint16]0x20b).CopyTo($bytes, 152)
    [BitConverter]::GetBytes([uint32]16).CopyTo($bytes, 260)
    if ($Signed) { [BitConverter]::GetBytes([uint64]32).CopyTo($bytes, 296) }
    return ,$bytes
}
function New-ZipFixture([string]$Extra = '', [string]$WrongName = '') {
    Add-Type -AssemblyName System.IO.Compression
    $memory = [IO.MemoryStream]::new()
    $zip = [IO.Compression.ZipArchive]::new($memory, [IO.Compression.ZipArchiveMode]::Create, $true)
    try {
        foreach ($file in @('locron.exe', 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE')) {
            $name = "locron-v0.10.0-x86_64-pc-windows-msvc/$file"
            if ($file -ceq 'README.md' -and $WrongName) { $name = $WrongName }
            $entry = $zip.CreateEntry($name)
            $stream = $entry.Open()
            try {
                $bytes = if ($file -ceq 'locron.exe') { New-PeFixture } else { [Text.Encoding]::UTF8.GetBytes('fixture') }
                $stream.Write($bytes, 0, $bytes.Length)
            } finally { $stream.Dispose() }
        }
        if ($Extra) { $null = $zip.CreateEntry($Extra) }
    } finally { $zip.Dispose() }
    try { return ,$memory.ToArray() } finally { $memory.Dispose() }
}

$tag = 'v0.10.0'
$target = 'x86_64-pc-windows-msvc'
Assert-LocronPe (New-PeFixture) $target
Assert-LocronPe (New-PeFixture 0xaa64) 'aarch64-pc-windows-msvc'
Assert-Refused { Assert-LocronPe (New-PeFixture 0x14c) $target } '32-bit executable'
Assert-Refused { Assert-LocronPe (New-PeFixture 0xaa64) $target } 'wrong native architecture'
Assert-Refused { Assert-LocronPe (New-PeFixture 0x8664 $true) $target } 'signed initial release'
Assert-Refused { Assert-LocronPe ([byte[]]::new(12)) $target } 'truncated executable'
$truncatedSection = New-PeFixture
[BitConverter]::GetBytes([uint32]100).CopyTo($truncatedSection, 408)
[BitConverter]::GetBytes([uint32]500).CopyTo($truncatedSection, 412)
Assert-Refused { Assert-LocronPe $truncatedSection $target } 'section spans outside the executable'
$archive = New-ZipFixture
$files = Get-LocronArchiveFiles $archive $tag $target
Assert-True ($files.Count -eq 4) 'exact ZIP members'
$localConflict = [byte[]]$archive.Clone()
$localConflict[30] = [byte][char]'x'
Assert-Refused { Get-LocronArchiveFiles $localConflict $tag $target } 'conflicting local/central paths'
$descriptor = [byte[]]$archive.Clone()
$descriptor[6] = 8
Assert-Refused { Get-LocronArchiveFiles $descriptor $tag $target } 'data descriptors are refused'
$trailer = [byte[]]::new($archive.Length + 1)
$archive.CopyTo($trailer, 0)
Assert-Refused { Get-LocronArchiveFiles $trailer $tag $target } 'unexpected archive trailer'
$prefix = [byte[]]::new($archive.Length + 1)
$archive.CopyTo($prefix, 1)
Assert-Refused { Get-LocronArchiveFiles $prefix $tag $target } 'unexpected archive prefix'
foreach ($name in @('../outside', '/outside', 'C:/outside', 'dir\outside',
    'locron-v0.10.0-x86_64-pc-windows-msvc/README.MD')) {
    Assert-Refused { Get-LocronArchiveFiles (New-ZipFixture '' $name) $tag $target } 'unsafe member name'
}
Assert-Refused { Get-LocronArchiveFiles (New-ZipFixture 'extra') $tag $target } 'extra member'
Assert-Refused { Get-LocronArchiveFiles $archive 'v0.10.1' $target } 'wrong version directory'
Assert-Refused { Get-LocronArchiveFiles $archive $tag 'aarch64-pc-windows-msvc' } 'wrong target directory'
$hash = 'ab' * 32
$name = 'archive.zip'
$sums = [Text.Encoding]::UTF8.GetBytes("$hash  $name`n")
Assert-True ((Get-LocronChecksum $sums $name) -ceq $hash) 'exact checksum'
Assert-Refused { Get-LocronChecksum ([Text.Encoding]::UTF8.GetBytes("$hash  $name`n$hash  $name`n")) $name } 'duplicate checksum'
Assert-Refused { Get-LocronChecksum ([Text.Encoding]::UTF8.GetBytes("$hash  ../$name`n")) $name } 'checksum path traversal'
Assert-Refused { Get-LocronChecksum ([Text.Encoding]::UTF8.GetBytes("$hash  ..`n")) '..' } 'checksum dot component'
$inventory = @(Get-LocronPayloadInventory $tag)
Assert-True ($inventory.Count -eq 10) 'exact version-aware Windows payload inventory'
$completeSums = [Text.Encoding]::UTF8.GetBytes((($inventory | ForEach-Object { "$hash  $_" }) -join "`n") + "`n")
Assert-True ((Get-LocronChecksum $completeSums $inventory[8] $tag) -ceq $hash) 'complete exact payload checksum inventory'
$wrongCaseSums = [Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($completeSums).Replace('apple-darwin', 'APPLE-DARWIN'))
Assert-Refused { Get-LocronChecksum $wrongCaseSums $inventory[8] $tag } 'case-mutated unselected checksum payload'
Assert-Refused { Get-LocronHttps 'http://github.com/WhiteKiwi/locron' } 'HTTP transport'
Assert-Refused { Get-LocronHttps 'https://example.com/locron' } 'foreign transport'
foreach ($path in @('relative', '\\server\share\locron', 'C:\locron:stream', 'C:\locron.', 'C:\CON\locron')) {
    Assert-Refused { ConvertTo-LocronPath $path } "ambiguous path '$path'"
}
foreach ($path in @('C:\locron ', 'C:\locron.\', 'C:\locron.\child', 'C:\locron \child',
    'C:/locron./child', 'C:/locron /child', 'C:\safe\.\locron', 'C:\safe\..\locron',
    'C:/safe/./locron', 'C:/safe/../locron', 'C:\CON.txt\locron', 'C:/com1.txt/locron')) {
    Assert-Refused { ConvertTo-LocronPath $path } "ambiguous raw path '$path'"
}
$ordinaryPaths = [ordered]@{
    'C:\' = 'C:\'
    'C:\locron-fixture\file.name\folder name\.cache\locron' = 'C:\locron-fixture\file.name\folder name\.cache\locron'
    'C:/locron-fixture/folder name/.cache/locron/' = 'C:\locron-fixture\folder name\.cache\locron'
}
# Construct real Unicode independently of the script's legacy source encoding.
$unicodePath = 'C:\locron-fixture\' + [char]0x72b6 + [char]0x614b + '\locron'
$ordinaryPaths[$unicodePath] = $unicodePath
foreach ($case in $ordinaryPaths.GetEnumerator()) {
    Assert-True ((ConvertTo-LocronPath $case.Key) -ceq $case.Value) "ordinary path '$($case.Key)'"
}

$metadata = ConvertFrom-LocronJson ([Text.Encoding]::UTF8.GetBytes('{"schema":"test","nested":{"value":1}}'))
Assert-LocronFields $metadata @('schema', 'nested')
Assert-Refused { Assert-LocronFields $metadata @('schema') } 'unknown JSON field'
foreach ($json in @('{"schema":1,"schema":2}', '{"schema":1,"Schema":2}',
    '{"schema":1,"sch\u0065ma":2}', '{"files":{"a":1,"a":2}}', '{"schema":"unterminated}',
    '{"schema":1/* } { */,"schema":2}', '{schema:1,schema:2}', "{'schema':1,'schema':2}")) {
    Assert-Refused { ConvertFrom-LocronJson ([Text.Encoding]::UTF8.GetBytes($json)) } 'duplicate/malformed JSON metadata'
}
Assert-Refused { ConvertFrom-LocronJson ([byte[]]@(0xff)) } 'invalid metadata UTF-8'

$statusRequest = [ordered]@{ operation_id = 'e41c210d-c98d-47fb-9975-a5af66d01346'; sid = 'test-owned-SID'
    executable = 'C:\test-only\locron.exe'; version = '0.10.0'; kind = 'install' }
foreach ($kind in @('install', 'self_update', 'uninstall', 'maintenance_prepare', 'maintenance_complete', 'maintenance_remove')) {
    $statusRequest.kind = $kind
    $expectedPhase = switch -CaseSensitive ($kind) {
        'install' { 'completed' }; 'self_update' { 'completed' }; 'maintenance_complete' { 'completed' }
        'uninstall' { 'removed' }; 'maintenance_remove' { 'removed' }; 'maintenance_prepare' { 'prepared' }
    }
    $statusValue = [ordered]@{ schema = 'locron.windows-status/v1'; operation_id = $statusRequest.operation_id
        sid = $statusRequest.sid; executable = $statusRequest.executable; phase = $expectedPhase
        current_version = '0.10.0'; new_version = '0.10.0'; updated = ($kind -cin @('install', 'self_update'))
        prepared = ($kind -ceq 'maintenance_prepare'); warnings = @() }
    Assert-LocronStatus ([pscustomobject]$statusValue) $statusRequest
    foreach ($phase in @('completed', 'prepared', 'removed', 'accepted', 'failed', 'rolled_back')) {
        if ($phase -ceq $expectedPhase) { continue }
        $changed = Copy-FixtureMetadata $statusValue; $changed.phase = $phase
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } "wrong phase $kind/$phase"
    }
    foreach ($field in @('updated', 'prepared')) {
        $changed = Copy-FixtureMetadata $statusValue; $changed[$field] = -not $changed[$field]
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'wrong terminal result boolean'
        $changed[$field] = 'false'
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'coercible boolean string'
    }
    foreach ($field in @('phase', 'current_version', 'new_version', 'executable')) {
        $changed = Copy-FixtureMetadata $statusValue; $changed[$field] = 1
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'non-string result metadata'
    }
    foreach ($warnings in @('string-instead-of-array', @($null), @(1))) {
        $changed = Copy-FixtureMetadata $statusValue; $changed.warnings = $warnings
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'invalid warning inventory'
    }
    $changed = Copy-FixtureMetadata $statusValue; $changed.new_version = '0.10.1'
    Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'different selected release'
    $changed = Copy-FixtureMetadata $statusValue; $changed.current_version = '0.9.6'
    Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'prior release predates Windows support'
    if ($kind -cne 'install') {
        $changed = Copy-FixtureMetadata $statusValue; $changed.current_version = ''
        Assert-Refused { Assert-LocronStatus ([pscustomobject]$changed) $statusRequest } 'missing confirmed prior release'
    }
}
$statusRequest.kind = 'recover'
$recoveryStatus = [ordered]@{ schema = 'locron.windows-status/v1'; operation_id = $statusRequest.operation_id
    sid = $statusRequest.sid; executable = $statusRequest.executable; phase = 'removed'
    current_version = '0.10.0'; new_version = '0.10.0'; updated = $false; prepared = $false; warnings = @() }
Assert-LocronStatus ([pscustomobject]$recoveryStatus) $statusRequest 'uninstall'
Assert-Refused { Assert-LocronStatus ([pscustomobject]$recoveryStatus) $statusRequest } 'recovery without protected original kind'
Assert-Refused { Assert-LocronStatus ([pscustomobject]$recoveryStatus) $statusRequest 'install' } 'recovery cannot substitute removal for installation'
Assert-Refused { Assert-LocronStatus ([pscustomobject]$recoveryStatus) $statusRequest 'maintenance_remove' } 'recovery cannot adopt a package operation'

$pathDirectory = 'C:\test-only\locron'
foreach ($record in @(
    [ordered]@{ before = $null; before_kind = $null; after = $pathDirectory; after_kind = 'String' },
    [ordered]@{ before = ''; before_kind = 'String'; after = $pathDirectory; after_kind = 'String' },
    [ordered]@{ before = '%USERPROFILE%\bin'; before_kind = 'ExpandString'; after = "%USERPROFILE%\bin;$pathDirectory"; after_kind = 'ExpandString' },
    [ordered]@{ before = 'C:\other;'; before_kind = 'String'; after = "C:\other;$pathDirectory"; after_kind = 'String' }
)) { Assert-LocronUserPath ([pscustomobject]$record) $pathDirectory }
foreach ($record in @(
    [ordered]@{ before = $null; before_kind = $null; after = $pathDirectory; after_kind = 'ExpandString' },
    [ordered]@{ before = '%USERPROFILE%\bin'; before_kind = 'ExpandString'; after = $pathDirectory; after_kind = 'ExpandString' },
    [ordered]@{ before = ''; before_kind = 'ExpandString'; after = $pathDirectory; after_kind = 'String' },
    [ordered]@{ before = ''; before_kind = 'String'; after = $pathDirectory; after_kind = @('String') },
    [ordered]@{ before = 1; before_kind = 'String'; after = $pathDirectory; after_kind = 'String' },
    [ordered]@{ before = "a$([char]0)b"; before_kind = 'String'; after = "a$([char]0)b;$pathDirectory"; after_kind = 'String' }
)) { Assert-Refused { Assert-LocronUserPath ([pscustomobject]$record) $pathDirectory } 'PATH receipt derivation/type refusal' }
$percentDirectory = 'C:\literal%name\locron'
$percentRecord = [pscustomobject]@{ before = $null; before_kind = $null; after = $percentDirectory; after_kind = 'String' }
Assert-LocronUserPath $percentRecord $percentDirectory
$percentRecord.before = ''; $percentRecord.before_kind = 'ExpandString'; $percentRecord.after_kind = 'ExpandString'
Assert-Refused { Assert-LocronUserPath $percentRecord $percentDirectory } 'percent literal cannot use expandable PATH'
$semicolonDirectory = 'C:\a;b'
$semicolonRecord = [pscustomobject]@{ before = $null; before_kind = $null; after = $semicolonDirectory; after_kind = 'String' }
Assert-Refused { Assert-LocronUserPath $semicolonRecord $semicolonDirectory } 'semicolon cannot be one PATH field'

$packageLocation = 'C:\test-only\WinGet\Packages\fixture'
$packagePath = [IO.Path]::Combine($packageLocation, "locron-$tag-$target", 'locron.exe')
$packageEntry = [ordered]@{ key = 'test-only-registration'; package_id = 'WhiteKiwi.locron'; installer_type = 'portable'
    source_id = 'test-only-source'; version = '0.10.0'; install_location = $packageLocation; legacy_path = '' }
$selected = Select-LocronPackageBinding $packagePath $target @($packageEntry)
Assert-True ($selected.source_id -ceq 'test-only-source') 'exact portable package/source/path binding'
Assert-Refused { Select-LocronPackageBinding 'C:\test-only\unowned\locron.exe' $target @($packageEntry) } 'unregistered executable path'
Assert-Refused { Select-LocronPackageBinding $packagePath $target @($packageEntry, $packageEntry) } 'duplicate package registration'
$wrongPackage = Copy-FixtureMetadata $packageEntry
$wrongPackage.package_id = 'Other.Package'
Assert-Refused { Select-LocronPackageBinding $packagePath $target @($wrongPackage) } 'foreign package identifier'
$missingSource = Copy-FixtureMetadata $packageEntry
$missingSource.source_id = ''
Assert-Refused { Select-LocronPackageBinding $packagePath $target @($missingSource) } 'missing source identifier'
$oldPackage = Copy-FixtureMetadata $packageEntry
$oldPackage.legacy_path = [IO.Path]::Combine($packageLocation, 'locron.exe')
$null = Select-LocronPackageBinding $oldPackage.legacy_path $target @($oldPackage)
$escapedLegacy = Copy-FixtureMetadata $packageEntry
$escapedLegacy.legacy_path = 'C:\test-only\foreign\locron.exe'
Assert-Refused { Select-LocronPackageBinding $escapedLegacy.legacy_path $target @($escapedLegacy) } 'legacy target cannot escape its registered package location'

$sourceDescriptor = [Security.AccessControl.FileSecurity]::new()
$sourceDescriptor.SetOwner([Security.Principal.SecurityIdentifier]::new((Get-LocronSid)))
$sourceDescriptor.SetAccessRuleProtection($true, $false)
$sourceDescriptor.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.SecurityIdentifier]::new((Get-LocronSid)),
    [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow))
$sourceDescriptor.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.SecurityIdentifier]::new('S-1-1-0'),
    [Security.AccessControl.FileSystemRights]::ReadAndExecute, [Security.AccessControl.AccessControlType]::Allow))
Assert-LocronOwnedSourceDescriptor $sourceDescriptor
Assert-Refused { Assert-LocronDescriptor $sourceDescriptor $true $false } 'package read permissions do not authorize private helper storage'
$sourceDescriptor.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.SecurityIdentifier]::new('S-1-1-0'),
    [Security.AccessControl.FileSystemRights]::AppendData, [Security.AccessControl.AccessControlType]::Allow))
Assert-Refused { Assert-LocronOwnedSourceDescriptor $sourceDescriptor } 'foreign source-file mutation'

$ancestorDescriptor = [Security.AccessControl.DirectorySecurity]::new()
$ancestorDescriptor.SetOwner([Security.Principal.SecurityIdentifier]::new((Get-LocronSid)))
$ancestorDescriptor.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
    [Security.Principal.SecurityIdentifier]::new('S-1-3-0'),
    [Security.AccessControl.FileSystemRights]::FullControl,
    [Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit',
    [Security.AccessControl.PropagationFlags]::InheritOnly, [Security.AccessControl.AccessControlType]::Allow))
Assert-LocronDescriptor $ancestorDescriptor $false $true

$expectedRoot = [IO.Path]::GetFullPath([IO.Path]::Combine($env:TEMP, 'locron-distribution-fixture-' + [Guid]::NewGuid().ToString('D')))
$fixtureRoot = $null
$junction = $null
try {
    Write-Output 'locron-bootstrap-fixture stage=private-root-create-before'
    $fixtureRoot = New-LocronPrivateDirectory $expectedRoot
    Write-Output 'locron-bootstrap-fixture stage=private-root-create-after'
    Assert-True ($fixtureRoot -ceq $expectedRoot) 'private root identity'
    $path = [IO.Path]::Combine($fixtureRoot, ('Unicode spaces ' + [char]0xd55c + [char]0xae00 + '.txt'))
    $bytes = [Text.Encoding]::UTF8.GetBytes('protected fixture')
    Write-Output 'locron-bootstrap-fixture stage=private-leaf-write-before'
    Write-LocronPrivateFile $path $bytes
    Write-Output 'locron-bootstrap-fixture stage=private-leaf-write-after'
    Assert-True ((Get-LocronSha256 (Read-LocronPrivateFile $path)) -ceq (Get-LocronSha256 $bytes)) 'private read/write'
    Assert-Refused { Write-LocronPrivateFile $path $bytes } 'no replacement of existing files'
    $nativeTarget = Get-LocronTarget
    $receiptFiles = [ordered]@{}
    foreach ($file in @('locron.exe', 'README.md', 'LICENSE-MIT', 'LICENSE-APACHE', 'uninstall.ps1', '.locron-installer.ps1')) {
        $receiptFiles[$file] = $hash
    }
    $receipt = [ordered]@{ schema = 'locron.install/windows-v1'; sid = Get-LocronSid; channel = 'standalone'
        directory = $fixtureRoot; executable = [IO.Path]::Combine($fixtureRoot, 'locron.exe')
        target = $nativeTarget; version = '0.10.0'
        archive_url = "https://github.com/WhiteKiwi/locron/releases/download/v0.10.0/locron-v0.10.0-$nativeTarget.zip"
        archive_sha256 = $hash; binary_sha256 = $hash; files = $receiptFiles; user_path = $null }
    Write-LocronPrivateFile ([IO.Path]::Combine($fixtureRoot, '.locron-install-receipt-v1')) ([Text.Encoding]::UTF8.GetBytes(
        ($receipt | ConvertTo-Json -Depth 6 -Compress)))
    $owned = Read-LocronReceipt $fixtureRoot
    Assert-True ($owned.executable -ceq $receipt.executable) 'strict owned installation receipt'
    $foreignReceiptRoot = New-LocronPrivateDirectory ([IO.Path]::Combine($fixtureRoot, 'foreign-receipt'))
    Write-LocronPrivateFile ([IO.Path]::Combine($foreignReceiptRoot, '.locron-install-receipt-v1')) ([Text.Encoding]::UTF8.GetBytes(
        ($receipt | ConvertTo-Json -Depth 6 -Compress)))
    Assert-Refused { Read-LocronReceipt $foreignReceiptRoot } 'receipt cannot authorize a different directory'
    $broad = [Security.AccessControl.DirectorySecurity]::new()
    $broad.SetOwner([Security.Principal.SecurityIdentifier]::new((Get-LocronSid)))
    $broad.SetAccessRuleProtection($true, $false)
    $broad.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
        [Security.Principal.SecurityIdentifier]::new('S-1-1-0'),
        [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow))
    Assert-Refused { Assert-LocronDescriptor $broad $true $true } 'broad private DACL'
    $foreign = [Security.AccessControl.DirectorySecurity]::new()
    $foreign.SetOwner([Security.Principal.SecurityIdentifier]::new('S-1-5-18'))
    Assert-Refused { Assert-LocronDescriptor $foreign $true $true } 'foreign private ownership'
    $targetDirectory = New-LocronPrivateDirectory ([IO.Path]::Combine($fixtureRoot, 'target'))
    $junction = [IO.Path]::Combine($fixtureRoot, 'junction')
    $null = New-Item -ItemType Junction -Path $junction -Target $targetDirectory
    Assert-Refused { Assert-LocronDirectory $junction $true } 'reparse directory'
    [IO.DirectoryInfo]::new($junction).Delete()
    $junction = $null
} finally {
    if ($junction) { [IO.DirectoryInfo]::new($junction).Delete() }
    if ($fixtureRoot) {
        $resolved = [IO.Path]::GetFullPath($fixtureRoot)
        if ($resolved -cne $expectedRoot -or [IO.Path]::GetDirectoryName($resolved) -cne [IO.Path]::GetFullPath($env:TEMP).TrimEnd('\')) {
            throw 'refusing fixture cleanup outside its exact test-owned directory'
        }
        $null = Assert-LocronDirectory $resolved $true
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}
Write-Output 'Windows installer private-path/archive/source fixtures passed; no installation or live state changed.'
