$PSModuleAutoLoadingPreference = 'None'
$locronLibraries = @(
    @('Utility', $env:LOCRON_STOCK_UTILITY),
    @('Management', $env:LOCRON_STOCK_MANAGEMENT)
)
foreach ($locronLibrary in $locronLibraries) {
    if ($locronLibrary[0] -eq 'Management' -and [string]::IsNullOrEmpty($locronLibrary[1])) { continue }
    $locronPath = [string]$locronLibrary[1]
    if ([string]::IsNullOrEmpty($locronPath) -or -not [Text.RegularExpressions.Regex]::IsMatch($locronPath, '^[A-Za-z]:\\')) { throw 'invalid guarded Framework binary path' }
    $locronName = 'Microsoft.PowerShell.Commands.' + $locronLibrary[0]
    $locronExpected = $locronName + ', Version=3.0.0.0, Culture=neutral, PublicKeyToken=31bf3856ad364e35'
    $locronAssembly = [Reflection.Assembly]::LoadFrom($locronPath)
    $locronLoadedPath = $locronAssembly.Location
    if ($locronLoadedPath.StartsWith('\\?\')) { $locronLoadedPath = $locronLoadedPath.Substring(4) }
    if ($locronAssembly.FullName -cne $locronExpected -or -not [string]::Equals([IO.Path]::GetFullPath($locronLoadedPath), [IO.Path]::GetFullPath($locronPath), [StringComparison]::OrdinalIgnoreCase)) { throw 'stock binary binding mismatch' }
    $locronImported = @(Microsoft.PowerShell.Core\Import-Module -Assembly $locronAssembly -PassThru -Function @() -Alias @())
    if ($locronImported.Count -ne 1 -or $locronImported[0].ModuleType -ne [System.Management.Automation.ModuleType]::Binary) { throw 'stock binary module mismatch' }
    if ($locronLibrary[0] -eq 'Utility') {
        $locronFromJson = $locronImported[0].ExportedCmdlets['ConvertFrom-Json']
        $locronToJson = $locronImported[0].ExportedCmdlets['ConvertTo-Json']
        foreach ($locronCommand in @($locronFromJson, $locronToJson)) {
            if ($locronCommand -isnot [System.Management.Automation.CmdletInfo] -or $locronCommand.ImplementingType.Assembly -ne $locronAssembly) { throw 'stock JSON command binding mismatch' }
        }
        if ($locronFromJson.ImplementingType.FullName -cne 'Microsoft.PowerShell.Commands.ConvertFromJsonCommand' -or $locronToJson.ImplementingType.FullName -cne 'Microsoft.PowerShell.Commands.ConvertToJsonCommand') { throw 'stock JSON command type mismatch' }
    }
}
if ($null -eq $locronFromJson -or $null -eq $locronToJson) { throw 'stock JSON commands unavailable' }
