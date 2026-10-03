$locronPolicyType = [System.Management.Automation.PSObject].Assembly.GetType('System.Management.Automation.Internal.SecuritySupport', $true)
$locronPolicyMethod = $locronPolicyType.GetMethod('GetExecutionPolicy', [Reflection.BindingFlags]'NonPublic,Static', $null, [Type[]]@([string]), $null)
if ($null -eq $locronPolicyMethod) { throw 'stock policy reflection is unavailable' }
$locronActualPolicy = $locronPolicyMethod.Invoke($null, [object[]]@('Microsoft.PowerShell'))
if ($null -eq $locronActualPolicy -or -not $locronActualPolicy.GetType().IsEnum -or $locronActualPolicy.GetType().FullName -cne 'Microsoft.PowerShell.ExecutionPolicy' -or $locronActualPolicy.ToString() -cne 'Restricted') {
    throw 'stock child effective policy is not Restricted'
}
