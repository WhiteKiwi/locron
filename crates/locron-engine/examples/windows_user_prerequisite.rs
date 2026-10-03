//! Explicit hosted two-user prerequisite; compilation or zero-case libtest is not acceptance.
//! Private identity frames belong only to the orchestrator's inherited control channels.

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(windows)]
    {
        let entered = std::time::Instant::now();
        windows::entry(entered)
    }
    #[cfg(not(windows))]
    {
        eprintln!("windows-user-prerequisite requires native Windows");
        ExitCode::FAILURE
    }
}

#[cfg(windows)]
mod windows {
    use std::fs::{File, OpenOptions};
    use std::io::{self, Read as _, Seek as _, SeekFrom, Write};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::{Component, Path, PathBuf, Prefix};
    use std::process::{Command as NativeCommand, ExitCode, Stdio};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    };
    use std::time::{Duration, Instant};

    use locron_core::filesystem::DirectoryGuard;
    use locron_engine::windows_child::{ChildWindow, OwnedChild, SpawnFailure};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest, Sha256};
    use tokio::process::Command;
    use windows_permissions::constants::{AceType, SeObjectType, SecurityInformation};
    use windows_permissions::{LocalBox, SecurityDescriptor, Sid, wrappers};

    const IDENTITY: &str = "locron.windows-user-identity/v1";
    const OWNER: &str = "locron.windows-user-owner/v1";
    const READY: &[u8] = b"locron-prerequisite-ready-v1\n";
    const MAX_MILLIS: u64 = 30_000;
    const FRAME_LIMIT: usize = 1_024;
    const TOKEN_PROBE: &str = r"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    $users = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-545')
    $admins = [Security.Principal.SecurityIdentifier]::new('S-1-5-32-544')
    $forbidden = $false
    foreach ($group in $identity.Groups) {
        if ($group.Value.StartsWith('S-1-5-32-', [StringComparison]::Ordinal) -and $group.Value -cne 'S-1-5-32-545') { $forbidden = $true }
    }
    [pscustomobject]@{
        sid = $identity.User.Value
        users_enabled = $principal.IsInRole($users)
        administrators_enabled = $principal.IsInRole($admins)
        forbidden_builtin_group_present = $forbidden
    } | & $locronToJson -Compress
} finally { $identity.Dispose() }
";

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum Role {
        Outer,
        Inner,
        Grandchild,
    }

    struct Arguments {
        role: Role,
        actor: String,
        control: PathBuf,
        remaining: Duration,
    }

    #[derive(Clone, Copy)]
    enum Failure {
        Arguments,
        Deadline,
        Native,
        Token,
        Protocol,
        Containment,
    }

    type Result<T> = std::result::Result<T, Failure>;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        sid: String,
        users_enabled: bool,
        administrators_enabled: bool,
        forbidden_builtin_group_present: bool,
    }

    #[derive(Serialize)]
    struct IdentityFrame<'a> {
        schema: &'a str,
        actor: &'a str,
        role: &'a str,
        pid: u32,
        sid: &'a str,
        probe_sid: &'a str,
        users_enabled: bool,
        administrators_enabled: bool,
        forbidden_builtin_group_present: bool,
    }

    #[derive(Serialize)]
    struct StartedFrame<'a> {
        schema: &'a str,
        actor: &'a str,
        phase: &'a str,
        pid: u32,
        sid: &'a str,
        root_pid: u32,
    }

    #[derive(Serialize)]
    struct LiveFrame<'a> {
        schema: &'a str,
        actor: &'a str,
        phase: &'a str,
        root_exit_zero: bool,
        tree_empty: bool,
        negative_timed_out: bool,
    }

    #[derive(Serialize)]
    struct CleanupFrame<'a> {
        schema: &'a str,
        actor: &'a str,
        phase: &'a str,
        root_exit_zero: bool,
        tree_empty: bool,
    }

    fn parse() -> Result<Arguments> {
        if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
            || std::env::var("LOCRON_PREREQUISITE_RUNNER_ENVIRONMENT").as_deref()
                != Ok("github-hosted")
            || std::env::var("LOCRON_PREREQUISITE_MODE").as_deref() != Ok("two-user-v1")
        {
            return Err(Failure::Arguments);
        }
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        if arguments.len() != 9
            || arguments[0] != "--hosted-prerequisite"
            || arguments[1] != "--role"
            || arguments[3] != "--actor"
            || arguments[5] != "--control-dir"
            || arguments[7] != "--remaining-ms"
        {
            return Err(Failure::Arguments);
        }
        let role = match arguments[2].to_str() {
            Some("outer") => Role::Outer,
            Some("inner") => Role::Inner,
            Some("grandchild") => Role::Grandchild,
            _ => return Err(Failure::Arguments),
        };
        let actor = arguments[4].to_str().ok_or(Failure::Arguments)?;
        if !matches!(actor, "A" | "B") {
            return Err(Failure::Arguments);
        }
        let control = local_path(arguments[6].to_str().ok_or(Failure::Arguments)?)?;
        let namespace = control
            .parent()
            .and_then(Path::file_name)
            .and_then(std::ffi::OsStr::to_str)
            .ok_or(Failure::Arguments)?;
        if control.file_name().and_then(std::ffi::OsStr::to_str) != Some(actor)
            || namespace.len() != 55
            || !namespace.starts_with("locron-prerequisite-v1-")
            || !hex(&namespace[23..], 32)
        {
            return Err(Failure::Arguments);
        }
        let millis = arguments[8]
            .to_str()
            .ok_or(Failure::Arguments)?
            .parse::<u64>()
            .map_err(|_| Failure::Arguments)?;
        if !(1..=MAX_MILLIS).contains(&millis) {
            return Err(Failure::Arguments);
        }
        Ok(Arguments {
            role,
            actor: actor.to_owned(),
            control,
            remaining: Duration::from_millis(millis),
        })
    }

    fn timely(deadline: Instant) -> Result<()> {
        if Instant::now() >= deadline {
            Err(Failure::Deadline)
        } else {
            Ok(())
        }
    }

    fn millis_left(deadline: Instant) -> Result<u64> {
        timely(deadline)?;
        let millis = u64::try_from(
            deadline
                .saturating_duration_since(Instant::now())
                .as_millis(),
        )
        .map_err(|_| Failure::Deadline)?;
        if millis == 0 {
            return Err(Failure::Deadline);
        }
        Ok(millis.min(MAX_MILLIS))
    }

    fn sid(deadline: Instant) -> Result<String> {
        let token_error = |failure| match failure {
            Failure::Native => Failure::Token,
            other => other,
        };
        let native = checked_native(
            deadline,
            windows_permissions::utilities::current_process_sid,
        )
        .map_err(token_error)?;
        let text = checked_native(deadline, || wrappers::ConvertSidToStringSid(&native))
            .map_err(token_error)?
            .into_string()
            .map_err(|_| Failure::Token)?;
        timely(deadline)?;
        let parts: Vec<_> = text.split('-').collect();
        if text.len() > 256
            || !text.is_ascii()
            || !(4..=18).contains(&parts.len())
            || parts[0] != "S"
            || parts[1] != "1"
            || parts[2..].iter().any(|part| {
                part.is_empty()
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
                    || (part.len() > 1 && part.starts_with('0'))
            })
        {
            return Err(Failure::Token);
        }
        Ok(text)
    }

    fn emit(frame: &impl Serialize, output: impl Write, deadline: Instant) -> Result<()> {
        timely(deadline)?;
        let mut bytes = serde_json::to_vec(frame).map_err(|_| Failure::Protocol)?;
        bytes.push(b'\n');
        if bytes.len() > FRAME_LIMIT {
            return Err(Failure::Protocol);
        }
        let mut output = output;
        checked_native(deadline, || output.write_all(&bytes))?;
        checked_native(deadline, || output.flush())
    }

    fn identity(arguments: &Arguments, role: &'static str, deadline: Instant) -> Result<()> {
        let primary = sid(deadline)?;
        timely(deadline)?;
        let value = locron_core::windows::run_script_json_until(
            TOKEN_PROBE,
            &serde_json::json!({}),
            deadline,
        )
        .map_err(|_| Failure::Token)?;
        timely(deadline)?;
        let probe: Probe = serde_json::from_value(value).map_err(|_| Failure::Protocol)?;
        if primary != probe.sid
            || !probe.users_enabled
            || probe.administrators_enabled
            || probe.forbidden_builtin_group_present
        {
            return Err(Failure::Token);
        }
        emit(
            &IdentityFrame {
                schema: IDENTITY,
                actor: &arguments.actor,
                role,
                pid: std::process::id(),
                sid: &primary,
                probe_sid: &probe.sid,
                users_enabled: probe.users_enabled,
                administrators_enabled: probe.administrators_enabled,
                forbidden_builtin_group_present: probe.forbidden_builtin_group_present,
            },
            io::stdout().lock(),
            deadline,
        )
    }

    fn child_arguments(
        arguments: &Arguments,
        role: &str,
        remaining: u64,
    ) -> Vec<std::ffi::OsString> {
        [
            "--hosted-prerequisite".into(),
            "--role".into(),
            role.into(),
            "--actor".into(),
            arguments.actor.clone().into(),
            "--control-dir".into(),
            arguments.control.as_os_str().to_owned(),
            "--remaining-ms".into(),
            remaining.to_string().into(),
        ]
        .into()
    }

    // Quarantine retains the actual native ownership until the administrative controller
    // destroys this failed actor. Parking is not a join, tree proof or a successful cleanup.
    fn retain_unknown<T>(owner: T) -> ! {
        loop {
            std::hint::black_box(&owner);
            std::thread::park();
        }
    }

    async fn outer(arguments: &Arguments, deadline: Instant) -> Result<()> {
        // Only argument/image/cwd/deadline/runtime selection precedes this first admission.
        let image = checked_native(deadline, std::env::current_exe)?;
        let mut command = Command::new(image);
        command
            .args(child_arguments(arguments, "inner", millis_left(deadline)?))
            .current_dir(&arguments.control)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null());
        timely(deadline)?;
        let mut child = match OwnedChild::spawn(command, ChildWindow::Hidden) {
            Ok(child) => child,
            Err(SpawnFailure::NotStarted(_)) => return Err(Failure::Native),
            Err(SpawnFailure::ExecutionMayHaveStarted { containment, .. }) => {
                retain_unknown(containment)
            }
        };
        let result = async {
            timely(deadline)?;
            let primary = sid(deadline)?;
            let root_pid = child
                .id()
                .filter(|pid| *pid > 0)
                .ok_or(Failure::Containment)?;
            emit(
                &StartedFrame {
                    schema: OWNER,
                    actor: &arguments.actor,
                    phase: "owner_started",
                    pid: std::process::id(),
                    sid: &primary,
                    root_pid,
                },
                io::stderr().lock(),
                deadline,
            )?;
            let root_status = loop {
                timely(deadline)?;
                let status = child.try_wait().map_err(|_| Failure::Containment)?;
                timely(deadline)?;
                if let Some(status) = status {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            };
            timely(deadline)?;
            let tree_empty = child.tree_empty().map_err(|_| Failure::Containment)?;
            timely(deadline)?;
            if !root_status.success() || tree_empty {
                return Err(Failure::Containment);
            }
            let negative_deadline = deadline.min(Instant::now() + Duration::from_millis(100));
            let negative = child.confirm_exit_until(negative_deadline).await;
            timely(deadline)?;
            if !matches!(negative, Err(ref error) if error.kind() == io::ErrorKind::TimedOut) {
                return Err(Failure::Containment);
            }
            let tree_empty = child.tree_empty().map_err(|_| Failure::Containment)?;
            timely(deadline)?;
            if tree_empty {
                return Err(Failure::Containment);
            }
            emit(
                &LiveFrame {
                    schema: OWNER,
                    actor: &arguments.actor,
                    phase: "root_reaped_tree_live_negative_confirm",
                    root_exit_zero: root_status.success(),
                    tree_empty,
                    negative_timed_out: true,
                },
                io::stderr().lock(),
                deadline,
            )?;
            timely(deadline)?;
            let terminated = child
                .terminate_until(deadline)
                .await
                .map_err(|_| Failure::Containment)?;
            timely(deadline)?;
            let tree_empty = child.tree_empty().map_err(|_| Failure::Containment)?;
            timely(deadline)?;
            if !terminated.success() || !tree_empty {
                return Err(Failure::Containment);
            }
            emit(
                &CleanupFrame {
                    schema: OWNER,
                    actor: &arguments.actor,
                    phase: "cleanup_confirmed",
                    root_exit_zero: terminated.success(),
                    tree_empty,
                },
                io::stderr().lock(),
                deadline,
            )
        }
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(_) => retain_unknown(child),
        }
    }

    fn inner(arguments: &Arguments, deadline: Instant) -> Result<()> {
        identity(arguments, "inner", deadline)?;
        let image = checked_native(deadline, std::env::current_exe)?;
        let mut command = NativeCommand::new(image);
        command
            .args(child_arguments(
                arguments,
                "grandchild",
                millis_left(deadline)?,
            ))
            .current_dir(&arguments.control)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null());
        let mut grandchild = checked_native(deadline, || command.spawn())?;
        let ready = arguments.control.join("grandchild.ready");
        loop {
            timely(deadline)?;
            let ended = grandchild.try_wait().map_err(|_| Failure::Containment)?;
            timely(deadline)?;
            if ended.is_some() {
                return Err(Failure::Containment);
            }
            let marker = checked_native(deadline, || match File::open(&ready) {
                Ok(file) => Ok(Some(file)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(error) => Err(error),
            })
            .map_err(|failure| match failure {
                Failure::Native => Failure::Protocol,
                other => other,
            })?;
            if let Some(file) = marker {
                let mut bytes = Vec::with_capacity(READY.len() + 1);
                let mut bounded =
                    file.take(u64::try_from(READY.len() + 1).expect("fixed marker limit"));
                checked_native(deadline, || bounded.read_to_end(&mut bytes)).map_err(
                    |failure| match failure {
                        Failure::Native => Failure::Protocol,
                        other => other,
                    },
                )?;
                if bytes != READY {
                    return Err(Failure::Protocol);
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        timely(deadline)?;
        if grandchild
            .try_wait()
            .map_err(|_| Failure::Containment)?
            .is_some()
        {
            return Err(Failure::Containment);
        }
        timely(deadline)?;
        // Keep this typed native handle until OS process teardown, rather than dropping it
        // when this worker returns to the entry driver. The outer retained Job still supplies
        // the actual descendant proof; leaking the handle is not a cleanup confirmation.
        let process_lifetime: std::os::windows::io::OwnedHandle = grandchild.into();
        std::mem::forget(process_lifetime);
        Ok(())
    }

    fn grandchild(arguments: &Arguments, deadline: Instant) -> Result<()> {
        identity(arguments, "grandchild", deadline)?;
        let mut marker = checked_native(deadline, || {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(arguments.control.join("grandchild.ready"))
        })?;
        checked_native(deadline, || marker.write_all(READY))?;
        checked_native(deadline, || marker.sync_all())?;
        timely(deadline)?;
        drop(marker);
        loop {
            timely(deadline)?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    const GUARD: &str = "locron.windows-user-guard/v1";
    const ADMIN: &str = "S-1-5-32-544";
    const SYSTEM: &str = "S-1-5-18";
    const INSTALLER: &str = "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464";
    const MUTATION: u32 = 0x500d_0156;
    const DIRECTORY_ACCESS: u32 = 0x0002_0081;
    const DIRECTORY_FLAGS: u32 = 0x0220_0000;
    const GUARD_TOKEN_PROBE: &str = r"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
try {
    $principal = [Security.Principal.WindowsPrincipal]::new($identity)
    [pscustomobject]@{
        sid = $identity.User.Value
        owner_sid = $identity.Owner.Value
        administrative = $principal.IsInRole([Security.Principal.SecurityIdentifier]::new('S-1-5-32-544'))
    } | & $locronToJson -Compress
} finally { $identity.Dispose() }
";

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GuardRequest {
        schema: String,
        nonce: String,
        seq: u8,
        op: String,
        remaining_ms: u32,
        payload: GuardPayload,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum GuardPayload {
        Acquire(Acquire),
        Job(NewJob),
        Controls(NewControls),
        Image(ImageHash),
        Empty(Empty),
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Acquire {
        anchor: String,
        source_image: String,
        source_sha256: String,
        cwd: String,
        controller_remaining_ms: u32,
        setup_remaining_ms: u32,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NewJob {
        name: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct NewControls {
        actors: [ActorSid; 2],
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ActorSid {
        label: String,
        sid: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ImageHash {
        sha256: String,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Empty {}

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct GuardToken {
        sid: String,
        owner_sid: String,
        administrative: bool,
    }

    #[derive(Serialize)]
    struct GuardAck<'a, T: Serialize> {
        schema: &'static str,
        nonce: &'a str,
        seq: u8,
        op: &'static str,
        ok: bool,
        payload: T,
    }

    #[derive(Serialize)]
    struct AnchorAck<'a> {
        pid: u32,
        runner_sid: &'a str,
        source_sha256: &'a str,
    }
    #[derive(Serialize)]
    struct CreatedAck {
        created: bool,
    }
    #[derive(Serialize)]
    struct ControlsAck {
        created: u8,
    }
    #[derive(Serialize)]
    struct HashAck<'a> {
        sha256: &'a str,
    }
    #[derive(Serialize)]
    struct ReleasedAck {
        released: bool,
    }
    #[derive(Serialize)]
    struct ReleasedControlsAck {
        released: u8,
    }
    #[derive(Serialize)]
    struct FailedAck {
        failure: &'static str,
    }

    struct GuardState {
        anchor: Option<DirectoryGuard>,
        source_parent: Option<DirectoryGuard>,
        cwd: Option<DirectoryGuard>,
        source: Option<File>,
        job: Option<File>,
        controls: Vec<File>,
        image: Option<File>,
        // A successful exclusive create enters this ledger BEFORE any subsequent native call.
        // Failure retains it; a partial object never becomes an adopted cleanup authority.
        created: Vec<PathBuf>,
        anchor_path: PathBuf,
        job_path: PathBuf,
        primary: String,
        default_owner: String,
        actors: Vec<String>,
    }

    impl GuardState {
        fn new() -> Self {
            Self {
                anchor: None,
                source_parent: None,
                cwd: None,
                source: None,
                job: None,
                controls: Vec::new(),
                image: None,
                created: Vec::new(),
                anchor_path: PathBuf::new(),
                job_path: PathBuf::new(),
                primary: String::new(),
                default_owner: String::new(),
                actors: Vec::new(),
            }
        }
    }

    fn canonical_sid(text: &str) -> bool {
        let parts: Vec<_> = text.split('-').collect();
        text.len() <= 256
            && text.is_ascii()
            && (4..=18).contains(&parts.len())
            && parts[0] == "S"
            && parts[1] == "1"
            && parts[2..].iter().enumerate().all(|(index, part)| {
                !part.is_empty()
                    && !(part.len() > 1 && part.starts_with('0'))
                    && part.bytes().all(|byte| byte.is_ascii_digit())
                    && if index == 0 {
                        part.parse::<u64>().is_ok()
                    } else {
                        part.parse::<u32>().is_ok()
                    }
            })
    }

    fn hex(text: &str, length: usize) -> bool {
        text.len() == length
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }

    fn local_path(text: &str) -> Result<PathBuf> {
        use std::os::windows::ffi::OsStrExt as _;
        if text.encode_utf16().count() > 512 {
            return Err(Failure::Protocol);
        }
        let path = PathBuf::from(text);
        let mut components = path.components();
        let Some(Component::Prefix(prefix)) = components.next() else {
            return Err(Failure::Protocol);
        };
        let (Prefix::Disk(drive) | Prefix::VerbatimDisk(drive)) = prefix.kind() else {
            return Err(Failure::Protocol);
        };
        if components.next() != Some(Component::RootDir) {
            return Err(Failure::Protocol);
        }
        let mut normalized = PathBuf::from(format!(r"\\?\{}:\", char::from(drive)));
        for component in components {
            let Component::Normal(name) = component else {
                return Err(Failure::Protocol);
            };
            let value = name.to_str().ok_or(Failure::Protocol)?;
            let stem = value
                .split('.')
                .next()
                .unwrap_or_default()
                .to_ascii_uppercase();
            if value.is_empty()
                || value.ends_with(['.', ' '])
                || value.contains('~')
                || value.chars().any(|character| {
                    character <= '\u{1f}'
                        || matches!(
                            character,
                            '<' | '>' | ':' | '"' | '|' | '?' | '*' | '/' | '\\'
                        )
                })
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit())
                || name.encode_wide().any(|unit| unit == 0)
            {
                return Err(Failure::Protocol);
            }
            normalized.push(name);
        }
        Ok(normalized)
    }

    fn checked_native<T>(
        deadline: Instant,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> Result<T> {
        timely(deadline)?;
        let result = operation();
        if timely(deadline).is_err() {
            // The uncancellable operation has returned late. Keep its exact result and
            // the full caller stack/guards; the driver may not accept or replay it.
            retain_unknown(result);
        }
        result.map_err(|_| Failure::Native)
    }

    fn native_sid(value: &Sid, deadline: Instant) -> Result<String> {
        let text = checked_native(deadline, || wrappers::ConvertSidToStringSid(value))?
            .into_string()
            .map_err(|_| Failure::Native)?;
        timely(deadline)?;
        if !canonical_sid(&text) {
            return Err(Failure::Native);
        }
        Ok(text)
    }

    fn security(file: &File, deadline: Instant) -> Result<LocalBox<SecurityDescriptor>> {
        checked_native(deadline, || {
            wrappers::GetSecurityInfo(
                file,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Owner | SecurityInformation::Dacl,
            )
        })
    }

    fn shape(file: &File, directory: bool, deadline: Instant) -> Result<()> {
        let metadata = checked_native(deadline, || file.metadata())?;
        if metadata.file_attributes() & 0x400 != 0
            || metadata.is_dir() != directory
            || (!directory && !metadata.is_file())
        {
            return Err(Failure::Native);
        }
        Ok(())
    }

    fn directory_file(path: &Path, initialize: bool, deadline: Instant) -> Result<File> {
        let file = checked_native(deadline, || {
            OpenOptions::new()
                .access_mode(DIRECTORY_ACCESS | if initialize { 0x000c_0000 } else { 0 })
                .share_mode(3)
                .custom_flags(DIRECTORY_FLAGS)
                .open(path)
        })?;
        shape(&file, true, deadline)?;
        Ok(file)
    }

    fn fixed_descriptor(file: &File, directory: bool, deadline: Instant) -> Result<()> {
        shape(file, directory, deadline)?;
        let descriptor = security(file, deadline)?;
        let owner = native_sid(descriptor.owner().ok_or(Failure::Native)?, deadline)?;
        if ![SYSTEM, ADMIN, INSTALLER].contains(&owner.as_str()) {
            return Err(Failure::Native);
        }
        let acl = descriptor.dacl().ok_or(Failure::Native)?;
        for index in 0..acl.len() {
            let ace = acl.get_ace(index).ok_or(Failure::Native)?;
            if !matches!(
                ace.ace_type(),
                AceType::ACCESS_ALLOWED_ACE_TYPE | AceType::ACCESS_DENIED_ACE_TYPE
            ) {
                return Err(Failure::Native);
            }
            let principal = native_sid(ace.sid().ok_or(Failure::Native)?, deadline)?;
            if ace.ace_type() == AceType::ACCESS_ALLOWED_ACE_TYPE
                && ace.flags().bits() & 0x08 == 0
                && ![SYSTEM, ADMIN, INSTALLER].contains(&principal.as_str())
                && ace.mask().bits()
                    & if directory {
                        MUTATION & !0x06
                    } else {
                        MUTATION
                    }
                    != 0
            {
                return Err(Failure::Native);
            }
        }
        timely(deadline)
    }

    fn fixed_chain(path: &Path, deadline: Instant) -> Result<DirectoryGuard> {
        let guard = checked_native(deadline, || DirectoryGuard::ancestors(path))?;
        // The public guard permits its current SID. This fixture ADDITIONALLY requires only
        // fixed privileged owners/writers, checking every exact ancestor while that chain lives.
        let mut current = PathBuf::new();
        for component in guard.normalized_path().components() {
            current.push(component.as_os_str());
            if matches!(component, Component::Prefix(_)) {
                continue;
            }
            let file = directory_file(&current, false, deadline)?;
            fixed_descriptor(&file, true, deadline)?;
        }
        Ok(guard)
    }

    fn inherited_safe(
        file: &File,
        token_owner: &str,
        initial: bool,
        deadline: Instant,
    ) -> Result<()> {
        let descriptor = security(file, deadline)?;
        let owner = native_sid(descriptor.owner().ok_or(Failure::Native)?, deadline)?;
        if ![SYSTEM, ADMIN, token_owner].contains(&owner.as_str()) {
            return Err(Failure::Native);
        }
        let acl = descriptor.dacl().ok_or(Failure::Native)?;
        for index in 0..acl.len() {
            let ace = acl.get_ace(index).ok_or(Failure::Native)?;
            if !matches!(
                ace.ace_type(),
                AceType::ACCESS_ALLOWED_ACE_TYPE | AceType::ACCESS_DENIED_ACE_TYPE
            ) {
                return Err(Failure::Native);
            }
            let mut principal = native_sid(ace.sid().ok_or(Failure::Native)?, deadline)?;
            if principal == "S-1-3-1" {
                return Err(Failure::Native);
            }
            if principal == "S-1-3-0" {
                principal = token_owner.to_owned();
            }
            if ace.ace_type() == AceType::ACCESS_ALLOWED_ACE_TYPE
                && (initial || ace.flags().bits() & 0x03 != 0)
                && ace.mask().bits() & MUTATION != 0
                && ![SYSTEM, ADMIN, token_owner].contains(&principal.as_str())
            {
                return Err(Failure::Native);
            }
        }
        timely(deadline)
    }

    fn final_directory(file: &File, actor: Option<&str>, deadline: Instant) -> Result<()> {
        shape(file, true, deadline)?;
        let descriptor = security(file, deadline)?;
        if native_sid(descriptor.owner().ok_or(Failure::Native)?, deadline)? != ADMIN {
            return Err(Failure::Native);
        }
        let text = checked_native(deadline, || {
            wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
                &descriptor,
                SecurityInformation::Dacl,
            )
        })?;
        if !text
            .to_string_lossy()
            .strip_prefix("D:")
            .and_then(|value| value.split('(').next())
            .is_some_and(|flags| flags.contains('P'))
        {
            return Err(Failure::Native);
        }
        let acl = descriptor.dacl().ok_or(Failure::Native)?;
        let expected = if actor.is_some() { 3 } else { 2 };
        if acl.len() != expected {
            return Err(Failure::Native);
        }
        let mut principals = Vec::new();
        for index in 0..acl.len() {
            let ace = acl.get_ace(index).ok_or(Failure::Native)?;
            let principal = native_sid(ace.sid().ok_or(Failure::Native)?, deadline)?;
            if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
                || ace.flags().bits() != 0x03
                || ace.mask().bits() != 0x001f_01ff
                || !(principal == SYSTEM || principal == ADMIN || actor == Some(principal.as_str()))
                || principals.contains(&principal)
            {
                return Err(Failure::Native);
            }
            principals.push(principal);
        }
        Ok(())
    }

    fn empty(path: &Path, deadline: Instant) -> Result<()> {
        let mut entries = checked_native(deadline, || std::fs::read_dir(path))?;
        if checked_native(deadline, || entries.next().transpose())?.is_some() {
            return Err(Failure::Native);
        }
        timely(deadline)
    }

    fn create_directory(
        state: &mut GuardState,
        path: PathBuf,
        actor: Option<&str>,
        deadline: Instant,
    ) -> Result<File> {
        let parent = directory_file(path.parent().ok_or(Failure::Native)?, false, deadline)?;
        inherited_safe(&parent, &state.default_owner, false, deadline)?;
        // Record create Ok immediately, including a LATE Ok. Post-gate refusal retains this
        // ledger and all guards; it cannot adopt/remove an unconfirmed newly created object.
        timely(deadline)?;
        std::fs::create_dir(&path).map_err(|_| Failure::Native)?;
        state.created.push(path.clone());
        timely(deadline)?;
        let mut file = directory_file(&path, true, deadline)?;
        inherited_safe(&file, &state.default_owner, true, deadline)?;
        empty(&path, deadline)?;
        let extra = actor.map_or_else(String::new, |actor| format!("(A;OICI;FA;;;{actor})"));
        let descriptor: LocalBox<SecurityDescriptor> = checked_native(deadline, || {
            format!("O:BAD:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY){extra}").parse()
        })?;
        checked_native(deadline, || {
            wrappers::SetSecurityInfo(
                &mut file,
                SeObjectType::SE_FILE_OBJECT,
                SecurityInformation::Owner
                    | SecurityInformation::Dacl
                    | SecurityInformation::ProtectedDacl,
                descriptor.owner(),
                None,
                descriptor.dacl(),
                None,
            )
        })?;
        final_directory(&file, actor, deadline)?;
        empty(&path, deadline)?;
        Ok(file)
    }

    // Hash only the exact held regular build/image handle. Its duplicate shares the cursor;
    // these fresh source/image handles are hashed once, with no concurrent reader.
    fn hash_file(file: &File, deadline: Instant) -> Result<String> {
        let mut reader = checked_native(deadline, || file.try_clone())?;
        checked_native(deadline, || reader.seek(SeekFrom::Start(0)))?;
        timely(deadline)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 65_536];
        loop {
            let count = checked_native(deadline, || reader.read(&mut buffer))?;
            if count == 0 {
                break;
            }
            timely(deadline)?;
            hasher.update(&buffer[..count]);
            timely(deadline)?;
        }
        timely(deadline)?;
        let digest = hasher.finalize();
        timely(deadline)?;
        let result = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        timely(deadline)?;
        Ok(result)
    }

    fn regular(path: &Path, deadline: Instant) -> Result<File> {
        let file = checked_native(deadline, || {
            OpenOptions::new()
                .read(true)
                .share_mode(1)
                .custom_flags(0x0020_0000)
                .open(path)
        })?;
        shape(&file, false, deadline)?;
        Ok(file)
    }

    fn image_descriptor(file: &File, actors: &[String], deadline: Instant) -> Result<()> {
        shape(file, false, deadline)?;
        let descriptor = security(file, deadline)?;
        if native_sid(descriptor.owner().ok_or(Failure::Native)?, deadline)? != ADMIN {
            return Err(Failure::Native);
        }
        let sddl = checked_native(deadline, || {
            wrappers::ConvertSecurityDescriptorToStringSecurityDescriptor(
                &descriptor,
                SecurityInformation::Dacl,
            )
        })?;
        if !sddl
            .to_string_lossy()
            .strip_prefix("D:")
            .and_then(|value| value.split('(').next())
            .is_some_and(|flags| flags.contains('P'))
        {
            return Err(Failure::Native);
        }
        let acl = descriptor.dacl().ok_or(Failure::Native)?;
        if acl.len() != 4 {
            return Err(Failure::Native);
        }
        let mut seen = Vec::new();
        for index in 0..acl.len() {
            let ace = acl.get_ace(index).ok_or(Failure::Native)?;
            let principal = native_sid(ace.sid().ok_or(Failure::Native)?, deadline)?;
            let mask = if [ADMIN, SYSTEM].contains(&principal.as_str()) {
                0x001f_01ff
            } else if actors.contains(&principal) {
                0x0012_00a9
            } else {
                return Err(Failure::Native);
            };
            if ace.ace_type() != AceType::ACCESS_ALLOWED_ACE_TYPE
                || ace.flags().bits() != 0
                || ace.mask().bits() != mask
                || seen.contains(&principal)
            {
                return Err(Failure::Native);
            }
            seen.push(principal);
        }
        timely(deadline)
    }

    fn guard_frame(
        input: &mut impl Read,
        total: &mut usize,
        deadline: Instant,
    ) -> Result<GuardRequest> {
        let mut bytes = Vec::with_capacity(4096);
        loop {
            let mut byte = [0_u8; 1];
            checked_native(deadline, || input.read_exact(&mut byte))?;
            *total += 1;
            bytes.push(byte[0]);
            if *total > 32768 || bytes.len() > 4096 {
                return Err(Failure::Protocol);
            }
            if byte[0] == b'\n' {
                break;
            }
        }
        // serde's struct visitors reject duplicates as well as unknown keys; Value is never
        // used to decode the wire, including its untagged exact payload struct variants.
        let frame: GuardRequest = serde_json::from_slice(&bytes).map_err(|_| Failure::Protocol)?;
        if frame.schema != GUARD
            || !hex(&frame.nonce, 32)
            || frame.remaining_ms == 0
            || frame.remaining_ms > 180000
        {
            return Err(Failure::Protocol);
        }
        timely(deadline)?;
        Ok(frame)
    }

    fn guard_ack<T: Serialize>(
        frame: &GuardRequest,
        op: &'static str,
        payload: T,
        output: &mut impl Write,
        total: &mut usize,
        deadline: Instant,
    ) -> Result<()> {
        timely(deadline)?;
        let mut bytes = serde_json::to_vec(&GuardAck {
            schema: GUARD,
            nonce: &frame.nonce,
            seq: frame.seq,
            op,
            ok: op != "failed",
            payload,
        })
        .map_err(|_| Failure::Protocol)?;
        bytes.push(b'\n');
        *total += bytes.len();
        if bytes.len() > 4096 || *total > 32768 {
            return Err(Failure::Protocol);
        }
        checked_native(deadline, || output.write_all(&bytes))?;
        checked_native(deadline, || output.flush())
    }

    fn elapsed_cap(entered: Instant, deadline: Instant, driver: &AtomicU64) -> Result<()> {
        let duration = deadline.saturating_duration_since(entered);
        let millis = u64::try_from(duration.as_millis()).map_err(|_| Failure::Deadline)?;
        if millis == 0 {
            return Err(Failure::Deadline);
        }
        driver.store(millis, Ordering::Release);
        timely(deadline)
    }

    fn guard_owner(entered: Instant, driver: Arc<AtomicU64>) -> Result<()> {
        let mut state = GuardState::new();
        let mut input = io::stdin().lock();
        let mut output = io::stdout().lock();
        let mut received = 0;
        let mut sent = 0;
        let mut nonce = String::new();
        let mut deadline = entered + Duration::from_secs(30);
        let mut controller = entered + Duration::from_secs(180);
        for sequence in 0..8 {
            let frame = match guard_frame(&mut input, &mut received, deadline) {
                Ok(frame) => frame,
                Err(_) => retain_unknown(state),
            };
            if frame.seq != sequence || (!nonce.is_empty() && frame.nonce != nonce) {
                retain_unknown(state);
            }
            if nonce.is_empty() {
                nonce = frame.nonce.clone();
            }
            let Some(request_cap) =
                Instant::now().checked_add(Duration::from_millis(u64::from(frame.remaining_ms)))
            else {
                retain_unknown(state);
            };
            deadline = deadline.min(request_cap);
            if elapsed_cap(entered, deadline, &driver).is_err() {
                retain_unknown(state);
            }
            let result = (|| {
                match (sequence, frame.op.as_str(), &frame.payload) {
                    (0, "acquire_anchor", GuardPayload::Acquire(payload)) => {
                        if payload.controller_remaining_ms == 0
                            || payload.controller_remaining_ms > 180000
                            || payload.setup_remaining_ms == 0
                            || payload.setup_remaining_ms > 30000
                            || !hex(&payload.source_sha256, 64)
                        {
                            return Err(Failure::Protocol);
                        }
                        controller = entered
                            + Duration::from_millis(u64::from(payload.controller_remaining_ms));
                        deadline = deadline.min(controller).min(
                            entered + Duration::from_millis(u64::from(payload.setup_remaining_ms)),
                        );
                        elapsed_cap(entered, deadline, &driver)?;
                        state.primary = sid(deadline)?;
                        let probe = locron_core::windows::run_script_json_until(
                            GUARD_TOKEN_PROBE,
                            &serde_json::json!({}),
                            deadline,
                        )
                        .map_err(|_| Failure::Token)?;
                        timely(deadline)?;
                        let probe: GuardToken =
                            serde_json::from_value(probe).map_err(|_| Failure::Protocol)?;
                        if !probe.administrative
                            || probe.sid != state.primary
                            || !canonical_sid(&probe.owner_sid)
                            || ![ADMIN, SYSTEM, state.primary.as_str()]
                                .contains(&probe.owner_sid.as_str())
                        {
                            return Err(Failure::Token);
                        }
                        state.default_owner = probe.owner_sid;
                        let anchor = local_path(&payload.anchor)?;
                        let source = local_path(&payload.source_image)?;
                        let cwd = local_path(&payload.cwd)?;
                        state.anchor = Some(fixed_chain(&anchor, deadline)?);
                        state.source_parent = Some(fixed_chain(
                            source.parent().ok_or(Failure::Protocol)?,
                            deadline,
                        )?);
                        state.cwd = Some(fixed_chain(&cwd, deadline)?);
                        state.source = Some(regular(&source, deadline)?);
                        let source_file = state.source.as_ref().ok_or(Failure::Native)?;
                        fixed_descriptor(source_file, false, deadline)?;
                        if hash_file(source_file, deadline)? != payload.source_sha256
                            || checked_native(deadline, std::env::current_exe).and_then(|path| {
                                local_path(path.to_str().ok_or(Failure::Native)?)
                            })? != source
                        {
                            return Err(Failure::Native);
                        }
                        state.anchor_path = state
                            .anchor
                            .as_ref()
                            .ok_or(Failure::Native)?
                            .normalized_path()
                            .to_owned();
                        guard_ack(
                            &frame,
                            "anchor_held",
                            AnchorAck {
                                pid: std::process::id(),
                                runner_sid: &state.primary,
                                source_sha256: &payload.source_sha256,
                            },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (1, "create_job", GuardPayload::Job(payload)) => {
                        if payload.name.len() != 55
                            || !payload.name.starts_with("locron-prerequisite-v1-")
                            || !hex(&payload.name[23..], 32)
                        {
                            return Err(Failure::Protocol);
                        }
                        let path = state.anchor_path.join(&payload.name);
                        let file = create_directory(&mut state, path.clone(), None, deadline)?;
                        state.job_path = path;
                        state.job = Some(file);
                        guard_ack(
                            &frame,
                            "job_held",
                            CreatedAck { created: true },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (2, "create_controls", GuardPayload::Controls(payload)) => {
                        if payload.actors[0].label != "A"
                            || payload.actors[1].label != "B"
                            || payload.actors[0].sid == payload.actors[1].sid
                            || payload.actors.iter().any(|actor| {
                                !canonical_sid(&actor.sid)
                                    || actor.sid == state.primary
                                    || [ADMIN, SYSTEM, INSTALLER].contains(&actor.sid.as_str())
                            })
                        {
                            return Err(Failure::Protocol);
                        }
                        for actor in &payload.actors {
                            let path = state.job_path.join(&actor.label);
                            let file =
                                create_directory(&mut state, path, Some(&actor.sid), deadline)?;
                            state.controls.push(file);
                            state.actors.push(actor.sid.clone());
                        }
                        guard_ack(
                            &frame,
                            "controls_held",
                            ControlsAck { created: 2 },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (3, "hold_image", GuardPayload::Image(payload)) => {
                        if !hex(&payload.sha256, 64) {
                            return Err(Failure::Protocol);
                        }
                        state.image = Some(regular(
                            &state.job_path.join("windows_user_prerequisite.exe"),
                            deadline,
                        )?);
                        let file = state.image.as_ref().ok_or(Failure::Native)?;
                        image_descriptor(file, &state.actors, deadline)?;
                        if hash_file(file, deadline)? != payload.sha256 {
                            return Err(Failure::Native);
                        }
                        guard_ack(
                            &frame,
                            "image_held",
                            HashAck {
                                sha256: &payload.sha256,
                            },
                            &mut output,
                            &mut sent,
                            deadline,
                        )?;
                        // Residence is inside the original controller cap, not a renewed setup.
                        deadline = controller;
                        elapsed_cap(entered, deadline, &driver)
                    }
                    (4, "release_image", GuardPayload::Empty(_)) => {
                        drop(state.image.take());
                        guard_ack(
                            &frame,
                            "image_released",
                            ReleasedAck { released: true },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (5, "release_controls", GuardPayload::Empty(_)) => {
                        state.controls.clear();
                        guard_ack(
                            &frame,
                            "controls_released",
                            ReleasedControlsAck { released: 2 },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (6, "release_job", GuardPayload::Empty(_)) => {
                        drop(state.job.take());
                        guard_ack(
                            &frame,
                            "job_released",
                            ReleasedAck { released: true },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    (7, "finish", GuardPayload::Empty(_)) => {
                        let mut extra = [0_u8; 1];
                        if checked_native(deadline, || input.read(&mut extra))? != 0 {
                            return Err(Failure::Protocol);
                        }
                        drop(state.source.take());
                        drop(state.source_parent.take());
                        drop(state.cwd.take());
                        drop(state.anchor.take());
                        guard_ack(
                            &frame,
                            "all_released",
                            ReleasedAck { released: true },
                            &mut output,
                            &mut sent,
                            deadline,
                        )
                    }
                    _ => Err(Failure::Protocol),
                }
            })();
            if let Err(failure) = result {
                let category = match failure {
                    Failure::Deadline => "deadline",
                    Failure::Protocol => "protocol",
                    Failure::Token => "token",
                    _ => "guard_setup",
                };
                let _ = guard_ack(
                    &frame,
                    "failed",
                    FailedAck { failure: category },
                    &mut output,
                    &mut sent,
                    deadline,
                );
                retain_unknown(state);
            }
        }
        timely(deadline)
    }

    fn guard_entry(entered: Instant) -> ExitCode {
        let driver = Arc::new(AtomicU64::new(30000));
        let owner_deadline = Arc::clone(&driver);
        let (sender, receiver) = mpsc::sync_channel(1);
        let owner = std::thread::Builder::new()
            .name("windows-prerequisite-guard-owner".into())
            .spawn(move || {
                let result = guard_owner(entered, owner_deadline);
                let _ = sender.send(result);
            });
        let Ok(owner) = owner else {
            return ExitCode::FAILURE;
        };
        loop {
            let deadline = entered + Duration::from_millis(driver.load(Ordering::Acquire));
            if timely(deadline).is_err() {
                retain_unknown(owner);
            }
            match receiver.recv_timeout(
                Duration::from_millis(5).min(deadline.saturating_duration_since(Instant::now())),
            ) {
                Ok(Ok(())) if timely(deadline).is_ok() => return ExitCode::SUCCESS,
                Ok(Err(_)) if timely(deadline).is_ok() => return ExitCode::FAILURE,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                _ => retain_unknown(owner),
            }
        }
    }

    pub(super) fn entry(entered: Instant) -> ExitCode {
        let arguments: Vec<_> = std::env::args_os().skip(1).collect();
        if arguments.len() == 2
            && arguments[0] == "--hosted-prerequisite"
            && arguments[1] == "guard-admin"
        {
            if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
                || std::env::var("LOCRON_HOSTED_PREREQUISITE").as_deref() != Ok("1")
                || std::env::var("LOCRON_RUNNER_ENVIRONMENT").as_deref() != Ok("github-hosted")
            {
                return ExitCode::FAILURE;
            }
            return guard_entry(entered);
        }
        let Ok(arguments) = parse() else {
            return ExitCode::FAILURE;
        };
        let Some(deadline) = entered.checked_add(arguments.remaining) else {
            return ExitCode::FAILURE;
        };
        if timely(deadline).is_err() {
            return ExitCode::FAILURE;
        }
        let (sender, receiver) = mpsc::sync_channel(1);
        let owner = std::thread::Builder::new()
            .name("windows-prerequisite-native-owner".into())
            .spawn(move || {
                let result = match arguments.role {
                    Role::Outer => tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| Failure::Native)
                        .and_then(|runtime| runtime.block_on(outer(&arguments, deadline))),
                    Role::Inner => inner(&arguments, deadline),
                    Role::Grandchild => grandchild(&arguments, deadline),
                };
                let _ = sender.send(result);
            });
        let Ok(owner) = owner else {
            return ExitCode::FAILURE;
        };
        match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(Ok(())) if timely(deadline).is_ok() => ExitCode::SUCCESS,
            Ok(Err(_)) if timely(deadline).is_ok() => ExitCode::FAILURE,
            _ => retain_unknown(owner),
        }
    }
}
