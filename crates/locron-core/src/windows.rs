//! Narrow, fixed Windows filesystem adapters shared by the workspace.

use std::io;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use base64::Engine;
use serde_json::{Value, json};

const ADAPTER_TIMEOUT: Duration = Duration::from_secs(30);
const OUTPUT_LIMIT: u64 = 128 * 1024;
static USER_SID: OnceLock<String> = OnceLock::new();

/// Returns the absolute stock Windows PowerShell 5.1 executable.
pub fn stock_powershell() -> io::Result<PathBuf> {
    // SystemRoot is supplied by Windows, but do not accept a relative override.
    let root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|root| root.is_absolute())
        .ok_or_else(|| io::Error::other("Windows SystemRoot is unavailable"))?;
    Ok(root.join("System32/WindowsPowerShell/v1.0/powershell.exe"))
}

/// Runs a fixed, caller-reviewed adapter with JSON input and bounded JSON output.
///
/// Source must be a static string. Paths and options belong in `input`, never in source.
pub fn run_script_json(script: &'static str, input: &Value) -> io::Result<Value> {
    run_script_with_timeout(script, input, ADAPTER_TIMEOUT)
}

fn run_script_with_timeout(
    script: &'static str,
    input: &Value,
    timeout: Duration,
) -> io::Result<Value> {
    let request = serde_json::to_vec(input).map_err(io::Error::other)?;
    if request.len() > 64 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Windows adapter input is too large",
        ));
    }
    let executable = stock_powershell()?;
    let source = format!(
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; [Console]::InputEncoding = [Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); try {{ $request = [Console]::In.ReadToEnd() | ConvertFrom-Json; {script} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}"
    );
    let encoded = base64::engine::general_purpose::STANDARD.encode(
        source
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    // A dedicated thread permits callers already inside Tokio; no async type crosses this API.
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(run_adapter(executable, encoded, request, timeout))
    })
    .join()
    .map_err(|_| io::Error::other("Windows adapter worker failed"))?
}

async fn run_adapter(
    executable: PathBuf,
    encoded: String,
    request: Vec<u8>,
    timeout: Duration,
) -> io::Result<Value> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let deadline = tokio::time::Instant::now() + timeout;
    let mut child = tokio::process::Command::new(executable)
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded])
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW; no execution-policy changes.
        .kill_on_drop(true)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing adapter stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("missing adapter stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("missing adapter stderr"))?;
    let mut writer = tokio::spawn(async move { stdin.write_all(&request).await });
    let mut output = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await?;
        Ok::<_, io::Error>(bytes)
    });
    let mut errors = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr
            .take(OUTPUT_LIMIT + 1)
            .read_to_end(&mut bytes)
            .await?;
        Ok::<_, io::Error>(bytes)
    });
    let operation = tokio::time::timeout_at(deadline, async {
        let ((), output, errors, status) = tokio::try_join!(
            async { (&mut writer).await.map_err(io::Error::other)? },
            async { (&mut output).await.map_err(io::Error::other)? },
            async { (&mut errors).await.map_err(io::Error::other)? },
            child.wait(),
        )?;
        Ok::<_, io::Error>((output, errors, status))
    })
    .await;
    let (output, errors, status) = match operation {
        Ok(Ok(result)) => result,
        failed => {
            // Cancel pipe tasks, then kill and reap the owned adapter under a second bound.
            writer.abort();
            output.abort();
            errors.abort();
            let _ = child.start_kill();
            let cleanup = tokio::time::timeout(Duration::from_secs(3), async {
                if !writer.is_finished() {
                    let _ = (&mut writer).await;
                }
                if !output.is_finished() {
                    let _ = (&mut output).await;
                }
                if !errors.is_finished() {
                    let _ = (&mut errors).await;
                }
                child.wait().await
            })
            .await;
            if !matches!(cleanup, Ok(Ok(_))) {
                return Err(io::Error::other(
                    "could not confirm Windows adapter termination",
                ));
            }
            return match failed {
                Ok(Err(error)) => Err(error),
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "stock Windows adapter timed out",
                )),
                Ok(Ok(_)) => unreachable!(),
            };
        }
    };
    if output.len() > OUTPUT_LIMIT as usize || errors.len() > OUTPUT_LIMIT as usize {
        return Err(io::Error::other(
            "Windows adapter exceeded its output limit",
        ));
    }
    if !status.success() {
        return Err(io::Error::other(format!(
            "stock Windows adapter failed: {}",
            String::from_utf8_lossy(&errors).trim()
        )));
    }
    serde_json::from_slice(&output).map_err(io::Error::other)
}

/// Returns the actual current token's SID, independent of username environment text.
pub fn current_user_sid() -> io::Result<String> {
    if let Some(sid) = USER_SID.get() {
        return Ok(sid.clone());
    }
    let result = run_script_json(
        "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value | ConvertTo-Json -Compress",
        &json!({}),
    )?;
    let sid = result
        .as_str()
        .filter(|sid| {
            sid.starts_with("S-1-")
                && sid
                    .split('-')
                    .skip(1)
                    .all(|part| part.parse::<u64>().is_ok())
        })
        .ok_or_else(|| io::Error::other("Windows identity adapter returned an invalid SID"))?
        .to_owned();
    let _ = USER_SID.set(sid.clone());
    Ok(sid)
}

pub(crate) fn create_private_directory(path: &std::path::Path) -> io::Result<()> {
    run_script_json(
        r"
        $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User;
        $acl = [System.Security.AccessControl.DirectorySecurity]::new();
        $acl.SetOwner($sid); $acl.SetAccessRuleProtection($true, $false);
        $inherit = [System.Security.AccessControl.InheritanceFlags]'ContainerInherit,ObjectInherit';
        foreach ($principal in @($sid, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
            $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($principal, [System.Security.AccessControl.FileSystemRights]::FullControl, $inherit, [System.Security.AccessControl.PropagationFlags]::None, [System.Security.AccessControl.AccessControlType]::Allow);
            $acl.AddAccessRule($rule);
        }
        [System.IO.DirectoryInfo]::new([string]$request.path).Create($acl);
        @{created=$true} | ConvertTo-Json -Compress
    ",
        &json!({"path": path}),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_rejects_large_input_before_spawn() {
        assert!(
            run_script_json(
                "@{} | ConvertTo-Json -Compress",
                &json!({"large": "x".repeat(70 * 1024)})
            )
            .is_err()
        );
    }

    #[test]
    fn adapter_startup_and_script_wait_are_bounded() {
        let started = std::time::Instant::now();
        let result = run_script_with_timeout(
            "Start-Sleep -Seconds 60; @{} | ConvertTo-Json -Compress",
            &json!({"input": "x".repeat(60 * 1024)}),
            Duration::from_millis(100),
        );
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn adapter_output_is_bounded() {
        assert!(run_script_json("('x' * 200000) | ConvertTo-Json -Compress", &json!({})).is_err());
    }
}
