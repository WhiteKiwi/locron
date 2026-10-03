//! Test-only bounded phase visibility for the actual generic adapter.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const PREFIX: &str = "locron-generic/v1/";
const PHASES: [&str; 11] = [
    "source-entry",
    "encoding-ready",
    "policy-confirmed",
    "binding-start",
    "binding-ready",
    "input-complete",
    "json-start",
    "json-parsed",
    "caller-start",
    "caller-complete",
    "catch",
];

pub(super) fn token(phase: &str) -> String {
    format!("[Console]::Error.WriteLine('{PREFIX}{phase}');")
}

pub(super) fn source(script: &str) -> String {
    source_with_policy(script, super::loader_tests::restricted_child())
}

fn source_with_policy(script: &str, restricted: bool) -> String {
    // The same compiled binary-only bootstrap and retained command objects run in both builds.
    // Only test builds split the EOF/JSON pipeline and emit fixed phase tokens.
    format!(
        "{} $ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; [Console]::InputEncoding = [Text.UTF8Encoding]::new($false); [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); {} try {{ {} {} {}\n{} $locronInput = [Console]::In.ReadToEnd(); {} {} $request = $locronInput | & $locronFromJson; {} {} {script}\n; {} }} catch {{ {} [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}",
        token("source-entry"),
        token("encoding-ready"),
        if restricted {
            format!(
                "{}\n{}",
                super::loader_tests::POLICY_ASSERTION,
                token("policy-confirmed")
            )
        } else {
            String::new()
        },
        token("binding-start"),
        super::STOCK_JSON_BOOTSTRAP,
        token("binding-ready"),
        token("input-complete"),
        token("json-start"),
        token("json-parsed"),
        token("caller-start"),
        token("caller-complete"),
        token("catch")
    )
}

pub(super) struct Trace {
    entered: Instant,
    stages: Mutex<VecDeque<(&'static str, u32, u128)>>,
}

impl Trace {
    pub(super) fn stage_value(&self, expected: &str) -> Option<u32> {
        self.stages
            .lock()
            .unwrap()
            .iter()
            .find(|(phase, _, _)| *phase == expected)
            .map(|(_, value, _)| *value)
    }

    pub(super) fn new() -> Self {
        Self {
            entered: Instant::now(),
            stages: Mutex::new(VecDeque::with_capacity(24)),
        }
    }

    pub(super) fn record(&self, phase: &'static str, pid: u32) {
        if let Ok(mut stages) = self.stages.try_lock() {
            if stages.len() == 24 {
                stages.pop_front();
            }
            stages.push_back((phase, pid, self.entered.elapsed().as_millis()));
        }
    }

    fn line(&self, line: &[u8]) {
        let Ok(line) = std::str::from_utf8(line) else {
            return;
        };
        let Some(phase) = line.trim_end_matches('\r').strip_prefix(PREFIX) else {
            return;
        };
        if let Some(known) = PHASES.iter().copied().find(|known| *known == phase) {
            self.record(known, 0);
        }
    }

    pub(super) fn report(&self, phase: &'static str) {
        self.record(phase, 0);
        if let Ok(stages) = self.stages.try_lock() {
            // Fixed names/counters only. Raw exception stderr stays in the existing error result.
            eprintln!("locron generic adapter phases: {stages:?}");
        }
    }

    #[cfg(test)]
    pub(super) fn child_phases(&self) -> Vec<&'static str> {
        self.stages
            .lock()
            .unwrap()
            .iter()
            .filter(|(phase, _, _)| PHASES.contains(phase))
            .map(|(phase, _, _)| *phase)
            .collect()
    }

    pub(super) fn has_stage(&self, expected: &str) -> bool {
        self.stages
            .lock()
            .unwrap()
            .iter()
            .any(|(phase, _, _)| *phase == expected)
    }
}

pub(super) async fn capture_stderr(
    mut stream: impl tokio::io::AsyncRead + Unpin,
    trace: Arc<Trace>,
    limit: u64,
) -> io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let limit = usize::try_from(limit).map_err(io::Error::other)?;
    let mut bytes = Vec::new();
    let mut line = Vec::with_capacity(80);
    let mut overlong = false;
    let mut buffer = [0; 4096];
    loop {
        let available = (limit + 1 - bytes.len()).min(buffer.len());
        let read = stream.read(&mut buffer[..available]).await?;
        if read == 0 {
            if !overlong {
                trace.line(&line);
            }
            return Ok(bytes);
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > limit {
            return Err(io::Error::other(
                "Windows adapter exceeded its output limit",
            ));
        }
        for byte in &buffer[..read] {
            if *byte == b'\n' {
                if !overlong {
                    trace.line(&line);
                }
                line.clear();
                overlong = false;
            } else if line.len() < 80 {
                line.push(*byte);
            } else {
                overlong = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PREFIX, Trace, capture_stderr, source, source_with_policy, token};
    use std::sync::Arc;

    #[test]
    fn fixed_phases_never_render_inputs_or_promote_unknown_stderr() {
        let trace = Trace::new();
        trace.line(format!("{PREFIX}source-entry").as_bytes());
        trace.line(b"private payload from an exception");
        trace.line(format!("{PREFIX}unknown private payload").as_bytes());
        trace.line(format!("{PREFIX}json-parsed\r").as_bytes());
        assert_eq!(trace.child_phases(), ["source-entry", "json-parsed"]);
        let actual = source("@{ok=$true}|& $locronToJson -Compress");
        for phase in super::PHASES {
            if phase == "policy-confirmed" {
                continue;
            }
            assert_eq!(actual.matches(&token(phase)).count(), 1);
        }
        assert!(actual.contains("$locronInput | & $locronFromJson"));
        assert!(actual.contains(super::super::STOCK_JSON_BOOTSTRAP));
        assert!(!actual.contains("Utility.psd1"));
        assert!(actual.contains(&format!(
            "$locronToJson -Compress\n; {}",
            token("caller-complete")
        )));
        let commented = source("Invoke-Fixture # caller ends with a line comment");
        assert!(commented.contains(&format!(
            "# caller ends with a line comment\n; {}",
            token("caller-complete")
        )));
    }

    #[test]
    fn restricted_assertion_precedes_binding_and_requires_the_actual_enum() {
        let source = source_with_policy("@{} | & $locronToJson -Compress", true);
        let policy = source
            .find(super::super::loader_tests::POLICY_ASSERTION)
            .unwrap();
        assert!(policy < source.find(super::super::STOCK_JSON_BOOTSTRAP).unwrap());
        assert_eq!(source.matches(&token("policy-confirmed")).count(), 1);
        assert!(source.contains("Microsoft.PowerShell.ExecutionPolicy"));
        assert!(source.contains(".Invoke($null, [object[]]@('Microsoft.PowerShell'))"));
        assert!(!source.contains("Set-ExecutionPolicy"));
    }

    #[tokio::test]
    async fn capture_preserves_raw_stderr_and_refuses_limit_plus_one_without_eof() {
        use tokio::io::AsyncWriteExt;
        let trace = Arc::new(Trace::new());
        let (mut writer, reader) = tokio::io::duplex(256);
        let payload = format!("{PREFIX}source-entry\nordinary stderr\n");
        writer.write_all(payload.as_bytes()).await.unwrap();
        writer.shutdown().await.unwrap();
        assert_eq!(
            capture_stderr(reader, Arc::clone(&trace), 128)
                .await
                .unwrap(),
            payload.as_bytes()
        );
        assert_eq!(trace.child_phases(), ["source-entry"]);
        let (mut writer, reader) = tokio::io::duplex(256);
        writer.write_all(&[b'x'; 129]).await.unwrap();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(100),
                capture_stderr(reader, trace, 128)
            )
            .await
            .unwrap()
            .is_err()
        );
        drop(writer);
    }
}
