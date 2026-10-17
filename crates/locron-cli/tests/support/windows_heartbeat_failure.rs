//! Closed Windows heartbeat failure observations shared by integration targets.
#![cfg(windows)]

use locron_store::{FrameChannel, FrameReader, StatePaths};
use std::path::Path;

pub(super) const HEARTBEAT_FAILURE_PHYSICAL_CAP: u64 = 8 * 1024;
pub(super) const HEARTBEAT_FAILURE_PAYLOAD_CAP: u64 = 4 * 1024;
const HEARTBEAT_FAILURE_FRAME_CAP: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HeartbeatFailurePhase {
    Create,
    Write,
    Flush,
    Persist,
}

impl HeartbeatFailurePhase {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Write => "write",
            Self::Flush => "flush",
            Self::Persist => "persist",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HeartbeatFailure {
    pub(super) phase: HeartbeatFailurePhase,
    pub(super) counter: u64,
    pub(super) kind: std::io::ErrorKind,
    pub(super) raw_os: Option<i32>,
}

impl std::fmt::Display for HeartbeatFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "phase={} counter={} kind={:?} raw_os={:?}",
            self.phase.as_str(),
            self.counter,
            self.kind,
            self.raw_os,
        )
    }
}

fn parse_heartbeat_failure(stderr: &[u8]) -> Option<HeartbeatFailure> {
    let stderr = std::str::from_utf8(stderr).ok()?;
    let mut observed = None;
    for line in stderr.split_inclusive('\n') {
        if !line.contains("native-heartbeat-failure") {
            continue;
        }
        if observed.is_some() {
            return None;
        }
        let line = line.strip_suffix('\n')?;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let (phase, rest) = line
            .strip_prefix("native-heartbeat-failure phase=")?
            .split_once(" counter=")?;
        let phase = match phase {
            "create" => HeartbeatFailurePhase::Create,
            "write" => HeartbeatFailurePhase::Write,
            "flush" => HeartbeatFailurePhase::Flush,
            "persist" => HeartbeatFailurePhase::Persist,
            _ => return None,
        };
        let (counter_text, rest) = rest.split_once(" kind=")?;
        let counter = counter_text.parse::<u64>().ok()?;
        if counter.to_string() != counter_text {
            return None;
        }
        let (kind, raw_os) = rest.split_once(" raw_os=")?;
        let kind = match kind {
            "NotFound" => std::io::ErrorKind::NotFound,
            "PermissionDenied" => std::io::ErrorKind::PermissionDenied,
            "AlreadyExists" => std::io::ErrorKind::AlreadyExists,
            "Interrupted" => std::io::ErrorKind::Interrupted,
            "InvalidInput" => std::io::ErrorKind::InvalidInput,
            "InvalidData" => std::io::ErrorKind::InvalidData,
            "WouldBlock" => std::io::ErrorKind::WouldBlock,
            "TimedOut" => std::io::ErrorKind::TimedOut,
            "WriteZero" => std::io::ErrorKind::WriteZero,
            "BrokenPipe" => std::io::ErrorKind::BrokenPipe,
            "StorageFull" => std::io::ErrorKind::StorageFull,
            "Other" => std::io::ErrorKind::Other,
            _ => return None,
        };
        let raw_os = if raw_os == "None" {
            None
        } else {
            let raw_text = raw_os.strip_prefix("Some(")?.strip_suffix(')')?;
            let raw = raw_text.parse::<i32>().ok()?;
            if raw.to_string() != raw_text {
                return None;
            }
            Some(raw)
        };
        observed = Some(HeartbeatFailure {
            phase,
            counter,
            kind,
            raw_os,
        });
    }
    observed
}

pub(super) fn read_finalized_heartbeat_failure(
    paths: &StatePaths,
    run_id: &str,
    attempt: u16,
    physical: u64,
    payload: u64,
) -> Option<HeartbeatFailure> {
    if !(8..=HEARTBEAT_FAILURE_PHYSICAL_CAP).contains(&physical)
        || payload > HEARTBEAT_FAILURE_PAYLOAD_CAP
    {
        return None;
    }
    let path = paths.final_output(run_id, attempt).ok()?;
    let stable = locron_core::filesystem::open_private_read_stable(&path).ok()?;
    if stable.metadata().ok()?.len() != physical {
        return None;
    }
    // Retain the stable read guard across the reader's guarded reopen and all reads.
    // The file/retained-byte caps do not change FrameReader's existing 1 MiB maximum
    // internal allocation when a corrupt frame declares an oversized payload.
    let mut reader = FrameReader::open(stable.normalized_path()).ok()?;
    let mut consumed = 8_u64;
    let mut retained = 0_u64;
    let mut frames = 0;
    let mut stderr = Vec::new();
    while consumed < physical {
        if frames == HEARTBEAT_FAILURE_FRAME_CAP {
            return None;
        }
        let frame = reader.next_frame().ok()??;
        frames += 1;
        let length = u64::try_from(frame.payload.len()).ok()?;
        consumed = consumed.checked_add(locron_store::FRAME_HEADER_LEN as u64 + length)?;
        retained = retained.checked_add(length)?;
        if consumed > physical || retained > HEARTBEAT_FAILURE_PAYLOAD_CAP {
            return None;
        }
        if frame.channel == FrameChannel::Stderr {
            stderr.extend_from_slice(&frame.payload);
        }
    }
    // FrameReader treats a short trailing header as EOF. Exact physical accounting
    // above refuses that partial stream instead of accepting its earlier record.
    if consumed != physical || retained != payload || stable.metadata().ok()?.len() != physical {
        return None;
    }
    parse_heartbeat_failure(&stderr)
}
