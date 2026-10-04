# Explicit disposable-host prerequisite; this script never qualifies private-state or IPC access.
[CmdletBinding()]
param([switch]$HostedPrerequisite)

$controllerEntered = [Diagnostics.Stopwatch]::GetTimestamp()
$ErrorActionPreference = 'Stop'
if (-not $HostedPrerequisite -or $env:GITHUB_ACTIONS -cne 'true' -or
    $env:LOCRON_PREREQUISITE_RUNNER_ENVIRONMENT -cne 'github-hosted' -or
    $env:LOCRON_PREREQUISITE_MODE -cne 'two-user-v1') {
    [Environment]::Exit(1)
}

# Only this retained runspace may own account, credential, stream or filesystem operations.
# Driver code below consumes only its bounded sanitized result and deadline scalars.
# BEGIN fixed-prerequisite-diagnostics driver_helpers
function Get-FixedFailureKind($exception) {
    $specific='unknown'; $wrapper='unknown'; $node=$exception
    for ($index=0; $index -lt 5; $index++) {
        if ($node -isnot [Exception]) {break}
        if ($node -is [UnauthorizedAccessException]) {$specific='unauthorized'}
        elseif ($node -is [Security.SecurityException]) {$specific='security'}
        elseif ($node -is [IO.IOException]) {$specific='io'}
        elseif ($node -is [ComponentModel.Win32Exception]) {$specific='win32'}
        elseif ($node -is [ArgumentException]) {$specific='argument'}
        elseif ($node -is [InvalidOperationException]) {$specific='invalid_operation'}
        elseif ($node -is [TimeoutException]) {$specific='timeout'}
        elseif ($node -is [InvalidCastException]) {$specific='invalid_cast'}
        elseif ($node -is [OverflowException]) {$specific='overflow'}
        elseif ($node -is [Management.Automation.PipelineStoppedException]) {$wrapper='pipeline'}
        elseif ($node -is [Management.Automation.MethodInvocationException]) {$wrapper='method_invocation'}
        elseif ($node -is [Management.Automation.RuntimeException]) {$wrapper='runtime'}
        elseif ($node -is [Management.Automation.ParentContainsErrorRecordException]) {$wrapper='parent_error_record'}
        if ($index -eq 4) {break}
        $node=$node.InnerException
    }
    if ($specific -cne 'unknown') {return $specific}
    return $wrapper
}

function Fixed-DiagnosticValue($value,[string[]]$allowed) {
    if ($value -is [string] -and $value -cin $allowed) {return $value}
    return 'unknown'
}

function Write-FixedRefusalDiagnostic($branch,$driverKind=$null) {
    $checkpoints=@('entry','metadata','stock_preflight','stock_utility_open','stock_utility_import','stock_accounts_open','stock_accounts_import','stock_exports','runner_identity','os_metadata','checkout_bootstrap','image_open','image_hash','anchor_bootstrap','guardian_start','guard_acquire_anchor','guard_create_job','account_a_create','account_b_create','guard_create_controls','image_copy','guard_hold_image','actor_start','actor_read','cleanup_accounts','cleanup_release_image','cleanup_remove_image','cleanup_remove_markers','cleanup_release_controls','cleanup_remove_controls','cleanup_release_job','cleanup_remove_job','cleanup_finish_guard','cleanup_guard_eof','cleanup_dispose','evidence_publish','confirmed')
    $branches=@('shared_unknown','phase_expired_pending','phase_expired_completed','endinvoke_exception','result_rejected','phase_expired_after_dispose','phase_expired_after_success_print')
    $categories=@('preflight','account','credential_start','token','protocol','containment','deadline','cleanup','guard_setup','native_owner_unknown')
    $kinds=@('unauthorized','security','io','win32','argument','invalid_operation','timeout','method_invocation','runtime','pipeline','invalid_cast','overflow','parent_error_record','unknown')
    $evidenceStates=@('not_attempted','attempting','written','refused')
    # BEGIN fixed-prerequisite-substage renderer_closed_set
    $substages=@('entry','path_validation','attributes','directory_security','security_owner','security_sddl','security_raw_acl','security_rules','owner_check','owner_untrusted','acl_presence_check','null_acl','ace_shape_check','ace_shape','foreign_mutation_check','foreign_mutation','file_open','file_security','evidence_serialize','evidence_parent','evidence_directory_create','evidence_directory_security','evidence_identity_write','evidence_receipt_write')
    # END fixed-prerequisite-substage renderer_closed_set
    $checkpoint=Fixed-DiagnosticValue $shared['checkpoint'] $checkpoints
    $failureCheckpoint=Fixed-DiagnosticValue $shared['failure_checkpoint'] $checkpoints
    $branch=Fixed-DiagnosticValue $branch $branches
    $category=Fixed-DiagnosticValue $shared['failure_category'] $categories
    $kind=Fixed-DiagnosticValue $shared['failure_kind'] $kinds
    if ($null -ne $driverKind) {$kind=Fixed-DiagnosticValue $driverKind $kinds}
    $evidence=Fixed-DiagnosticValue $shared['failure_evidence'] $evidenceStates
    $evidenceKind=Fixed-DiagnosticValue $shared['evidence_kind'] $kinds
    # BEGIN fixed-prerequisite-substage renderer_validation
    $substage=Fixed-DiagnosticValue $shared['substage'] $substages
    $failureSubstage=Fixed-DiagnosticValue $shared['failure_substage'] $substages
    $evidenceSubstage=Fixed-DiagnosticValue $shared['evidence_substage'] $substages
    # END fixed-prerequisite-substage renderer_validation
    $line='windows-user-prerequisite failed: native_owner_unknown checkpoint='+$checkpoint+
        ' failure_checkpoint='+$failureCheckpoint+' branch='+$branch+' category='+$category+
        ' kind='+$kind+' evidence='+$evidence+' evidence_kind='+$evidenceKind
    # BEGIN fixed-prerequisite-substage renderer_append
    $line+=' substage='+$substage+' failure_substage='+$failureSubstage+' evidence_substage='+$evidenceSubstage
    # END fixed-prerequisite-substage renderer_append
    # Every selected scalar is fixed ASCII; allow two bytes for the newline.
    if ($line.Length -gt 510) {
        $line='windows-user-prerequisite failed: native_owner_unknown checkpoint=unknown failure_checkpoint=unknown branch=unknown category=unknown kind=unknown evidence=unknown evidence_kind=unknown'
        # BEGIN fixed-prerequisite-substage renderer_fallback
        $line+=' substage=unknown failure_substage=unknown evidence_substage=unknown'
        # END fixed-prerequisite-substage renderer_fallback
    }
    [Console]::Error.WriteLine($line)
}

# END fixed-prerequisite-diagnostics driver_helpers
$nativeOwner = {
    param($shared, $metadata)
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    $VerbosePreference = 'SilentlyContinue'
    $DebugPreference = 'SilentlyContinue'
    $WarningPreference = 'SilentlyContinue'
    $InformationPreference = 'SilentlyContinue'
    Set-StrictMode -Version 2.0

    # BEGIN fixed-prerequisite-diagnostics owner_classifier
    function Get-FixedFailureKind($exception) {
        $specific='unknown'; $wrapper='unknown'; $node=$exception
        for ($index=0; $index -lt 5; $index++) {
            if ($node -isnot [Exception]) {break}
            if ($node -is [UnauthorizedAccessException]) {$specific='unauthorized'}
            elseif ($node -is [Security.SecurityException]) {$specific='security'}
            elseif ($node -is [IO.IOException]) {$specific='io'}
            elseif ($node -is [ComponentModel.Win32Exception]) {$specific='win32'}
            elseif ($node -is [ArgumentException]) {$specific='argument'}
            elseif ($node -is [InvalidOperationException]) {$specific='invalid_operation'}
            elseif ($node -is [TimeoutException]) {$specific='timeout'}
            elseif ($node -is [InvalidCastException]) {$specific='invalid_cast'}
            elseif ($node -is [OverflowException]) {$specific='overflow'}
            elseif ($node -is [Management.Automation.PipelineStoppedException]) {$wrapper='pipeline'}
            elseif ($node -is [Management.Automation.MethodInvocationException]) {$wrapper='method_invocation'}
            elseif ($node -is [Management.Automation.RuntimeException]) {$wrapper='runtime'}
            elseif ($node -is [Management.Automation.ParentContainsErrorRecordException]) {$wrapper='parent_error_record'}
            if ($index -eq 4) {break}
            $node=$node.InnerException
        }
        if ($specific -cne 'unknown') {return $specific}
        return $wrapper
    }

    # END fixed-prerequisite-diagnostics owner_classifier
    function Check-Deadline {
        if ($shared['expired'] -or [Diagnostics.Stopwatch]::GetTimestamp() -ge
            [long]$shared['phase_deadline']) { throw 'deadline' }
    }

    function Begin-Phase([int]$seconds, [long]$born) {
        Check-Deadline
        $candidate = $born + ([long]$seconds * [Diagnostics.Stopwatch]::Frequency)
        $shared['phase_deadline'] = [Math]::Min([long]$shared['controller_deadline'], $candidate)
        Check-Deadline
    }

    function Native-Call([scriptblock]$operation) {
        Check-Deadline
        $value = & $operation
        # Transfer returned native resources before the post-call gate. A late FileStream,
        # token or pending Task stays in this owner even when success is refused.
        if ($value -is [IDisposable]) {$shared['resources'].Add($value)}
        Check-Deadline
        return $value
    }

    function Keep-Unknown {
        $shared['unknown'] = $true
        # Retain every exact owner reference until this failed hosted process/VM is disposed.
        # Neither parking nor process disposal is an account/tree cleanup confirmation.
        while ($true) { [Threading.Thread]::Sleep(50) }
    }

    function New-Password {
        $password = Native-Call {[Security.SecureString]::new()}
        $random = Native-Call {[Security.Cryptography.RandomNumberGenerator]::Create()}
        $buffer = [byte[]]::new(1)
        # Guaranteed categories. Account names contain c; the password alphabet excludes c/C.
        $classes = @('GHJKLMNPQRSTUVWXYZ', 'ghijkmnopqrstuvwxyz', '23456789', '!%+,-.:=@^_~')
        $alphabet = $classes -join ''
        try {
            for ($index = 0; $index -lt 32; $index++) {
                $choices = $alphabet
                if ($index -lt 4) { $choices = $classes[$index] }
                $ceiling = 256 - (256 % $choices.Length)
                do {
                    Check-Deadline
                    $random.GetBytes($buffer)
                    Check-Deadline
                    $sample = [int]$buffer[0]
                    [Array]::Clear($buffer, 0, $buffer.Length)
                } while ($sample -ge $ceiling)
                $null=Native-Call {$password.AppendChar($choices[$sample % $choices.Length])}
            }
            $null=Native-Call {$password.MakeReadOnly()}
            return $password
        } finally {
            [Array]::Clear($buffer, 0, $buffer.Length)
            # A late call retains the RNG/SecureString in the one native owner.
            $null=Native-Call {$random.Dispose()}
        }
    }

    function Quote-NativeArgument([string]$value) {
        # Framework ProcessStartInfo has no ArgumentList; use the Windows native argv rules.
        $quoted = [Text.StringBuilder]::new('"')
        $slashes = 0
        foreach ($character in $value.ToCharArray()) {
            if ($character -eq [char]92) { $slashes++; continue }
            if ($character -eq [char]34) {
                $null = $quoted.Append([char]92, (2 * $slashes) + 1).Append([char]34)
            } else {
                $null = $quoted.Append([char]92, $slashes).Append($character)
            }
            $slashes = 0
        }
        $null = $quoted.Append([char]92, 2 * $slashes).Append([char]34)
        return $quoted.ToString()
    }

    function Bounded-Ascii([string]$value, [int]$limit) {
        if ([string]::IsNullOrEmpty($value) -or $value.Length -gt $limit) { throw 'preflight' }
        foreach ($character in $value.ToCharArray()) {
            if ([int]$character -lt 32 -or [int]$character -gt 126) { throw 'preflight' }
        }
        return $value
    }

    function Check-Metadata {
        if (-not [regex]::IsMatch($metadata.source_sha, '\A[0-9a-f]{40}\z') -or
            -not [regex]::IsMatch($metadata.base_sha, '\A[0-9a-f]{40}\z') -or
            -not [regex]::IsMatch($metadata.run_id, '\A[1-9][0-9]{0,19}\z') -or
            -not [regex]::IsMatch($metadata.run_attempt, '\A[1-9][0-9]{0,19}\z') -or
            $metadata.target -cnotin @('x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc')) {
            throw 'preflight'
        }
        $null = Bounded-Ascii $metadata.workflow 96
        $null = Bounded-Ascii $metadata.image 96
        Check-Deadline
    }

    function New-Channel($stream, [string]$kind) {
        $buffer = [byte[]]::new(512)
        $channel = @{
            stream = $stream; kind = $kind; buffer = $buffer; task = $null
            pending = [Collections.Generic.List[byte]]::new()
            frames = [Collections.Generic.List[object]]::new()
            total = 0; eof = $false
        }
        $shared['resources'].Add($channel)
        $channel.task = Native-Call { $stream.ReadAsync($buffer, 0, $buffer.Length) }
        return $channel
    }

    function Decode-Frame([byte[]]$bytes, [string]$kind) {
        $frameLimit=1024
        if ($kind -ceq 'guard') {$frameLimit=4096}
        if ($bytes.Length -lt 2 -or $bytes.Length -gt $frameLimit -or $bytes[-1] -ne 10) {
            throw 'protocol'
        }
        $decoder = [Text.UTF8Encoding]::new($false, $true)
        $text = $decoder.GetString($bytes, 0, $bytes.Length - 1)
        Check-Deadline
        # Full anchored, ordered grammar rejects unknown/duplicate keys BEFORE JSON decoding.
        # The only strings accepted from private actors are fixed labels and canonical SIDs.
        $sid = 'S-1-(?:0|[1-9][0-9]{0,19})(?:-(?:0|[1-9][0-9]{0,9})){1,15}'
        $pidPattern = '[1-9][0-9]{0,9}'
        $boolean = '(?:true|false)'
        if ($kind -ceq 'identity') {
            $pattern = '\A\{"schema":"locron\.windows-user-identity/v1","actor":"[AB]","role":"(?:inner|grandchild)","pid":' + $pidPattern + ',"sid":"' + $sid + '","probe_sid":"' + $sid + '","users_enabled":' + $boolean + ',"administrators_enabled":' + $boolean + ',"forbidden_builtin_group_present":' + $boolean + '\}\z'
        } elseif ($kind -ceq 'owner') {
            $prefix = '\A\{"schema":"locron\.windows-user-owner/v1","actor":"[AB]","phase":"'
            $started = 'owner_started","pid":' + $pidPattern + ',"sid":"' + $sid + '","root_pid":' + $pidPattern
            $live = 'root_reaped_tree_live_negative_confirm","root_exit_zero":' + $boolean + ',"tree_empty":' + $boolean + ',"negative_timed_out":' + $boolean
            $cleanup = 'cleanup_confirmed","root_exit_zero":' + $boolean + ',"tree_empty":' + $boolean
            $pattern = $prefix + '(?:' + $started + '|' + $live + '|' + $cleanup + ')\}\z'
        } elseif ($kind -ceq 'guard') {
            $prefix = '\A\{"schema":"locron\.windows-user-guard/v1","nonce":"[0-9a-f]{32}","seq":[0-7],"op":"'
            $anchor = 'anchor_held","ok":true,"payload":\{"pid":' + $pidPattern + ',"runner_sid":"' + $sid + '","source_sha256":"[0-9a-f]{64}"\}'
            $job = 'job_held","ok":true,"payload":\{"created":true\}'
            $controls = 'controls_held","ok":true,"payload":\{"created":2\}'
            $image = 'image_held","ok":true,"payload":\{"sha256":"[0-9a-f]{64}"\}'
            $release = '(?:image_released|job_released|all_released)","ok":true,"payload":\{"released":true\}'
            $controlRelease = 'controls_released","ok":true,"payload":\{"released":2\}'
            $failed = 'failed","ok":false,"payload":\{"failure":"(?:preflight|account|credential_start|token|protocol|containment|deadline|cleanup|native_owner_unknown|guard_setup)"\}'
            $pattern = $prefix + '(?:' + $anchor + '|' + $job + '|' + $controls + '|' + $image + '|' + $release + '|' + $controlRelease + '|' + $failed + ')\}\z'
        } else { throw 'protocol' }
        $regex = [Text.RegularExpressions.Regex]::new($pattern,
            [Text.RegularExpressions.RegexOptions]::CultureInvariant,
            [TimeSpan]::FromMilliseconds(20))
        if (-not $regex.IsMatch($text)) { throw 'protocol' }
        Check-Deadline
        $frame = $text | & $script:fromJson
        Check-Deadline
        if ($kind -ceq 'identity' -or ($kind -ceq 'owner' -and $frame.phase -ceq 'owner_started')) {
            if ([string]$frame.sid -cne ([string]$frame.sid).Trim() -or
                ([string]$frame.sid).Length -gt 256 -or [uint64]$frame.pid -gt [uint32]::MaxValue) {
                throw 'protocol'
            }
        }
        if ($kind -ceq 'identity' -and ([string]$frame.probe_sid).Length -gt 256) {
            throw 'protocol'
        }
        if ($kind -ceq 'owner' -and $frame.phase -ceq 'owner_started' -and
            [uint64]$frame.root_pid -gt [uint32]::MaxValue) { throw 'protocol' }
        return $frame
    }

    function Pump-Channel($channel) {
        Check-Deadline
        if ($channel.eof -or -not $channel.task.IsCompleted) { return }
        $read = $channel.task.GetAwaiter().GetResult()
        Check-Deadline
        if ($read -lt 0 -or $read -gt $channel.buffer.Length) { throw 'protocol' }
        if ($read -eq 0) {
            if ($channel.pending.Count -ne 0) { throw 'protocol' }
            $channel.eof = $true
            $channel.task = $null
            return
        }
        $channel.total += $read
        $byteLimit=4096; $frameLimit=1024
        if ($channel.kind -ceq 'guard') {$byteLimit=32768; $frameLimit=4096}
        if ($channel.kind -ceq 'empty' -or $channel.total -gt $byteLimit) { throw 'protocol' }
        for ($index = 0; $index -lt $read; $index++) {
            $channel.pending.Add($channel.buffer[$index])
            if ($channel.pending.Count -gt $frameLimit) { throw 'protocol' }
            if ($channel.buffer[$index] -eq 10) {
                $frame = Decode-Frame $channel.pending.ToArray() $channel.kind
                $channel.frames.Add($frame)
                $channel.pending.Clear()
                $limit = 3
                if ($channel.kind -ceq 'identity') { $limit = 2 }
                if ($channel.kind -ceq 'guard') { $limit = 8 }
                if ($channel.frames.Count -gt $limit) { throw 'protocol' }
            }
        }
        Check-Deadline
        $channel.task = Native-Call {
            $channel.stream.ReadAsync($channel.buffer, 0, $channel.buffer.Length)
        }
    }

    function Read-Actor($process, $account) {
        $stdout = New-Channel (Native-Call { $process.StandardOutput.BaseStream }) 'identity'
        $stderr = New-Channel (Native-Call { $process.StandardError.BaseStream }) 'owner'
        $actorProcessId = Native-Call { $process.Id }
        do {
            Pump-Channel $stdout
            Pump-Channel $stderr
            if (($stdout.total + $stderr.total) -gt 8192) { throw 'protocol' }
            $exited = Native-Call { $process.HasExited }
            Check-Deadline
            if (-not ($exited -and $stdout.eof -and $stderr.eof)) {
                [Threading.Thread]::Sleep(5)
            }
        } while (-not ($exited -and $stdout.eof -and $stderr.eof))
        $exitCode = Native-Call { $process.ExitCode }
        if ($exitCode -ne 0 -or $stdout.frames.Count -ne 2 -or $stderr.frames.Count -ne 3) {
            throw 'containment'
        }
        $inner = $stdout.frames[0]
        $grandchild = $stdout.frames[1]
        $started = $stderr.frames[0]
        $live = $stderr.frames[1]
        $cleanup = $stderr.frames[2]
        foreach ($frame in @($inner, $grandchild, $started, $live, $cleanup)) {
            if ($frame.actor -cne $account.label) { throw 'protocol' }
        }
        if ($inner.role -cne 'inner' -or $grandchild.role -cne 'grandchild' -or
            $started.phase -cne 'owner_started' -or
            $live.phase -cne 'root_reaped_tree_live_negative_confirm' -or
            $cleanup.phase -cne 'cleanup_confirmed' -or
            [uint64]$started.pid -ne [uint64]$actorProcessId -or
            [uint64]$started.root_pid -ne [uint64]$inner.pid -or
            [uint64]$inner.pid -eq [uint64]$grandchild.pid -or
            [uint64]$inner.pid -eq [uint64]$actorProcessId -or [uint64]$grandchild.pid -eq [uint64]$actorProcessId) {
            throw 'protocol'
        }
        foreach ($frame in @($inner, $grandchild)) {
            if ($frame.sid -cne $account.sid.Value -or $frame.probe_sid -cne $frame.sid -or
                -not $frame.users_enabled -or $frame.administrators_enabled -or
                $frame.forbidden_builtin_group_present) { throw 'token' }
        }
        if ($started.sid -cne $account.sid.Value) { throw 'token' }
        if (-not $live.root_exit_zero -or $live.tree_empty -or -not $live.negative_timed_out -or
            -not $cleanup.root_exit_zero -or -not $cleanup.tree_empty) { throw 'containment' }
        Check-Deadline
        return [ordered]@{
            label = $account.label; primary_tokens_match = $true; users_only = $true
            root_exit_zero = $true; tree_live_negative_confirmed = $true
            tree_empty_confirmed = $true; stdout_eof = $stdout.eof
            stderr_eof = $stderr.eof; outer_exit_zero = ($exitCode -eq 0)
        }
    }

    function Json-Public($value) {
        # Fixed sanitized evidence only; no private frame reaches this fallback serializer.
        if ($null -eq $value) {return 'null'}
        if ($value -is [bool]) {if ($value) {return 'true'} else {return 'false'}}
        if ($value -is [string]) {
            foreach ($character in $value.ToCharArray()) {
                if ([int]$character -lt 32 -or [int]$character -gt 126) {throw 'preflight'}
            }
            return '"' + $value.Replace('\','\\').Replace('"','\"') + '"'
        }
        if ($value -is [Collections.IDictionary]) {
            $parts=[Collections.Generic.List[string]]::new()
            foreach ($key in $value.Keys) {$parts.Add((Json-Public ([string]$key))+':'+(Json-Public $value[$key]))}
            return '{'+($parts -join ',')+'}'
        }
        if ($value -is [Array]) {
            $parts=[Collections.Generic.List[string]]::new()
            foreach ($item in $value) {$parts.Add((Json-Public $item))}
            return '['+($parts -join ',')+']'
        }
        if ($value -is [int] -or $value -is [long] -or $value -is [uint32]) {
            if ($value -lt 0) {throw 'preflight'}
            return $value.ToString([Globalization.CultureInfo]::InvariantCulture)
        }
        throw 'protocol'
    }

    function Local-Path([string]$value) {
        # BEGIN fixed-prerequisite-substage path_validation
        $shared['substage']='path_validation'
        # END fixed-prerequisite-substage path_validation
        if ([string]::IsNullOrEmpty($value) -or $value.Length -gt 512 -or
            -not [regex]::IsMatch($value,'\A[A-Za-z]:\\') -or
            $value.Contains('/') -or $value.Contains('~')) {throw 'guard_setup'}
        $full=[IO.Path]::GetFullPath($value)
        if ($full -cne $value) {throw 'guard_setup'}
        foreach ($component in $value.Substring(3).Split([char]92)) {
            if ($component.Length -eq 0 -or $component.EndsWith('.') -or $component.EndsWith(' ') -or
                $component -match '[<>:"|?*\x00-\x1f]' -or
                $component -match '\A(?:CON|PRN|AUX|NUL|COM[0-9]|LPT[0-9])(?:\.|$)') {throw 'guard_setup'}
        }
        return $full
    }

    function Check-Attributes([string]$path,[bool]$directory) {
        # BEGIN fixed-prerequisite-substage attributes
        $shared['substage']='attributes'
        # END fixed-prerequisite-substage attributes
        $attributes=Native-Call {[IO.File]::GetAttributes($path)}
        if (($attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 -or
            ((($attributes -band [IO.FileAttributes]::Directory) -ne 0) -ne $directory)) {throw 'guard_setup'}
    }

    function Require-Absent([string]$path,[string]$failure='cleanup') {
        # Exists suppresses access/I/O errors. Only a genuine missing leaf under the
        # still-held known parent proves absence; missing/unknown ancestry refuses.
        $missing=$false
        try {$null=Native-Call {[IO.File]::GetAttributes($path)}} catch {
            $exception=$_.Exception
            while ($null -ne $exception.InnerException) {$exception=$exception.InnerException}
            if (($exception.HResult -band 65535) -ne 2) {throw $failure}
            $missing=$true
        }
        Check-Deadline
        if (-not $missing) {throw $failure}
    }

    function Check-FixedSecurity($security,[bool]$directory) {
        $trusted=@('S-1-5-18','S-1-5-32-544','S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464')
        # BEGIN fixed-prerequisite-substage fixed_security_owner
        $shared['substage']='security_owner'
        # END fixed-prerequisite-substage fixed_security_owner
        $owner=Native-Call {$security.GetOwner([Security.Principal.SecurityIdentifier]).Value}
        # BEGIN fixed-prerequisite-substage owner_predicate
        $shared['substage']='owner_check'
        # END fixed-prerequisite-substage owner_predicate
        if ($owner -cnotin $trusted) {
            # BEGIN fixed-prerequisite-substage owner_rejection
            $shared['substage']='owner_untrusted'
            # END fixed-prerequisite-substage owner_rejection
            throw 'guard_setup'
        }
        # BEGIN fixed-prerequisite-substage fixed_security_sddl
        $shared['substage']='security_sddl'
        # END fixed-prerequisite-substage fixed_security_sddl
        $sddl=Native-Call {$security.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)}
        # BEGIN fixed-prerequisite-substage fixed_security_raw_acl
        $shared['substage']='security_raw_acl'
        # END fixed-prerequisite-substage fixed_security_raw_acl
        $raw=Native-Call {[Security.AccessControl.RawSecurityDescriptor]::new($sddl)}
        # BEGIN fixed-prerequisite-substage acl_presence_predicate
        $shared['substage']='acl_presence_check'
        # END fixed-prerequisite-substage acl_presence_predicate
        if ($null -eq $raw.DiscretionaryAcl) {
            # BEGIN fixed-prerequisite-substage acl_presence_rejection
            $shared['substage']='null_acl'
            # END fixed-prerequisite-substage acl_presence_rejection
            throw 'guard_setup'
        }
        $mask=[long]1343029590
        if ($directory) {$mask=$mask -band (-bnot [long]6)}
        # BEGIN fixed-prerequisite-substage fixed_security_rules
        $shared['substage']='security_rules'
        # END fixed-prerequisite-substage fixed_security_rules
        foreach ($ace in $raw.DiscretionaryAcl) {
            # BEGIN fixed-prerequisite-substage ace_shape_predicate
            $shared['substage']='ace_shape_check'
            # END fixed-prerequisite-substage ace_shape_predicate
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback -or
                $ace.AceQualifier -notin @([Security.AccessControl.AceQualifier]::AccessAllowed,[Security.AccessControl.AceQualifier]::AccessDenied)) {
                # BEGIN fixed-prerequisite-substage ace_shape_rejection
                $shared['substage']='ace_shape'
                # END fixed-prerequisite-substage ace_shape_rejection
                throw 'guard_setup'
            }
            # BEGIN fixed-prerequisite-substage foreign_mutation_predicate
            $shared['substage']='foreign_mutation_check'
            # END fixed-prerequisite-substage foreign_mutation_predicate
            if ($ace.AceQualifier -eq [Security.AccessControl.AceQualifier]::AccessAllowed -and
                ($ace.AceFlags -band [Security.AccessControl.AceFlags]::InheritOnly) -eq 0 -and
                $ace.SecurityIdentifier.Value -cnotin $trusted -and
                ([long]$ace.AccessMask -band $mask) -ne 0) {
                # BEGIN fixed-prerequisite-substage foreign_mutation_rejection
                $shared['substage']='foreign_mutation'
                # END fixed-prerequisite-substage foreign_mutation_rejection
                throw 'guard_setup'
            }
        }
        Check-Deadline
    }

    function Bootstrap-Chain([string]$path) {
        $null=Local-Path $path
        $current=$path.Substring(0,3)
        $parts=@($path.Substring(3).Split([char]92))
        for ($index=-1;$index -lt $parts.Count;$index++) {
            if ($index -ge 0) {$current=[IO.Path]::Combine($current,$parts[$index])}
            Check-Attributes $current $true
            # BEGIN fixed-prerequisite-substage directory_security
            $shared['substage']='directory_security'
            # END fixed-prerequisite-substage directory_security
            $security=Native-Call {[IO.DirectoryInfo]::new($current).GetAccessControl()}
            Check-FixedSecurity $security $true
        }
        # Descriptor bootstrap is NOT retained ancestry or mapped-image authentication.
    }

    function Open-FixedFile([string]$path) {
        $null=Local-Path $path
        Bootstrap-Chain ([IO.Path]::GetDirectoryName($path))
        Check-Attributes $path $false
        # BEGIN fixed-prerequisite-substage file_open
        $shared['substage']='file_open'
        # END fixed-prerequisite-substage file_open
        $stream=Native-Call {[IO.FileStream]::new($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)}
        $shared['resources'].Add($stream)
        Check-Attributes $path $false
        # BEGIN fixed-prerequisite-substage file_security
        $shared['substage']='file_security'
        # END fixed-prerequisite-substage file_security
        $security=Native-Call {$stream.GetAccessControl()}
        Check-FixedSecurity $security $false
        return $stream
    }

    function Stream-Hash($stream) {
        $algorithm=Native-Call {[Security.Cryptography.SHA256]::Create()}
        try {
            $null=Native-Call {$stream.Position=0}
            $bytes=Native-Call {$algorithm.ComputeHash($stream)}
            $null=Native-Call {$stream.Position=0}
            Check-Deadline
            return ([BitConverter]::ToString($bytes)).Replace('-','').ToLowerInvariant()
        } finally {$null=Native-Call {$algorithm.Dispose()}}
    }

    function Bind-Stock {
        # BEGIN fixed-prerequisite-diagnostics stock_preflight
        $shared['checkpoint']='stock_preflight'
        # END fixed-prerequisite-diagnostics stock_preflight
        if ($PSVersionTable.PSVersion.Major -ne 5 -or $PSVersionTable.PSVersion.Minor -ne 1 -or
            $PSVersionTable.PSEdition -cne 'Desktop' -or -not [Environment]::Is64BitProcess) {throw 'preflight'}
        $system=Local-Path $env:SystemRoot
        $name='Microsoft.PowerShell.Commands.Utility'
        $binary=[IO.Path]::Combine($system,'Microsoft.NET','assembly','GAC_MSIL',$name,
            'v4.0_3.0.0.0__31bf3856ad364e35',($name+'.dll'))
        # BEGIN fixed-prerequisite-diagnostics stock_utility_open
        $shared['checkpoint']='stock_utility_open'
        # END fixed-prerequisite-diagnostics stock_utility_open
        $null=Open-FixedFile $binary
        # BEGIN fixed-prerequisite-diagnostics stock_utility_import
        $shared['checkpoint']='stock_utility_import'
        # END fixed-prerequisite-diagnostics stock_utility_import
        $module=Native-Call {Import-Module -Name $binary -PassThru -ErrorAction Stop}
        # BEGIN fixed-prerequisite-diagnostics stock_utility_exports
        $shared['checkpoint']='stock_exports'
        # END fixed-prerequisite-diagnostics stock_utility_exports
        $script:fromJson=$module.ExportedCmdlets['ConvertFrom-Json']
        $script:toJson=$module.ExportedCmdlets['ConvertTo-Json']
        if ($null -eq $script:fromJson -or $null -eq $script:toJson -or $module.Path -cne $binary) {throw 'preflight'}
        # Import only the exact held native module; no manifest/nested-file autoload.
        $localModule=[IO.Path]::Combine($system,'System32','WindowsPowerShell','v1.0','Modules','Microsoft.PowerShell.LocalAccounts','Microsoft.PowerShell.LocalAccounts.dll')
        # BEGIN fixed-prerequisite-diagnostics stock_accounts_open
        $shared['checkpoint']='stock_accounts_open'
        # END fixed-prerequisite-diagnostics stock_accounts_open
        $null=Open-FixedFile $localModule
        # BEGIN fixed-prerequisite-diagnostics stock_accounts_import
        $shared['checkpoint']='stock_accounts_import'
        # END fixed-prerequisite-diagnostics stock_accounts_import
        $accountsModule=Native-Call {Import-Module -Name $localModule -PassThru -ErrorAction Stop}
        if ($accountsModule.Path -cne $localModule) {throw 'preflight'}
        # BEGIN fixed-prerequisite-diagnostics stock_account_exports
        $shared['checkpoint']='stock_exports'
        # END fixed-prerequisite-diagnostics stock_account_exports
        $script:localCommands=@{}
        foreach ($command in @('New-LocalUser','Get-LocalUser','Get-LocalGroup','Get-LocalGroupMember','Add-LocalGroupMember','Remove-LocalUser')) {
            $info=$accountsModule.ExportedCmdlets[$command]
            if ($null -eq $info -or $info.CommandType -ne [Management.Automation.CommandTypes]::Cmdlet) {throw 'preflight'}
            $script:localCommands[$command]=$info
        }
        Check-Deadline
    }

    function Remaining-Milliseconds([long]$deadline,[int]$maximum) {
        Check-Deadline
        $remaining=[Math]::Floor((($deadline-[Diagnostics.Stopwatch]::GetTimestamp())*1000.0)/[Diagnostics.Stopwatch]::Frequency)
        if ($remaining -lt 1) {throw 'deadline'}
        return [uint32][Math]::Min($remaining,$maximum)
    }

    function New-StartInfo([string]$image,[string]$cwd,[string]$arguments,[string]$temporary) {
        $info=[Diagnostics.ProcessStartInfo]::new()
        $info.FileName=$image; $info.WorkingDirectory=$cwd; $info.Arguments=$arguments
        $info.UseShellExecute=$false; $info.CreateNoWindow=$true
        $info.RedirectStandardOutput=$true; $info.RedirectStandardError=$true
        $info.EnvironmentVariables.Clear()
        $info.EnvironmentVariables['SystemRoot']=$env:SystemRoot
        $info.EnvironmentVariables['WINDIR']=$env:SystemRoot
        $info.EnvironmentVariables['PATH']=[IO.Path]::Combine($env:SystemRoot,'System32')+';'+$env:SystemRoot
        $info.EnvironmentVariables['TEMP']=$temporary; $info.EnvironmentVariables['TMP']=$temporary
        $info.EnvironmentVariables['GITHUB_ACTIONS']='true'
        $info.EnvironmentVariables['LOCRON_HOSTED_PREREQUISITE']='1'
        $info.EnvironmentVariables['LOCRON_RUNNER_ENVIRONMENT']='github-hosted'
        $info.EnvironmentVariables['LOCRON_PREREQUISITE_MODE']='two-user-v1'
        $info.EnvironmentVariables['LOCRON_PREREQUISITE_RUNNER_ENVIRONMENT']='github-hosted'
        return $info
    }

    function Start-Retained($info,[long]$born,[long]$completeDeadline,[bool]$credential) {
        $process=[Diagnostics.Process]::new(); $process.StartInfo=$info
        $shared['resources'].Add($process)
        $shared['phase_deadline']=[Math]::Min($completeDeadline,($born+(15*[Diagnostics.Stopwatch]::Frequency)))
        Check-Deadline
        $started=$process.Start()
        if ($started -and $credential) {$script:counts.executed++}
        Check-Deadline
        if (-not $started) {throw 'credential_start'}
        $shared['phase_deadline']=$completeDeadline
        Check-Deadline
        return $process
    }

    function Guard-Live {
        if ($null -eq $script:guardian) {throw 'guard_setup'}
        $exited=Native-Call {$script:guardian.HasExited}
        Pump-Channel $script:guardStdout; Pump-Channel $script:guardStderr
        if ($exited -or $script:guardStdout.eof -or $script:guardStderr.eof -or
            $script:guardStdout.frames.Count -ne $script:guardSequence) {throw 'guard_setup'}
    }

    function Wait-Task($task) {
        $shared['resources'].Add($task)
        while (-not $task.IsCompleted) {Check-Deadline; [Threading.Thread]::Sleep(5)}
        Check-Deadline
        $value=$task.GetAwaiter().GetResult()
        Check-Deadline
        return $value
    }

    function Guard-Request([string]$op,$payload,[string]$expected,[bool]$finish=$false) {
        Guard-Live
        $request=[ordered]@{schema='locron.windows-user-guard/v1';nonce=$script:guardNonce;
            seq=$script:guardSequence;op=$op;
            remaining_ms=(Remaining-Milliseconds $shared['phase_deadline'] 180000);payload=$payload}
        $json=$request | & $script:toJson -Compress -Depth 6
        $bytes=[Text.UTF8Encoding]::new($false,$true).GetBytes($json+"`n")
        $script:guardSent+=$bytes.Length
        if ($bytes.Length -gt 4096 -or $script:guardSent -gt 32768) {throw 'protocol'}
        $stream=Native-Call {$script:guardian.StandardInput.BaseStream}
        $null=Wait-Task (Native-Call {$stream.WriteAsync($bytes,0,$bytes.Length)})
        $null=Wait-Task (Native-Call {$stream.FlushAsync()})
        if ($finish) {$null=Native-Call {$script:guardian.StandardInput.Close()}}
        do {
            Pump-Channel $script:guardStdout; Pump-Channel $script:guardStderr
            $exited=Native-Call {$script:guardian.HasExited}
            if ($script:guardStdout.frames.Count -le $script:guardSequence) {
                if ($exited -or $script:guardStdout.eof) {throw 'guard_setup'}
                [Threading.Thread]::Sleep(5)
            }
        } while ($script:guardStdout.frames.Count -le $script:guardSequence)
        if ($script:guardStdout.frames.Count -ne ($script:guardSequence+1)) {throw 'protocol'}
        $ack=$script:guardStdout.frames[$script:guardSequence]
        if ($ack.nonce -cne $script:guardNonce -or $ack.seq -ne $script:guardSequence) {throw 'protocol'}
        if (-not $ack.ok) {throw ([string]$ack.payload.failure)}
        if ($ack.op -cne $expected) {throw 'protocol'}
        $script:guardSequence++
        Check-Deadline
        return $ack.payload
    }

    function Check-HeldDirectory([string]$path) {
        Guard-Live
        $renamed=$path+'.refused'
        Require-Absent $renamed 'guard_setup'
        $refused=$false
        try {$null=Native-Call {[IO.Directory]::Move($path,$renamed)}} catch {
            $exception=$_.Exception
            while ($null -ne $exception.InnerException) {$exception=$exception.InnerException}
            if (($exception.HResult -band 65535) -ne 32) {throw 'guard_setup'}
            $refused=$true
        }
        if (-not $refused) {throw 'guard_setup'}
        Check-Attributes $path $true
        Require-Absent $renamed 'guard_setup'
        $refused=$false
        try {$null=Native-Call {[IO.Directory]::Delete($path,$false)}} catch {
            $exception=$_.Exception
            while ($null -ne $exception.InnerException) {$exception=$exception.InnerException}
            if (($exception.HResult -band 65535) -ne 32) {throw 'guard_setup'}
            $refused=$true
        }
        if (-not $refused) {throw 'guard_setup'}
        Check-Attributes $path $true
        Guard-Live
    }

    function New-Account([string]$label,[string]$name) {
        Guard-Live
        $password=New-Password
        $record=@{label=$label;name=$name;sid=$null;created=$false;removed=$false;password=$password}
        $script:accounts.Add($record)
        Check-Deadline
        $new=& $script:localCommands['New-LocalUser'] -Name $name -Password $password -AccountNeverExpires -ErrorAction Stop
        $record.created=$true; $script:counts.created++; $record.sid=$new.SID
        Check-Deadline
        if ($null -eq $record.sid -or $record.sid.Value -ceq $script:runnerSid) {throw 'account'}
        $null=Native-Call {& $script:localCommands['Add-LocalGroupMember'] -SID ([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')) -Member $new -ErrorAction Stop}
        $memberships=[Collections.Generic.List[string]]::new()
        $groups=@(Native-Call {& $script:localCommands['Get-LocalGroup'] -ErrorAction Stop})
        if ($groups.Count -gt 1024) {throw 'account'}
        foreach ($group in $groups) {
            $members=@(Native-Call {& $script:localCommands['Get-LocalGroupMember'] -Group $group -ErrorAction Stop})
            if ($members.Count -gt 4096) {throw 'account'}
            foreach ($member in $members) {if ($member.SID.Value -ceq $record.sid.Value) {$memberships.Add($group.SID.Value)}}
        }
        if ($memberships.Count -ne 1 -or $memberships[0] -cne 'S-1-5-32-545') {throw 'account'}
        Guard-Live
        return $record
    }

    function Remove-RecordedAccount($account) {
        Guard-Live
        if (-not $account.created -or $account.removed -or $null -eq $account.sid) {throw 'cleanup'}
        $actual=Native-Call {& $script:localCommands['Get-LocalUser'] -Name $account.name -ErrorAction Stop}
        if ($actual.Name -cne $account.name -or $actual.SID.Value -cne $account.sid.Value) {throw 'cleanup'}
        Check-Deadline
        $null=& $script:localCommands['Remove-LocalUser'] -SID $account.sid -ErrorAction Stop
        $account.removed=$true; $script:counts.removed++
        Check-Deadline
        $users=@(Native-Call {& $script:localCommands['Get-LocalUser'] -ErrorAction Stop})
        foreach ($user in $users) {if ($user.Name -ceq $account.name -or $user.SID.Value -ceq $account.sid.Value) {throw 'cleanup'}}
        $groupMembers=@(Native-Call {& $script:localCommands['Get-LocalGroupMember'] -SID ([Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')) -ErrorAction Stop})
        foreach ($member in $groupMembers) {if ($member.SID.Value -ceq $account.sid.Value) {throw 'cleanup'}}
        Guard-Live
    }

    function Copy-Image([string]$destination,$source,[string]$expected) {
        Guard-Live
        $security=[Security.AccessControl.FileSecurity]::new()
        $sddl='O:BAD:P(A;;FA;;;BA)(A;;FA;;;SY)'
        foreach ($account in $script:accounts) {$sddl+='(A;;FRFX;;;'+$account.sid.Value+')'}
        $null=Native-Call {$security.SetSecurityDescriptorSddlForm($sddl)}
        Check-Deadline
        $copy=[IO.FileStream]::new($destination,[IO.FileMode]::CreateNew,
            [Security.AccessControl.FileSystemRights]::FullControl,[IO.FileShare]::Read,
            65536,[IO.FileOptions]::WriteThrough,$security)
        $shared['resources'].Add($copy); $script:imageCreated=$true
        Check-Deadline
        $null=Native-Call {$source.Position=0}
        $null=Native-Call {$source.CopyTo($copy,65536)}
        $null=Native-Call {$copy.Flush($true)}
        Check-Attributes $destination $false
        $actual=Native-Call {$copy.GetAccessControl()}
        $owner=Native-Call {$actual.GetOwner([Security.Principal.SecurityIdentifier]).Value}
        if (-not $actual.AreAccessRulesProtected -or $owner -cne 'S-1-5-32-544') {throw 'guard_setup'}
        $actualSddl=Native-Call {$actual.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)}
        $raw=Native-Call {[Security.AccessControl.RawSecurityDescriptor]::new($actualSddl)}
        if ($null -eq $raw.DiscretionaryAcl -or $raw.DiscretionaryAcl.Count -ne 4) {throw 'guard_setup'}
        $seen=[Collections.Generic.List[string]]::new()
        foreach ($ace in $raw.DiscretionaryAcl) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback -or
                $ace.AceQualifier -ne [Security.AccessControl.AceQualifier]::AccessAllowed -or [int]$ace.AceFlags -ne 0) {throw 'guard_setup'}
            $principal=$ace.SecurityIdentifier.Value
            $rights=1179817
            if ($principal -cin @('S-1-5-18','S-1-5-32-544')) {$rights=2032127}
            elseif ($principal -cnotin @($script:accounts[0].sid.Value,$script:accounts[1].sid.Value)) {throw 'guard_setup'}
            if ($ace.AccessMask -ne $rights -or $seen.Contains($principal)) {throw 'guard_setup'}
            $seen.Add($principal)
        }
        if ((Stream-Hash $copy) -cne $expected) {throw 'guard_setup'}
        $null=Native-Call {$copy.Dispose()}
        Guard-Live
    }

    function Verify-RemoveMarker([string]$path,[string]$sid) {
        Check-Attributes $path $false
        $stream=Native-Call {[IO.FileStream]::new($path,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)}
        $shared['resources'].Add($stream)
        $security=Native-Call {$stream.GetAccessControl()}
        $owner=Native-Call {$security.GetOwner([Security.Principal.SecurityIdentifier]).Value}
        if ($owner -cnotin @($sid,'S-1-5-32-544','S-1-5-18')) {throw 'cleanup'}
        $sddl=Native-Call {$security.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)}
        $raw=Native-Call {[Security.AccessControl.RawSecurityDescriptor]::new($sddl)}
        if ($null -eq $raw.DiscretionaryAcl -or $raw.DiscretionaryAcl.Count -ne 3) {throw 'cleanup'}
        $seen=[Collections.Generic.List[string]]::new()
        foreach ($ace in $raw.DiscretionaryAcl) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback -or
                $ace.AceQualifier -ne [Security.AccessControl.AceQualifier]::AccessAllowed -or
                ([int]$ace.AceFlags -band (-bnot 16)) -ne 0 -or $ace.AccessMask -ne 2032127 -or
                $ace.SecurityIdentifier.Value -cnotin @($sid,'S-1-5-32-544','S-1-5-18') -or
                $seen.Contains($ace.SecurityIdentifier.Value)) {throw 'cleanup'}
            $seen.Add($ace.SecurityIdentifier.Value)
        }
        $expected=[Text.Encoding]::ASCII.GetBytes("locron-prerequisite-ready-v1`n")
        $bytes=[byte[]]::new($expected.Length+1)
        $length=Native-Call {$stream.Length}
        if ($length -ne $expected.Length) {throw 'cleanup'}
        $read=Native-Call {$stream.Read($bytes,0,$bytes.Length)}
        if ($read -ne $expected.Length) {throw 'cleanup'}
        for ($index=0;$index -lt $read;$index++) {if ($bytes[$index] -ne $expected[$index]) {throw 'cleanup'}}
        $null=Native-Call {$stream.Dispose()}
        $null=Native-Call {[IO.File]::Delete($path)}
        Require-Absent $path
    }

    function Exact-Entries([string]$path,[string[]]$expected) {
        $entries=@(Native-Call {[IO.Directory]::GetFileSystemEntries($path)})
        if ($entries.Count -ne $expected.Count) {throw 'cleanup'}
        foreach ($entry in $entries) {if ([IO.Path]::GetFileName($entry) -cnotin $expected) {throw 'cleanup'}}
    }

    function Check-PublicSecurity($security,[bool]$directory) {
        # BEGIN fixed-prerequisite-substage public_security_owner
        $shared['substage']='security_owner'
        # END fixed-prerequisite-substage public_security_owner
        $owner=Native-Call {$security.GetOwner([Security.Principal.SecurityIdentifier]).Value}
        if (-not $security.AreAccessRulesProtected -or
            $owner -cnotin @('S-1-5-18','S-1-5-32-544')) {throw 'cleanup'}
        # BEGIN fixed-prerequisite-substage public_security_sddl
        $shared['substage']='security_sddl'
        # END fixed-prerequisite-substage public_security_sddl
        $sddl=Native-Call {$security.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::All)}
        # BEGIN fixed-prerequisite-substage public_security_raw_acl
        $shared['substage']='security_raw_acl'
        # END fixed-prerequisite-substage public_security_raw_acl
        $raw=Native-Call {[Security.AccessControl.RawSecurityDescriptor]::new($sddl)}
        if ($null -eq $raw.DiscretionaryAcl -or $raw.DiscretionaryAcl.Count -ne 2) {throw 'cleanup'}
        $seen=[Collections.Generic.List[string]]::new()
        $flags=0
        if ($directory) {$flags=3}
        # BEGIN fixed-prerequisite-substage public_security_rules
        $shared['substage']='security_rules'
        # END fixed-prerequisite-substage public_security_rules
        foreach ($ace in $raw.DiscretionaryAcl) {
            if ($ace -isnot [Security.AccessControl.CommonAce] -or $ace.IsCallback -or
                $ace.AceQualifier -ne [Security.AccessControl.AceQualifier]::AccessAllowed -or
                [int]$ace.AceFlags -ne $flags -or $ace.AccessMask -ne 2032127 -or
                $ace.SecurityIdentifier.Value -cnotin @('S-1-5-18','S-1-5-32-544') -or
                $seen.Contains($ace.SecurityIdentifier.Value)) {throw 'cleanup'}
            $seen.Add($ace.SecurityIdentifier.Value)
        }
        Check-Deadline
    }

    function Write-PublicNew([string]$path,[byte[]]$bytes) {
        if ($bytes.Length -gt 4096) {throw 'protocol'}
        $security=[Security.AccessControl.FileSecurity]::new()
        $null=Native-Call {$security.SetSecurityDescriptorSddlForm('O:BAD:P(A;;FA;;;BA)(A;;FA;;;SY)')}
        $file=Native-Call {[IO.FileStream]::new($path,[IO.FileMode]::CreateNew,
            [Security.AccessControl.FileSystemRights]::FullControl,[IO.FileShare]::None,
            4096,[IO.FileOptions]::WriteThrough,$security)}
        Check-Attributes $path $false
        Check-PublicSecurity (Native-Call {$file.GetAccessControl()}) $false
        $null=Native-Call {$file.Write($bytes,0,$bytes.Length)}
        $null=Native-Call {$file.Flush($true)}
        $null=Native-Call {$file.Dispose()}
        Check-Deadline
    }

    function Write-Evidence($receipt) {
        # BEGIN fixed-prerequisite-substage evidence_serialize
        $shared['substage']='evidence_serialize'
        # END fixed-prerequisite-substage evidence_serialize
        $encoder=[Text.UTF8Encoding]::new($false,$true)
        $bytes=$encoder.GetBytes((Json-Public $receipt))
        if ($bytes.Length -gt 4096) {throw 'protocol'}
        $identity=[ordered]@{schema='locron.windows-user-built-example/v1';source_sha=$metadata.source_sha;
            base_sha=$metadata.base_sha;target=$metadata.target;compiler='1.94.0';sha256=$script:exampleHash}
        $identityBytes=$encoder.GetBytes((Json-Public $identity))
        if ($identityBytes.Length -gt 4096) {throw 'protocol'}
        # BEGIN fixed-prerequisite-substage evidence_parent
        $shared['substage']='evidence_parent'
        # END fixed-prerequisite-substage evidence_parent
        Bootstrap-Chain $metadata.cwd
        $directory=[IO.Path]::Combine($metadata.cwd,'prerequisite-evidence')
        $security=[Security.AccessControl.DirectorySecurity]::new()
        $null=Native-Call {$security.SetSecurityDescriptorSddlForm('O:BAD:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)')}
        # Existing-or-new only. This supplies NO create-this-run or removal authority;
        # unknown/existing descriptors are read back and refused without repair.
        $info=[IO.DirectoryInfo]::new($directory)
        # BEGIN fixed-prerequisite-substage evidence_directory_create
        $shared['substage']='evidence_directory_create'
        # END fixed-prerequisite-substage evidence_directory_create
        $null=Native-Call {$info.Create($security)}
        Check-Attributes $directory $true
        # BEGIN fixed-prerequisite-substage evidence_directory_security
        $shared['substage']='evidence_directory_security'
        # END fixed-prerequisite-substage evidence_directory_security
        Check-PublicSecurity (Native-Call {$info.GetAccessControl()}) $true
        # BEGIN fixed-prerequisite-substage evidence_identity_write
        $shared['substage']='evidence_identity_write'
        # END fixed-prerequisite-substage evidence_identity_write
        Write-PublicNew ([IO.Path]::Combine($directory,'built-example.json')) $identityBytes
        # BEGIN fixed-prerequisite-substage evidence_receipt_write
        $shared['substage']='evidence_receipt_write'
        # END fixed-prerequisite-substage evidence_receipt_write
        Write-PublicNew ([IO.Path]::Combine($directory,'receipt.json')) $bytes
    }

    $script:guardian=$null; $script:guardSequence=0; $script:guardSent=0
    $script:accounts=[Collections.Generic.List[object]]::new()
    $script:counts=[ordered]@{created=0;executed=0;removed=0}
    $script:imageCreated=$false; $script:exampleHash=''
    $guardFacts=[ordered]@{}
    foreach ($key in @('bootstrap_fixed_trust','anchor_held','job_created','controls_held','image_held','continuous',
        'image_released','controls_released','job_released','all_released','stdout_eof','stderr_eof','exit_zero')) {$guardFacts[$key]=$false}
    $facts=[Collections.Generic.List[object]]::new()
    $runnerAdministrative=$false; $os='unavailable'; $build='unavailable'
    try {
        # BEGIN fixed-prerequisite-diagnostics metadata
        $shared['checkpoint']='metadata'
        # END fixed-prerequisite-diagnostics metadata
        Check-Metadata
        Begin-Phase 30 $shared['entered']
        $setupDeadline=[long]$shared['phase_deadline']
        Bind-Stock
        # BEGIN fixed-prerequisite-diagnostics runner_identity
        $shared['checkpoint']='runner_identity'
        # END fixed-prerequisite-diagnostics runner_identity
        $identity=Native-Call {[Security.Principal.WindowsIdentity]::GetCurrent()}
        $shared['resources'].Add($identity)
        $principal=[Security.Principal.WindowsPrincipal]::new($identity)
        $runnerAdministrative=Native-Call {$principal.IsInRole([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))}
        $script:runnerSid=Native-Call {$identity.User.Value}
        if (-not $runnerAdministrative) {throw 'preflight'}
        # BEGIN fixed-prerequisite-diagnostics os_metadata
        $shared['checkpoint']='os_metadata'
        # END fixed-prerequisite-diagnostics os_metadata
        $os=Native-Call {[Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion','ProductName',$null)}
        $build=Native-Call {[Microsoft.Win32.Registry]::GetValue('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion','CurrentBuildNumber',$null)}
        $os=Bounded-Ascii ([string]$os) 96; $build=Bounded-Ascii ([string]$build) 32
        # BEGIN fixed-prerequisite-diagnostics checkout_bootstrap
        $shared['checkpoint']='checkout_bootstrap'
        # END fixed-prerequisite-diagnostics checkout_bootstrap
        $cwd=Local-Path $metadata.cwd
        Bootstrap-Chain $cwd
        # BEGIN fixed-prerequisite-diagnostics image_open
        $shared['checkpoint']='image_open'
        # END fixed-prerequisite-diagnostics image_open
        $sourcePath=Local-Path ([IO.Path]::Combine($cwd,'target',$metadata.target,'debug','examples','windows_user_prerequisite.exe'))
        $source=Open-FixedFile $sourcePath
        # BEGIN fixed-prerequisite-diagnostics image_hash
        $shared['checkpoint']='image_hash'
        # END fixed-prerequisite-diagnostics image_hash
        $script:exampleHash=Stream-Hash $source
        # BEGIN fixed-prerequisite-diagnostics anchor_bootstrap
        $shared['checkpoint']='anchor_bootstrap'
        # END fixed-prerequisite-diagnostics anchor_bootstrap
        $anchor=Local-Path $env:ProgramFiles
        Bootstrap-Chain $anchor
        # Guardian TEMP/TMP is the already validated existing administrative anchor;
        # credential actor TEMP/TMP is its exact held control leaf, never a runner profile.
        $temporary=$anchor
        $guardFacts.bootstrap_fixed_trust=$true
        $script:guardNonce=[Guid]::NewGuid().ToString('N')
        $jobName='locron-prerequisite-v1-'+[Guid]::NewGuid().ToString('N')
        $job=[IO.Path]::Combine($anchor,$jobName)
        # BEGIN fixed-prerequisite-diagnostics guardian_start
        $shared['checkpoint']='guardian_start'
        # END fixed-prerequisite-diagnostics guardian_start
        $guardBorn=[Diagnostics.Stopwatch]::GetTimestamp()
        $info=New-StartInfo $sourcePath $cwd '--hosted-prerequisite guard-admin' $temporary
        $info.RedirectStandardInput=$true; $info.LoadUserProfile=$false
        $script:guardian=Start-Retained $info $guardBorn $setupDeadline $false
        $script:guardStdout=New-Channel (Native-Call {$script:guardian.StandardOutput.BaseStream}) 'guard'
        $script:guardStderr=New-Channel (Native-Call {$script:guardian.StandardError.BaseStream}) 'empty'
        # BEGIN fixed-prerequisite-diagnostics guard_acquire_anchor
        $shared['checkpoint']='guard_acquire_anchor'
        # END fixed-prerequisite-diagnostics guard_acquire_anchor
        $ack=Guard-Request 'acquire_anchor' ([ordered]@{anchor=$anchor;source_image=$sourcePath;source_sha256=$script:exampleHash;cwd=$cwd;
            controller_remaining_ms=(Remaining-Milliseconds $shared['controller_deadline'] 180000);
            setup_remaining_ms=(Remaining-Milliseconds $setupDeadline 30000)}) 'anchor_held'
        $guardianId=Native-Call {$script:guardian.Id}
        if ($ack.pid -ne $guardianId -or $ack.runner_sid -cne $script:runnerSid -or $ack.source_sha256 -cne $script:exampleHash) {throw 'guard_setup'}
        $guardFacts.anchor_held=$true
        # BEGIN fixed-prerequisite-diagnostics guard_create_job
        $shared['checkpoint']='guard_create_job'
        # END fixed-prerequisite-diagnostics guard_create_job
        $null=Guard-Request 'create_job' ([ordered]@{name=$jobName}) 'job_held'
        $guardFacts.job_created=$true
        Check-HeldDirectory $job
        $nameStem='lc'+([Guid]::NewGuid().ToString('N')).Substring(0,12)
        # BEGIN fixed-prerequisite-diagnostics account_a_create
        $shared['checkpoint']='account_a_create'
        # END fixed-prerequisite-diagnostics account_a_create
        $accountA=New-Account 'A' ($nameStem+'a')
        # BEGIN fixed-prerequisite-diagnostics account_b_create
        $shared['checkpoint']='account_b_create'
        # END fixed-prerequisite-diagnostics account_b_create
        $accountB=New-Account 'B' ($nameStem+'b')
        if ($accountA.sid.Value -ceq $accountB.sid.Value) {throw 'account'}
        # BEGIN fixed-prerequisite-diagnostics guard_create_controls
        $shared['checkpoint']='guard_create_controls'
        # END fixed-prerequisite-diagnostics guard_create_controls
        $null=Guard-Request 'create_controls' ([ordered]@{actors=@([ordered]@{label='A';sid=$accountA.sid.Value},[ordered]@{label='B';sid=$accountB.sid.Value})}) 'controls_held'
        $guardFacts.controls_held=$true
        foreach ($account in $script:accounts) {Check-HeldDirectory ([IO.Path]::Combine($job,$account.label))}
        $imagePath=[IO.Path]::Combine($job,'windows_user_prerequisite.exe')
        # BEGIN fixed-prerequisite-diagnostics image_copy
        $shared['checkpoint']='image_copy'
        # END fixed-prerequisite-diagnostics image_copy
        Copy-Image $imagePath $source $script:exampleHash
        # BEGIN fixed-prerequisite-diagnostics guard_hold_image
        $shared['checkpoint']='guard_hold_image'
        # END fixed-prerequisite-diagnostics guard_hold_image
        $hashAck=Guard-Request 'hold_image' ([ordered]@{sha256=$script:exampleHash}) 'image_held'
        if ($hashAck.sha256 -cne $script:exampleHash) {throw 'guard_setup'}
        $guardFacts.image_held=$true
        foreach ($account in $script:accounts) {
            $shared['phase_deadline']=$shared['controller_deadline']; Check-Deadline
            Guard-Live
            $actorBorn=[Diagnostics.Stopwatch]::GetTimestamp()
            $actorDeadline=[Math]::Min([long]$shared['controller_deadline'],($actorBorn+(45*[Diagnostics.Stopwatch]::Frequency)))
            $shared['phase_deadline']=$actorDeadline
            $control=[IO.Path]::Combine($job,$account.label)
            $arguments=@('--hosted-prerequisite','--role','outer','--actor',$account.label,'--control-dir',$control,
                '--remaining-ms',([string](Remaining-Milliseconds $actorDeadline 30000)))
            $quoted=[Collections.Generic.List[string]]::new()
            foreach ($argument in $arguments) {$quoted.Add((Quote-NativeArgument $argument))}
            $info=New-StartInfo $imagePath $control ($quoted -join ' ') $control
            $info.UserName=$account.name; $info.Domain=[Environment]::MachineName
            $info.Password=$account.password; $info.LoadUserProfile=$true
            # BEGIN fixed-prerequisite-diagnostics actor_start
            $shared['checkpoint']='actor_start'
            # END fixed-prerequisite-diagnostics actor_start
            $process=Start-Retained $info $actorBorn $actorDeadline $true
            # BEGIN fixed-prerequisite-diagnostics actor_read
            $shared['checkpoint']='actor_read'
            # END fixed-prerequisite-diagnostics actor_read
            $facts.Add((Read-Actor $process $account))
            Guard-Live
            $null=Native-Call {$process.Dispose()}
        }
        $shared['phase_deadline']=$shared['controller_deadline']; Check-Deadline
        Begin-Phase 30 ([Diagnostics.Stopwatch]::GetTimestamp())
        # BEGIN fixed-prerequisite-diagnostics cleanup_accounts
        $shared['checkpoint']='cleanup_accounts'
        # END fixed-prerequisite-diagnostics cleanup_accounts
        foreach ($account in $script:accounts) {Remove-RecordedAccount $account}
        Guard-Live
        $guardFacts.continuous=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_release_image
        $shared['checkpoint']='cleanup_release_image'
        # END fixed-prerequisite-diagnostics cleanup_release_image
        $null=Guard-Request 'release_image' ([ordered]@{}) 'image_released'
        $guardFacts.image_released=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_remove_image
        $shared['checkpoint']='cleanup_remove_image'
        # END fixed-prerequisite-diagnostics cleanup_remove_image
        Exact-Entries $job @('A','B','windows_user_prerequisite.exe')
        $null=Native-Call {[IO.File]::Delete($imagePath)}
        Require-Absent $imagePath
        # BEGIN fixed-prerequisite-diagnostics cleanup_remove_markers
        $shared['checkpoint']='cleanup_remove_markers'
        # END fixed-prerequisite-diagnostics cleanup_remove_markers
        foreach ($account in $script:accounts) {
            $control=[IO.Path]::Combine($job,$account.label)
            Exact-Entries $control @('grandchild.ready')
            Verify-RemoveMarker ([IO.Path]::Combine($control,'grandchild.ready')) $account.sid.Value
            Exact-Entries $control @()
        }
        # BEGIN fixed-prerequisite-diagnostics cleanup_release_controls
        $shared['checkpoint']='cleanup_release_controls'
        # END fixed-prerequisite-diagnostics cleanup_release_controls
        $null=Guard-Request 'release_controls' ([ordered]@{}) 'controls_released'
        $guardFacts.controls_released=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_remove_controls
        $shared['checkpoint']='cleanup_remove_controls'
        # END fixed-prerequisite-diagnostics cleanup_remove_controls
        foreach ($account in $script:accounts) {
            $control=[IO.Path]::Combine($job,$account.label)
            $null=Native-Call {[IO.Directory]::Delete($control,$false)}
            Require-Absent $control
        }
        Exact-Entries $job @()
        # BEGIN fixed-prerequisite-diagnostics cleanup_release_job
        $shared['checkpoint']='cleanup_release_job'
        # END fixed-prerequisite-diagnostics cleanup_release_job
        $null=Guard-Request 'release_job' ([ordered]@{}) 'job_released'
        $guardFacts.job_released=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_remove_job
        $shared['checkpoint']='cleanup_remove_job'
        # END fixed-prerequisite-diagnostics cleanup_remove_job
        $null=Native-Call {[IO.Directory]::Delete($job,$false)}
        Require-Absent $job
        # BEGIN fixed-prerequisite-diagnostics cleanup_finish_guard
        $shared['checkpoint']='cleanup_finish_guard'
        # END fixed-prerequisite-diagnostics cleanup_finish_guard
        $null=Guard-Request 'finish' ([ordered]@{}) 'all_released' $true
        $guardFacts.all_released=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_guard_eof
        $shared['checkpoint']='cleanup_guard_eof'
        # END fixed-prerequisite-diagnostics cleanup_guard_eof
        do {
            Pump-Channel $script:guardStdout; Pump-Channel $script:guardStderr
            $exited=Native-Call {$script:guardian.HasExited}
            if (-not ($exited -and $script:guardStdout.eof -and $script:guardStderr.eof)) {[Threading.Thread]::Sleep(5)}
        } while (-not ($exited -and $script:guardStdout.eof -and $script:guardStderr.eof))
        $guardianExit=Native-Call {$script:guardian.ExitCode}
        if ($script:guardSequence -ne 8 -or $script:guardStdout.frames.Count -ne 8 -or $guardianExit -ne 0) {throw 'cleanup'}
        $guardFacts.stdout_eof=$true; $guardFacts.stderr_eof=$true; $guardFacts.exit_zero=$true
        # BEGIN fixed-prerequisite-diagnostics cleanup_dispose
        $shared['checkpoint']='cleanup_dispose'
        # END fixed-prerequisite-diagnostics cleanup_dispose
        foreach ($resource in $shared['resources']) {
            if ($resource -is [IDisposable]) {$null=Native-Call {$resource.Dispose()}}
        }
        $elapsed=[long][Math]::Floor((([Diagnostics.Stopwatch]::GetTimestamp()-[long]$shared['entered'])*1000.0)/[Diagnostics.Stopwatch]::Frequency)
        $receipt=[ordered]@{schema='locron.windows-user-prerequisite/v1';source_sha=$metadata.source_sha;base_sha=$metadata.base_sha;
            workflow=$metadata.workflow;run_id=$metadata.run_id;run_attempt=$metadata.run_attempt;target=$metadata.target;
            os=[ordered]@{sku=$os;build=$build;image=$metadata.image};compiler='1.94.0';example_sha256=$script:exampleHash;
            runner_administrative=$runnerAdministrative;identities_distinct=$true;actors=$facts.ToArray();counts=$script:counts;
            cleanup='confirmed';profile_disposition='disposable-vm';elapsed_ms=$elapsed;failure=$null;guard=$guardFacts}
        if ($facts.Count -ne 2 -or $script:counts.created -ne 2 -or $script:counts.executed -ne 2 -or $script:counts.removed -ne 2) {throw 'protocol'}
        foreach ($value in $guardFacts.Values) {if (-not $value) {throw 'protocol'}}
        # BEGIN fixed-prerequisite-diagnostics evidence_publish_success
        $shared['checkpoint']='evidence_publish'
        # END fixed-prerequisite-diagnostics evidence_publish_success
        Write-Evidence $receipt
        Check-Deadline
        # BEGIN fixed-prerequisite-diagnostics confirmed
        $shared['checkpoint']='confirmed'
        # END fixed-prerequisite-diagnostics confirmed
        [pscustomobject]@{confirmed=$true}
    } catch {
        # A raw exception/identity/path/partial native result never escapes this owner.
        # If bounded public failure evidence can still be written, it retains only actual
        # known counts/checkpoints. Otherwise missing evidence itself fails the workflow.
        $category='native_owner_unknown'
        if ($_.Exception.Message -cin @('preflight','account','credential_start','token','protocol','containment','deadline','cleanup','guard_setup')) {
            $category=$_.Exception.Message
        }
        # BEGIN fixed-prerequisite-diagnostics owner_failure_snapshot
        $shared['failure_checkpoint']=$shared['checkpoint']
        $shared['failure_category']=$category
        $shared['failure_kind']=Get-FixedFailureKind $_.Exception
        # BEGIN fixed-prerequisite-substage first_failure_snapshot
        $shared['failure_substage']=$shared['substage']
        # END fixed-prerequisite-substage first_failure_snapshot
        # END fixed-prerequisite-diagnostics owner_failure_snapshot
        $guardFacts.continuous=$false
        try {
            Check-Deadline
            $elapsed=[long][Math]::Floor((([Diagnostics.Stopwatch]::GetTimestamp()-[long]$shared['entered'])*1000.0)/[Diagnostics.Stopwatch]::Frequency)
            $receipt=[ordered]@{schema='locron.windows-user-prerequisite/v1';source_sha=$metadata.source_sha;base_sha=$metadata.base_sha;
                workflow=$metadata.workflow;run_id=$metadata.run_id;run_attempt=$metadata.run_attempt;target=$metadata.target;
                os=[ordered]@{sku=$os;build=$build;image=$metadata.image};compiler='1.94.0';example_sha256=$script:exampleHash;
                runner_administrative=$runnerAdministrative;identities_distinct=$false;actors=$facts.ToArray();counts=$script:counts;
                cleanup='unknown';profile_disposition='disposable-vm';elapsed_ms=$elapsed;failure=$category;guard=$guardFacts}
            # BEGIN fixed-prerequisite-diagnostics failure_evidence_attempt
            $shared['checkpoint']='evidence_publish'
            $shared['failure_evidence']='attempting'
            # END fixed-prerequisite-diagnostics failure_evidence_attempt
            Write-Evidence $receipt
            # BEGIN fixed-prerequisite-diagnostics failure_evidence_written
            $shared['failure_evidence']='written'
            # END fixed-prerequisite-diagnostics failure_evidence_written
        # BEGIN fixed-prerequisite-diagnostics evidence_refusal_handler
        } catch {
            $shared['evidence_kind']=Get-FixedFailureKind $_.Exception
            # BEGIN fixed-prerequisite-substage evidence_failure_snapshot
            $shared['evidence_substage']=$shared['substage']
            # END fixed-prerequisite-substage evidence_failure_snapshot
            $shared['failure_evidence']='refused'
        }
        # END fixed-prerequisite-diagnostics evidence_refusal_handler
        Keep-Unknown
    }
}

$shared=[hashtable]::Synchronized(@{entered=$controllerEntered;
    controller_deadline=($controllerEntered+(180*[Diagnostics.Stopwatch]::Frequency));
    phase_deadline=($controllerEntered+(30*[Diagnostics.Stopwatch]::Frequency));
    expired=$false;unknown=$false;resources=[Collections.Generic.List[object]]::new()})
# BEGIN fixed-prerequisite-diagnostics diagnostic_defaults
$shared['checkpoint']='entry'
$shared['failure_checkpoint']='unknown'
$shared['failure_category']='unknown'
$shared['failure_kind']='unknown'
$shared['failure_evidence']='not_attempted'
$shared['evidence_kind']='unknown'
# END fixed-prerequisite-diagnostics diagnostic_defaults
# BEGIN fixed-prerequisite-substage defaults
$shared['substage']='entry'
$shared['failure_substage']='unknown'
$shared['evidence_substage']='unknown'
# END fixed-prerequisite-substage defaults
$metadata=@{source_sha=$env:GITHUB_SHA;base_sha=$env:LOCRON_PREREQUISITE_BASE_SHA;
    workflow=$env:GITHUB_WORKFLOW;run_id=$env:GITHUB_RUN_ID;run_attempt=$env:GITHUB_RUN_ATTEMPT;
    target=$env:LOCRON_PREREQUISITE_TARGET;image=($env:ImageOS+'/'+$env:ImageVersion);cwd=[Environment]::CurrentDirectory}
$invocation=[Management.Automation.PowerShell]::Create()
$null=$invocation.AddScript($nativeOwner.ToString()).AddArgument($shared).AddArgument($metadata)
# No pipeline input collection exists; BeginInvoke cannot wait for open input.
$pending=$invocation.BeginInvoke()
while (-not $pending.IsCompleted) {
    if ($shared['unknown'] -or [Diagnostics.Stopwatch]::GetTimestamp() -ge [long]$shared['phase_deadline']) {
        $shared['expired']=$true
        # No unfinished EndInvoke/Stop/Dispose, resource-list access or cleanup claim.
        # BEGIN fixed-prerequisite-diagnostics pending_refusal
        $branch='phase_expired_pending'
        if ($shared['unknown']) {$branch='shared_unknown'}
        Write-FixedRefusalDiagnostic $branch
        # END fixed-prerequisite-diagnostics pending_refusal
        [Environment]::Exit(1)
    }
    [Threading.Thread]::Sleep(5)
}
if ($shared['expired'] -or [Diagnostics.Stopwatch]::GetTimestamp() -ge [long]$shared['phase_deadline']) {Write-FixedRefusalDiagnostic 'phase_expired_completed'; [Environment]::Exit(1)}
try {$result=$invocation.EndInvoke($pending)} catch {
    # BEGIN fixed-prerequisite-diagnostics endinvoke_refusal
    Write-FixedRefusalDiagnostic 'endinvoke_exception' (Get-FixedFailureKind $_.Exception)
    # END fixed-prerequisite-diagnostics endinvoke_refusal
    [Environment]::Exit(1)
}
if ($invocation.HadErrors -or $result.Count -ne 1 -or -not $result[0].confirmed -or
    [Diagnostics.Stopwatch]::GetTimestamp() -ge [long]$shared['phase_deadline']) {Write-FixedRefusalDiagnostic 'result_rejected'; [Environment]::Exit(1)}
$invocation.Dispose()
if ([Diagnostics.Stopwatch]::GetTimestamp() -ge [long]$shared['phase_deadline']) {Write-FixedRefusalDiagnostic 'phase_expired_after_dispose'; [Environment]::Exit(1)}
[Console]::WriteLine('windows-user-prerequisite confirmed: A/B tokens, trees, accounts and guards')
if ([Diagnostics.Stopwatch]::GetTimestamp() -ge [long]$shared['phase_deadline']) {Write-FixedRefusalDiagnostic 'phase_expired_after_success_print'; [Environment]::Exit(1)}
[Environment]::Exit(0)
