//! Real native CLI control fixtures; auxiliary child roles are not acceptance tests.

use super::private_state::{PrivateState, private_state_fixture};
use interprocess::os::windows::named_pipe::{PipeStream, pipe_mode};
use locron_core::filesystem::{
    DirectoryGuard, GuardedFile, create_private_new, file_identity, is_private, open_read_no_follow,
};
use locron_core::notification::{ACK_MESSAGE, WAKE_MESSAGE, endpoint_name_guarded};
use locron_store::{DaemonLock, LockProbe};
use std::fmt;
use std::fs::File;
use std::future::{Future, poll_fn};
use std::io::{self, Read, Seek, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, OnceLock};
use std::task::Poll;
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient, PipeMode, ServerOptions};
use tokio::runtime::{Builder, Runtime};

const GUARD: u8 = 1;
const CLIENT: u8 = 2;
const QUERIED: u8 = 4;
const MATCHED: u8 = 8;
const ACK: u8 = 16;
const REAPED: u8 = 32;
const CLEANED: u8 = 64;
const READ_LIMIT: u64 = 64 * 1024;
const MAX_CLI_CAPTURES: u16 = 512;
const UNOBSERVED_BYTES: u32 = u32::MAX;
const OUTER_NANOS: u64 = 30_000_000_000;
const TARGET_SELECTOR: &str = "windows_cli_control::native_cancel_target";
const PEER_SELECTOR: &str = "windows_cli_control::native_wake_peer_target";
const OUTPUT_SELECTOR: &str = "windows_cli_control::native_cli_output_target";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Code {
    Success,
    Expired,
    Native,
    ChildExited,
    Role,
    ForeignPeer,
    WrongDirection,
    BadAck,
    Cli,
    Run,
    Progress,
    Cleanup,
    Panicked,
    CaptureOversized,
    CaptureCollision,
}

impl Code {
    fn slot(self) -> u8 {
        match self {
            Self::Success => 0,
            Self::Expired => 1,
            Self::Native => 2,
            Self::ChildExited => 3,
            Self::Role => 4,
            Self::ForeignPeer => 5,
            Self::WrongDirection => 6,
            Self::BadAck => 7,
            Self::Cli => 8,
            Self::Run => 9,
            Self::Progress => 10,
            Self::Cleanup => 11,
            Self::Panicked => 12,
            Self::CaptureOversized => 13,
            Self::CaptureCollision => 14,
        }
    }
    fn decode(slot: u8) -> Option<Self> {
        match slot {
            0 => Some(Self::Success),
            1 => Some(Self::Expired),
            2 => Some(Self::Native),
            3 => Some(Self::ChildExited),
            4 => Some(Self::Role),
            5 => Some(Self::ForeignPeer),
            6 => Some(Self::WrongDirection),
            7 => Some(Self::BadAck),
            8 => Some(Self::Cli),
            9 => Some(Self::Run),
            10 => Some(Self::Progress),
            11 => Some(Self::Cleanup),
            12 => Some(Self::Panicked),
            13 => Some(Self::CaptureOversized),
            14 => Some(Self::CaptureCollision),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
enum Phase {
    Setup,
    Guard,
    Role,
    Connect,
    Query,
    Frame,
    Ack,
    Receipt,
    Add,
    Run,
    History,
    Progress,
    Cancel,
    Cleanup,
    Capture,
}

impl Phase {
    fn from_slot(slot: u8) -> Self {
        match slot {
            1 => Self::Guard,
            2 => Self::Role,
            3 => Self::Connect,
            4 => Self::Query,
            5 => Self::Frame,
            6 => Self::Ack,
            7 => Self::Receipt,
            8 => Self::Add,
            9 => Self::Run,
            10 => Self::History,
            11 => Self::Progress,
            12 => Self::Cancel,
            13 => Self::Cleanup,
            14 => Self::Capture,
            _ => Self::Setup,
        }
    }
}

// Scalar observations are independent returned facts, never ownership/readiness.
const OP_NAMES: [&str; 62] = [
    "unobserved",
    "StateSetup",
    "StateGuard",
    "CaseCurrentExe",
    "DaemonSpawn",
    "PeerSpawn",
    "ChildLiveness",
    "ReadyOpen",
    "ReadyRead",
    "AddCliSpawn",
    "RunCliSpawn",
    "HistoryCliSpawn",
    "CancelCliSpawn",
    "AddCliRead",
    "RunCliRead",
    "HistoryCliRead",
    "CancelCliRead",
    "AddCliWait",
    "RunCliWait",
    "HistoryCliWait",
    "CancelCliWait",
    "HistoryJson",
    "HistorySelect",
    "SubmitJson",
    "SubmitSelect",
    "ProgressOpen",
    "ProgressRead",
    "ProgressValidate",
    "RoleMetadataRead",
    "RoleLockProbe",
    "RoleMetadataRepeat",
    "EndpointName",
    "RuntimeBuild",
    "PipeOpen",
    "PipeClone",
    "PipeConvert",
    "PipeDirection",
    "PipePid",
    "PipeFrameWrite",
    "PipeAckRead",
    "PipeReceiptWrite",
    "CleanupTryWait",
    "CleanupKill",
    "CleanupWait",
    "DropStdout",
    "DropMetadata",
    "DropQueryHandle",
    "DropClient",
    "DropRuntime",
    "DropGuard",
    "DropState",
    "CleanupStateExists",
    "WorkOutcome",
    "StdoutCreate",
    "StdoutClone",
    "StdoutIdentity",
    "StdoutReaderOpen",
    "CaptureCliSpawn",
    "CaptureCliRead",
    "CaptureCliWait",
    "StdoutControlWrite",
    "StdoutControlFlush",
];

#[derive(Clone, Copy)]
#[repr(u8)]
enum Operation {
    StateSetup = 1,
    StateGuard,
    CaseCurrentExe,
    DaemonSpawn,
    PeerSpawn,
    ChildLiveness,
    ReadyOpen,
    ReadyRead,
    AddCliSpawn,
    RunCliSpawn,
    HistoryCliSpawn,
    CancelCliSpawn,
    AddCliRead,
    RunCliRead,
    HistoryCliRead,
    CancelCliRead,
    AddCliWait,
    RunCliWait,
    HistoryCliWait,
    CancelCliWait,
    HistoryJson,
    HistorySelect,
    SubmitJson,
    SubmitSelect,
    ProgressOpen,
    ProgressRead,
    ProgressValidate,
    RoleMetadataRead,
    RoleLockProbe,
    RoleMetadataRepeat,
    EndpointName,
    RuntimeBuild,
    PipeOpen,
    PipeClone,
    PipeConvert,
    PipeDirection,
    PipePid,
    PipeFrameWrite,
    PipeAckRead,
    PipeReceiptWrite,
    CleanupTryWait,
    CleanupKill,
    CleanupWait,
    DropStdout,
    DropMetadata,
    DropQueryHandle,
    DropClient,
    DropRuntime,
    DropGuard,
    DropState,
    CleanupStateExists,
    WorkOutcome,
    StdoutCreate,
    StdoutClone,
    StdoutIdentity,
    StdoutReaderOpen,
    CaptureCliSpawn,
    CaptureCliRead,
    CaptureCliWait,
    StdoutControlWrite,
    StdoutControlFlush,
}

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u8)]
enum ChildRole {
    NoChild,
    Daemon,
    NegativePeer,
    ControlCli,
}

impl ChildRole {
    fn name(self) -> &'static str {
        match self {
            Self::NoChild => "NoChild",
            Self::Daemon => "Daemon",
            Self::NegativePeer => "NegativePeer",
            Self::ControlCli => "ControlCli",
        }
    }

    fn decode(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NoChild),
            1 => Some(Self::Daemon),
            2 => Some(Self::NegativePeer),
            3 => Some(Self::ControlCli),
            _ => None,
        }
    }
}

const VALID_EVENT: u64 = 1 << 63;
const PAYLOAD_MASK: u64 = (1 << 38) - 1;
const RAW_MASK: u64 = (1 << 32) - 1;
const PRESENT: u64 = 1 << 32;
const INVALID_EVENT: u64 = u64::MAX;

fn event_word(tag: u8, operation: Operation, role: ChildRole, payload: u64) -> u64 {
    let operation = operation as u8;
    if !(1..=4).contains(&tag) || !(1..=61).contains(&operation) || payload > PAYLOAD_MASK {
        return INVALID_EVENT;
    }
    VALID_EVENT
        | (u64::from(tag) << 60)
        | (u64::from(role as u8) << 44)
        | (u64::from(operation) << 38)
        | payload
}

fn signed_payload(value: Option<i32>) -> u64 {
    value.map_or(0, |value| {
        PRESENT | u64::from(u32::from_ne_bytes(value.to_ne_bytes()))
    })
}

struct InvalidScalar;

fn decode_signed(payload: u64) -> Result<Option<i32>, InvalidScalar> {
    let bits = u32::try_from(payload & RAW_MASK).map_err(|_| InvalidScalar)?;
    if payload & PRESENT == 0 {
        return (bits == 0).then_some(None).ok_or(InvalidScalar);
    }
    Ok(Some(i32::from_ne_bytes(bits.to_ne_bytes())))
}

const KIND_NAMES: [&str; 18] = [
    "invalid",
    "Other",
    "NotFound",
    "PermissionDenied",
    "AlreadyExists",
    "WouldBlock",
    "TimedOut",
    "Interrupted",
    "InvalidInput",
    "InvalidData",
    "UnexpectedEof",
    "WriteZero",
    "BrokenPipe",
    "NotConnected",
    "ConnectionAborted",
    "ConnectionRefused",
    "ConnectionReset",
    "Unsupported",
];

fn kind_bucket(kind: io::ErrorKind) -> u8 {
    match kind {
        io::ErrorKind::NotFound => 2,
        io::ErrorKind::PermissionDenied => 3,
        io::ErrorKind::AlreadyExists => 4,
        io::ErrorKind::WouldBlock => 5,
        io::ErrorKind::TimedOut => 6,
        io::ErrorKind::Interrupted => 7,
        io::ErrorKind::InvalidInput => 8,
        io::ErrorKind::InvalidData => 9,
        io::ErrorKind::UnexpectedEof => 10,
        io::ErrorKind::WriteZero => 11,
        io::ErrorKind::BrokenPipe => 12,
        io::ErrorKind::NotConnected => 13,
        io::ErrorKind::ConnectionAborted => 14,
        io::ErrorKind::ConnectionRefused => 15,
        io::ErrorKind::ConnectionReset => 16,
        io::ErrorKind::Unsupported => 17,
        _ => 1,
    }
}

struct Observations {
    intent: AtomicU8,
    io_error: AtomicU64,
    daemon_status: AtomicU64,
    peer_status: AtomicU64,
    cli_status: AtomicU64,
    first_work: AtomicU64,
    ready_read: AtomicU64,
}

impl Observations {
    fn new() -> Self {
        Self {
            intent: AtomicU8::new(0),
            io_error: AtomicU64::new(0),
            daemon_status: AtomicU64::new(0),
            peer_status: AtomicU64::new(0),
            cli_status: AtomicU64::new(0),
            first_work: AtomicU64::new(0),
            ready_read: AtomicU64::new(0),
        }
    }

    fn intent(&self, operation: Operation) {
        self.intent.store(operation as u8, Ordering::Release);
    }

    fn error(&self, operation: Operation, role: ChildRole, error: &io::Error) {
        let payload =
            signed_payload(error.raw_os_error()) | (u64::from(kind_bucket(error.kind())) << 33);
        self.io_error
            .store(event_word(1, operation, role, payload), Ordering::Release);
    }

    fn io<T>(&self, operation: Operation, role: ChildRole, result: &io::Result<T>) {
        if let Err(error) = result {
            self.error(operation, role, error);
        }
    }

    fn status(&self, operation: Operation, role: ChildRole, status: std::process::ExitStatus) {
        let slot = match role {
            ChildRole::Daemon => &self.daemon_status,
            ChildRole::NegativePeer => &self.peer_status,
            ChildRole::ControlCli => &self.cli_status,
            ChildRole::NoChild => return,
        };
        slot.store(
            event_word(2, operation, role, signed_payload(status.code())),
            Ordering::Release,
        );
    }

    fn work(&self, work: Result<(), Code>, phase: u8) {
        self.intent(Operation::WorkOutcome);
        let code = match work {
            Ok(()) => Code::Success,
            Err(code) => code,
        };
        let word = if phase <= 14 {
            event_word(
                3,
                Operation::WorkOutcome,
                ChildRole::NoChild,
                u64::from(code.slot()) | (u64::from(phase) << 4),
            )
        } else {
            INVALID_EVENT
        };
        // One completed work result; cleanup and Drop cannot replace it.
        let _ = self
            .first_work
            .compare_exchange(0, word, Ordering::AcqRel, Ordering::Acquire);
    }

    fn ready(&self, bytes: &[u8]) {
        let (class, length) = match bytes {
            [] => (0_u64, 0_u64),
            b"1" => (1, 1),
            [_] => (2, 1),
            [_, _] => (2, 2),
            _ => {
                self.ready_read.store(INVALID_EVENT, Ordering::Release);
                return;
            }
        };
        self.ready_read.store(
            event_word(
                4,
                Operation::ReadyRead,
                ChildRole::NegativePeer,
                class | (length << 2),
            ),
            Ordering::Release,
        );
    }

    fn snapshot(&self) -> ObservationSnapshot {
        ObservationSnapshot {
            intent: self.intent.load(Ordering::Acquire),
            io_error: self.io_error.load(Ordering::Acquire),
            statuses: [
                self.daemon_status.load(Ordering::Acquire),
                self.peer_status.load(Ordering::Acquire),
                self.cli_status.load(Ordering::Acquire),
            ],
            first_work: self.first_work.load(Ordering::Acquire),
            ready_read: self.ready_read.load(Ordering::Acquire),
        }
    }
}

struct ObservationSnapshot {
    intent: u8,
    io_error: u64,
    statuses: [u64; 3],
    first_work: u64,
    ready_read: u64,
}

struct EventDisplay {
    word: u64,
    tag: u8,
    expected_role: Option<ChildRole>,
}

fn event_header(word: u64, tag: u8) -> Option<(u8, ChildRole, u64)> {
    let operation = u8::try_from((word >> 38) & 63).ok()?;
    let role = ChildRole::decode(u8::try_from((word >> 44) & 7).ok()?)?;
    let allowed = VALID_EVENT | (7 << 60) | (7 << 44) | (63 << 38) | PAYLOAD_MASK;
    if word & VALID_EVENT == 0
        || word & !allowed != 0
        || (word >> 60) & 7 != u64::from(tag)
        || !(1..=61).contains(&operation)
    {
        return None;
    }
    Some((operation, role, word & PAYLOAD_MASK))
}

impl fmt::Display for EventDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.word == 0 {
            return formatter.write_str("unobserved");
        }
        let Some((operation, role, payload)) = event_header(self.word, self.tag) else {
            return formatter.write_str("invalid");
        };
        if self.expected_role.is_some_and(|expected| expected != role) {
            return formatter.write_str("invalid");
        }
        let op = OP_NAMES
            .get(usize::from(operation))
            .copied()
            .unwrap_or("invalid");
        match self.tag {
            1 => {
                let kind = usize::try_from(payload >> 33)
                    .ok()
                    .filter(|kind| (1..=17).contains(kind));
                let Some(kind) = kind.and_then(|kind| KIND_NAMES.get(kind)) else {
                    return formatter.write_str("invalid");
                };
                let Ok(raw) = decode_signed(payload) else {
                    return formatter.write_str("invalid");
                };
                write!(formatter, "{{op={op},role={},kind={kind},raw=", role.name())?;
                match raw {
                    Some(raw) => write!(formatter, "{raw}"),
                    None => formatter.write_str("none"),
                }?;
                formatter.write_str("}")
            }
            2 => {
                if payload >> 33 != 0
                    || role == ChildRole::NoChild
                    || !matches!(operation, 6 | 17..=20 | 41 | 43 | 59)
                {
                    return formatter.write_str("invalid");
                }
                let Ok(code) = decode_signed(payload) else {
                    return formatter.write_str("invalid");
                };
                write!(formatter, "{{op={op},role={},code=", role.name())?;
                match code {
                    Some(code) => write!(formatter, "{code}"),
                    None => formatter.write_str("none"),
                }?;
                formatter.write_str("}")
            }
            3 => {
                if operation != 52
                    || role != ChildRole::NoChild
                    || payload >> 8 != 0
                    || (payload >> 4) > 14
                {
                    return formatter.write_str("invalid");
                }
                let Some(code) = u8::try_from(payload & 15).ok().and_then(Code::decode) else {
                    return formatter.write_str("invalid");
                };
                let Some(phase) = u8::try_from(payload >> 4).ok() else {
                    return formatter.write_str("invalid");
                };
                write!(
                    formatter,
                    "{{op={op},role=NoChild,code={code:?},phase={:?}}}",
                    Phase::from_slot(phase)
                )
            }
            4 => {
                let class = payload & 3;
                let length = (payload >> 2) & 3;
                if operation != 8
                    || role != ChildRole::NegativePeer
                    || payload >> 4 != 0
                    || !matches!((class, length), (0, 0) | (1, 1) | (2, 1 | 2))
                {
                    return formatter.write_str("invalid");
                }
                let class = match class {
                    0 => "Empty",
                    1 => "Expected",
                    _ => "Unexpected",
                };
                write!(
                    formatter,
                    "{{op={op},role=NegativePeer,class={class},len={length}}}"
                )
            }
            _ => formatter.write_str("invalid"),
        }
    }
}

impl fmt::Display for ObservationSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        // At most 640 ASCII bytes including the fixed capture operation names.
        // Slots are independent observations, not same-instruction causal facts.
        let intent = OP_NAMES
            .get(usize::from(self.intent))
            .copied()
            .unwrap_or("invalid");
        write!(
            formatter,
            " intent={intent} last_io={} daemon_status={} peer_status={} cli_status={} first_work={} ready={}",
            EventDisplay {
                word: self.io_error,
                tag: 1,
                expected_role: None
            },
            EventDisplay {
                word: self.statuses[0],
                tag: 2,
                expected_role: Some(ChildRole::Daemon)
            },
            EventDisplay {
                word: self.statuses[1],
                tag: 2,
                expected_role: Some(ChildRole::NegativePeer)
            },
            EventDisplay {
                word: self.statuses[2],
                tag: 2,
                expected_role: Some(ChildRole::ControlCli)
            },
            EventDisplay {
                word: self.first_work,
                tag: 3,
                expected_role: Some(ChildRole::NoChild)
            },
            EventDisplay {
                word: self.ready_read,
                tag: 4,
                expected_role: Some(ChildRole::NegativePeer)
            }
        )
    }
}

// This wrapper adds no gate, clock or I/O. The existing gated future still owns
// entry/post-poll expiry; a late actual return is a fact, never timely success.
async fn observed_io<T>(
    control: &Control,
    operation: Operation,
    role: ChildRole,
    future: impl Future<Output = io::Result<T>>,
) -> io::Result<T> {
    let mut future = pin!(future);
    poll_fn(|context| {
        control.observations.intent(operation);
        let result = future.as_mut().poll(context);
        if let Poll::Ready(result) = &result {
            control.observations.io(operation, role, result);
        }
        result
    })
    .await
}

// Test-private returned observations. These words never call a work/admission gate.
const CALL_VALID: u64 = 1 << 63;
const CALL_INVALID: u64 = 1;
const CALL_OVERFLOW: u64 = 2;
const CALL_TIME_US: u64 = 30_000_000;
const DAEMON_INVALID: u64 = u64::MAX;
const DAEMON_OVERFLOW: u64 = u64::MAX - 1;

#[derive(Clone, Copy, Eq, PartialEq)]
enum CallWordKind {
    Cli,
    NativeEnter,
    NativeReturn,
    Wait,
    Wrapper,
    Read,
    History,
    Cost,
    Progress,
    Daemon,
}

impl CallWordKind {
    fn layout(self) -> &'static [(u8, u8)] {
        match self {
            Self::Cli => &[(0, 10), (10, 10), (20, 4), (24, 3), (27, 25)],
            Self::NativeEnter | Self::NativeReturn => &[
                (0, 15),
                (15, 10),
                (25, 18),
                (43, 6),
                (49, 4),
                (53, 3),
                (56, 3),
            ],
            Self::Wait => &[(0, 32), (32, 1), (33, 10), (43, 18), (61, 2)],
            Self::Wrapper => &[(0, 4), (4, 10), (14, 10), (24, 4), (28, 3), (31, 25)],
            Self::Read => &[(0, 17), (17, 10), (27, 18)],
            Self::History => &[(0, 3), (3, 10), (13, 10), (23, 3), (26, 25)],
            Self::Cost => &[(0, 25), (25, 25), (50, 10)],
            Self::Progress => &[(0, 12), (12, 10), (22, 10), (32, 25), (57, 3), (60, 3)],
            Self::Daemon => &[],
        }
    }

    fn overflow(self, v: &[u64; 7]) -> bool {
        match self {
            Self::Cli => v[0] > 513 || v[1] > 1023 || v[4] > CALL_TIME_US,
            Self::NativeEnter | Self::NativeReturn => v[0] > 30_000 || v[1] > 513 || v[2] > 262_143,
            Self::Wait => v[2] > 513 || v[3] > 262_143,
            Self::Wrapper => v[1] > 513 || v[2] > 1023 || v[5] > CALL_TIME_US,
            Self::Read => v[0] > 65_537 || v[1] > 513 || v[2] > 262_143,
            Self::History => v[1] > 513 || v[2] > 1023 || v[4] > CALL_TIME_US,
            Self::Cost => v[0] > CALL_TIME_US || v[1] > CALL_TIME_US || v[2] > 1023,
            Self::Progress => v[0] > 3855 || v[1] > 1023 || v[2] > 513 || v[3] > CALL_TIME_US,
            Self::Daemon => v[0] > CALL_TIME_US,
        }
    }

    fn valid(self, v: &[u64; 7]) -> bool {
        if self.overflow(v) || v[self.layout().len()..].iter().any(|field| *field != 0) {
            return false;
        }
        match self {
            Self::Cli => (1..=513).contains(&v[0]) && v[2] <= 14 && v[3] <= 5,
            Self::NativeEnter | Self::NativeReturn => {
                (1..=513).contains(&v[1])
                    && (1..=262_143).contains(&v[2])
                    && (1..=61).contains(&v[3])
                    && v[4] <= 14
                    && v[5] <= 3
                    && if self == Self::NativeEnter {
                        v[6] == 0
                    } else {
                        (1..=4).contains(&v[6]) && (v[6] < 3 || matches!(v[3], 17..=20 | 59))
                    }
            }
            Self::Wait => {
                (1..=513).contains(&v[2])
                    && (1..=262_143).contains(&v[3])
                    && (1..=3).contains(&v[4])
                    && v[1] <= 1
                    && (v[1] != 0 || v[0] == 0)
                    && (v[4] == 2 || (v[0] == 0 && v[1] == 0))
            }
            Self::Wrapper => v[0] <= 14 && (1..=513).contains(&v[1]) && v[3] <= 14 && v[4] <= 5,
            Self::Read => (1..=513).contains(&v[1]) && (1..=262_143).contains(&v[2]),
            Self::History => {
                (1..=5).contains(&v[0])
                    && (1..=513).contains(&v[1])
                    && (1..=1023).contains(&v[2])
                    && v[3] <= 5
            }
            Self::Cost => v[0] <= v[1] && (1..=1023).contains(&v[2]),
            Self::Progress => v[4] <= 5 && (1..=4).contains(&v[5]) && (v[5] == 2 || v[0] == 0),
            Self::Daemon => false,
        }
    }
}

fn call_word(kind: CallWordKind, fields: [u64; 7]) -> u64 {
    if kind.overflow(&fields) {
        return CALL_OVERFLOW;
    }
    if !kind.valid(&fields) {
        return CALL_INVALID;
    }
    let mut word = CALL_VALID;
    for (index, (shift, width)) in kind.layout().iter().copied().enumerate() {
        if fields[index] >= 1_u64 << width {
            return CALL_INVALID;
        }
        word |= fields[index] << shift;
    }
    word
}

fn call_fields(kind: CallWordKind, word: u64) -> Option<[u64; 7]> {
    if word & CALL_VALID == 0 || kind == CallWordKind::Daemon {
        return None;
    }
    let mut fields = [0; 7];
    let mut allowed = CALL_VALID;
    for (index, (shift, width)) in kind.layout().iter().copied().enumerate() {
        let mask = (1_u64 << width) - 1;
        allowed |= mask << shift;
        fields[index] = (word >> shift) & mask;
    }
    (word & !allowed == 0 && kind.valid(&fields)).then_some(fields)
}

fn observed_micros(origin: Instant, returned: Instant) -> u64 {
    returned
        .checked_duration_since(origin)
        .and_then(|duration| u64::try_from(duration.as_micros()).ok())
        .unwrap_or(u64::MAX)
}

struct CallObservations {
    current_cli: AtomicU64,
    native_enter: AtomicU64,
    native_return: AtomicU64,
    current_wait: AtomicU64,
    wrapper_return: AtomicU64,
    capture_read: AtomicU64,
    last_history: AtomicU64,
    history_cost: AtomicU64,
    last_progress: AtomicU64,
    daemon_spawn: AtomicU64,
    capture_proof: AtomicU8,
}

impl CallObservations {
    fn new() -> Self {
        Self {
            current_cli: AtomicU64::new(0),
            native_enter: AtomicU64::new(0),
            native_return: AtomicU64::new(0),
            current_wait: AtomicU64::new(0),
            wrapper_return: AtomicU64::new(0),
            capture_read: AtomicU64::new(0),
            last_history: AtomicU64::new(0),
            history_cost: AtomicU64::new(0),
            last_progress: AtomicU64::new(0),
            daemon_spawn: AtomicU64::new(0),
            capture_proof: AtomicU8::new(0),
        }
    }

    fn snapshot(&self) -> CallSnapshot {
        CallSnapshot {
            current_cli: self.current_cli.load(Ordering::Acquire),
            native_enter: self.native_enter.load(Ordering::Acquire),
            native_return: self.native_return.load(Ordering::Acquire),
            current_wait: self.current_wait.load(Ordering::Acquire),
            wrapper_return: self.wrapper_return.load(Ordering::Acquire),
            capture_read: self.capture_read.load(Ordering::Acquire),
            last_history: self.last_history.load(Ordering::Acquire),
            history_cost: self.history_cost.load(Ordering::Acquire),
            last_progress: self.last_progress.load(Ordering::Acquire),
            daemon_spawn: self.daemon_spawn.load(Ordering::Acquire),
        }
    }

    fn prove(&self, bit: u8, actual_fact: bool) {
        if actual_fact {
            self.capture_proof.fetch_or(bit, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct CallSnapshot {
    current_cli: u64,
    native_enter: u64,
    native_return: u64,
    current_wait: u64,
    wrapper_return: u64,
    capture_read: u64,
    last_history: u64,
    history_cost: u64,
    last_progress: u64,
    daemon_spawn: u64,
}

impl CallSnapshot {
    fn output_returned(&self, epoch: u64, code: Code, bytes: u64) -> bool {
        let current = call_fields(CallWordKind::Cli, self.current_cli);
        let waited = call_fields(CallWordKind::Wait, self.current_wait);
        let returned = call_fields(CallWordKind::Wrapper, self.wrapper_return);
        let read = call_fields(CallWordKind::Read, self.capture_read);
        let entered = call_fields(CallWordKind::NativeEnter, self.native_enter);
        let native = call_fields(CallWordKind::NativeReturn, self.native_return);
        matches!((current, waited, returned, read, entered, native),
            (Some(current), Some(waited), Some(returned), Some(read), Some(entered), Some(native))
                if current[0] == epoch && current[1] == 0 && current[2] == 14
                    && waited[2] == epoch && waited[4] == 2 && waited[1] == 1 && waited[0] == 0
                    && returned[1] == epoch && returned[2] == 0 && returned[0] == u64::from(code.slot())
                    && read[1] == epoch && read[0] == bytes
                    && entered[1] == epoch && native[1] == epoch && entered[2] == native[2]
                    && read[2] == native[2] && native[3] == 58 && native[6] == 1)
    }

    fn collision_returned(&self) -> bool {
        let current = call_fields(CallWordKind::Cli, self.current_cli);
        let returned = call_fields(CallWordKind::Wrapper, self.wrapper_return);
        let entered = call_fields(CallWordKind::NativeEnter, self.native_enter);
        let native = call_fields(CallWordKind::NativeReturn, self.native_return);
        self.current_wait == 0
            && self.capture_read == 0
            && matches!((current, returned, entered, native),
                (Some(c), Some(r), Some(e), Some(n))
                    if c[0] == 3 && c[1] == 0 && c[2] == 14
                        && r[1] == 3 && r[0] == 14 && r[2] == 0
                        && e[1] == 3 && n[1] == 3 && e[2] == 1 && n[2] == 1
                        && e[3] == 53 && n[3] == 53 && n[6] == 2)
    }

    fn live_returned(&self) -> bool {
        let current = call_fields(CallWordKind::Cli, self.current_cli);
        let waited = call_fields(CallWordKind::Wait, self.current_wait);
        let returned = call_fields(CallWordKind::Wrapper, self.wrapper_return);
        let entered = call_fields(CallWordKind::NativeEnter, self.native_enter);
        let native = call_fields(CallWordKind::NativeReturn, self.native_return);
        self.capture_read == 0
            && matches!((current, waited, returned, entered, native),
                (Some(current), Some(waited), Some(returned), Some(entered), Some(native))
                    if current[0] == 1 && current[1] == 0 && current[2] == 14
                        && waited[2] == 1 && waited[4] == 1 && waited[1] == 0 && waited[0] == 0
                        && returned[1] == 1 && returned[0] == 1 && returned[2] == 0
                        && entered[1] == 1 && native[1] == 1 && entered[2] == native[2] && waited[3] == native[2]
                        && native[3] == 59 && native[6] == 3)
    }

    fn relation(&self, kind: CallWordKind, word: u64) -> &'static str {
        let Some(current) = call_fields(CallWordKind::Cli, self.current_cli) else {
            return "unobserved";
        };
        let Some(record) = call_fields(kind, word) else {
            return "unobserved";
        };
        let (epoch, history) = match kind {
            CallWordKind::History => (record[1], record[2]),
            CallWordKind::Progress => (record[2], record[1]),
            _ => return "invalid",
        };
        if epoch == 0 {
            "unobserved"
        } else if epoch < current[0] {
            "older"
        } else if epoch == current[0] && history == current[1] {
            "same"
        } else {
            // Fixed snapshot loads do not retry to manufacture cross-word coherence.
            "mixed"
        }
    }
}

struct CallWordDisplay {
    kind: CallWordKind,
    word: u64,
}

impl fmt::Display for CallWordDisplay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.word == 0 {
            return f.write_str("unobserved");
        }
        if self.kind == CallWordKind::Daemon {
            return match self.word {
                DAEMON_OVERFLOW => f.write_str("overflow"),
                1..=30_000_001 => write!(f, "{}", self.word - 1),
                _ => f.write_str("invalid"),
            };
        }
        if self.word == CALL_OVERFLOW {
            return f.write_str("overflow");
        }
        let Some(v) = call_fields(self.kind, self.word) else {
            return f.write_str("invalid");
        };
        match self.kind {
            CallWordKind::Cli => write!(
                f,
                "{{ep={},h={},p={},s={},t_us={}}}",
                v[0], v[1], v[2], v[3], v[4]
            ),
            CallWordKind::NativeEnter | CallWordKind::NativeReturn => write!(
                f,
                "{{ep={},n={},op={},p={},role={},r={},t_ms={}}}",
                v[1], v[2], v[3], v[4], v[5], v[6], v[0]
            ),
            CallWordKind::Wait => {
                write!(f, "{{ep={},n={},r={},code=", v[2], v[3], v[4])?;
                if v[1] == 0 {
                    f.write_str("none")?;
                } else {
                    let Some(code) = u32::try_from(v[0]).ok() else {
                        return f.write_str("invalid}");
                    };
                    write!(f, "{}", i32::from_ne_bytes(code.to_ne_bytes()))?;
                }
                f.write_str("}")
            }
            CallWordKind::Wrapper => write!(
                f,
                "{{ep={},h={},c={},p={},s={},t_us={}}}",
                v[1], v[2], v[0], v[3], v[4], v[5]
            ),
            CallWordKind::Read => write!(f, "{{ep={},n={},count={}}}", v[1], v[2], v[0]),
            CallWordKind::History => write!(
                f,
                "{{ep={},h={},state={},s={},t_us={}}}",
                v[1], v[2], v[0], v[3], v[4]
            ),
            CallWordKind::Cost => write!(f, "{{h={},last_us={},sum_us={}}}", v[2], v[0], v[1]),
            CallWordKind::Progress => {
                write!(
                    f,
                    "{{ep={},h={},s={},class={},count=",
                    v[2], v[1], v[4], v[5]
                )?;
                if v[5] == 2 {
                    write!(f, "{}", v[0])?;
                } else {
                    f.write_str("unobserved")?;
                }
                write!(f, ",t_us={}}}", v[3])
            }
            CallWordKind::Daemon => f.write_str("invalid"),
        }
    }
}

impl fmt::Display for CallSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, kind, word) in [
            ("cli", CallWordKind::Cli, self.current_cli),
            ("wait", CallWordKind::Wait, self.current_wait),
            ("native_enter", CallWordKind::NativeEnter, self.native_enter),
            (
                "native_return",
                CallWordKind::NativeReturn,
                self.native_return,
            ),
            ("wrapper_return", CallWordKind::Wrapper, self.wrapper_return),
            ("last_history", CallWordKind::History, self.last_history),
            ("history_cost", CallWordKind::Cost, self.history_cost),
            ("last_progress", CallWordKind::Progress, self.last_progress),
            ("daemon_spawn_us", CallWordKind::Daemon, self.daemon_spawn),
            ("capture_read", CallWordKind::Read, self.capture_read),
        ] {
            write!(f, " {name}={}", CallWordDisplay { kind, word })?;
        }
        write!(
            f,
            " history_rel={} progress_rel={}",
            self.relation(CallWordKind::History, self.last_history),
            self.relation(CallWordKind::Progress, self.last_progress)
        )
    }
}

#[derive(Clone, Copy)]
struct PendingHistory {
    serial: u16,
    epoch: u16,
    stage: u8,
    entered: Instant,
}

// Worker-local cells hold only finite counters and actual Instant context.
struct CallContext {
    attempts: std::cell::Cell<u16>,
    epoch: std::cell::Cell<u16>,
    history: std::cell::Cell<u16>,
    history_serial: std::cell::Cell<u16>,
    history_sum: std::cell::Cell<Option<u128>>,
    pending: std::cell::Cell<Option<PendingHistory>>,
    step: std::cell::Cell<u32>,
    phase: std::cell::Cell<u8>,
    stage: std::cell::Cell<u8>,
    active: std::cell::Cell<bool>,
    version_control: std::cell::Cell<bool>,
}

impl CallContext {
    fn new() -> Self {
        Self {
            attempts: std::cell::Cell::new(0),
            epoch: std::cell::Cell::new(0),
            history: std::cell::Cell::new(0),
            history_serial: std::cell::Cell::new(0),
            history_sum: std::cell::Cell::new(Some(0)),
            pending: std::cell::Cell::new(None),
            step: std::cell::Cell::new(0),
            phase: std::cell::Cell::new(0),
            stage: std::cell::Cell::new(0),
            active: std::cell::Cell::new(false),
            version_control: std::cell::Cell::new(false),
        }
    }

    fn begin(&self, control: &Control, phase: Phase) {
        let entered = Instant::now();
        let epoch = self
            .attempts
            .get()
            .checked_add(1)
            .filter(|n| *n <= 513)
            .unwrap_or(514);
        self.attempts.set(epoch);
        self.epoch.set(epoch);
        self.history
            .set(self.pending.get().map_or(0, |history| history.serial));
        if let Some(mut history) = self.pending.get() {
            history.epoch = epoch;
            self.pending.set(Some(history));
        }
        self.phase.set(phase as u8);
        self.step.set(0);
        self.active.set(true);
        let slots = &control.calls;
        slots.current_wait.store(0, Ordering::Release);
        slots.native_enter.store(0, Ordering::Release);
        slots.native_return.store(0, Ordering::Release);
        slots.wrapper_return.store(0, Ordering::Release);
        slots.capture_read.store(0, Ordering::Release);
        slots.current_cli.store(
            call_word(
                CallWordKind::Cli,
                [
                    u64::from(epoch),
                    u64::from(self.history.get()),
                    u64::from(phase as u8),
                    u64::from(self.stage.get()),
                    observed_micros(control.entered, entered),
                    0,
                    0,
                ],
            ),
            Ordering::Release,
        );
        if self.version_control.get() && epoch == 2 {
            let snapshot = slots.snapshot();
            slots.prove(
                2,
                call_fields(CallWordKind::Cli, snapshot.current_cli)
                    .is_some_and(|c| c[0] == 2 && c[1] == 0)
                    && snapshot.current_wait == 0
                    && snapshot.native_enter == 0
                    && snapshot.native_return == 0
                    && snapshot.wrapper_return == 0
                    && snapshot.capture_read == 0,
            );
        }
    }

    fn returned<T>(&self, control: &Control, result: &Result<T, Code>) {
        let returned = Instant::now();
        let code = result.as_ref().err().copied().unwrap_or(Code::Success);
        control.calls.wrapper_return.store(
            call_word(
                CallWordKind::Wrapper,
                [
                    u64::from(code.slot()),
                    u64::from(self.epoch.get()),
                    u64::from(self.history.get()),
                    u64::from(control.phase.load(Ordering::Acquire)),
                    u64::from(self.stage.get()),
                    observed_micros(control.entered, returned),
                    0,
                ],
            ),
            Ordering::Release,
        );
        self.active.set(false);
    }

    fn enter(&self, control: &Control, operation: Operation, role: ChildRole) -> u32 {
        if !self.active.get() {
            return 0;
        }
        let step = self
            .step
            .get()
            .checked_add(1)
            .filter(|n| *n <= 262_143)
            .unwrap_or(262_144);
        self.step.set(step);
        self.native(control, step, operation, role, 0);
        step
    }

    fn native(
        &self,
        control: &Control,
        step: u32,
        operation: Operation,
        role: ChildRole,
        boundary: u64,
    ) {
        if !self.active.get() || step == 0 {
            return;
        }
        let time = u64::try_from(control.entered.elapsed().as_millis()).unwrap_or(u64::MAX);
        let word = call_word(
            if boundary == 0 {
                CallWordKind::NativeEnter
            } else {
                CallWordKind::NativeReturn
            },
            [
                time,
                u64::from(self.epoch.get()),
                u64::from(step),
                u64::from(operation as u8),
                u64::from(self.phase.get()),
                u64::from(role as u8),
                boundary,
            ],
        );
        let slot = if boundary == 0 {
            &control.calls.native_enter
        } else {
            &control.calls.native_return
        };
        slot.store(word, Ordering::Release);
    }

    fn io_return(&self, control: &Control, step: u32, operation: Operation, ok: bool) {
        self.native(
            control,
            step,
            operation,
            ChildRole::ControlCli,
            if ok { 1 } else { 2 },
        );
    }

    fn wait_return(
        &self,
        control: &Control,
        step: u32,
        operation: Operation,
        result: &io::Result<Option<std::process::ExitStatus>>,
    ) {
        if !self.active.get() || step == 0 {
            return;
        }
        let (kind, code, boundary) = match result {
            Ok(None) => (1, None, 3),
            Ok(Some(status)) => (2, status.code(), 4),
            Err(_error) => (3, None, 2),
        };
        let signed = signed_payload(code);
        control.calls.current_wait.store(
            call_word(
                CallWordKind::Wait,
                [
                    signed & RAW_MASK,
                    (signed >> 32) & 1,
                    u64::from(self.epoch.get()),
                    u64::from(step),
                    kind,
                    0,
                    0,
                ],
            ),
            Ordering::Release,
        );
        self.native(control, step, operation, ChildRole::ControlCli, boundary);
    }

    fn read_return(&self, control: &Control, step: u32, count: usize) {
        if !self.active.get() || step == 0 {
            return;
        }
        control.calls.capture_read.store(
            call_word(
                CallWordKind::Read,
                [
                    u64::try_from(count).unwrap_or(u64::MAX),
                    u64::from(self.epoch.get()),
                    u64::from(step),
                    0,
                    0,
                    0,
                    0,
                ],
            ),
            Ordering::Release,
        );
    }

    fn begin_history(&self) {
        let entered = Instant::now();
        let serial = self
            .history_serial
            .get()
            .checked_add(1)
            .filter(|n| *n <= 1023)
            .unwrap_or(1024);
        self.history_serial.set(serial);
        self.pending.set(Some(PendingHistory {
            serial,
            epoch: 0,
            stage: self.stage.get(),
            entered,
        }));
    }

    fn history_return(&self, control: &Control, result: &Result<String, Code>, returned: Instant) {
        if let (Some(history), Ok(state)) = (self.pending.get(), result) {
            let state = match state.as_str() {
                "queued" => 1,
                "running" => 2,
                "cancelled" => 3,
                "succeeded" => 4,
                _ => 5,
            };
            control.calls.last_history.store(
                call_word(
                    CallWordKind::History,
                    [
                        state,
                        u64::from(history.epoch),
                        u64::from(history.serial),
                        u64::from(history.stage),
                        observed_micros(control.entered, returned),
                        0,
                        0,
                    ],
                ),
                Ordering::Release,
            );
            let duration = returned
                .checked_duration_since(history.entered)
                .map(|elapsed| elapsed.as_micros());
            let sum = self
                .history_sum
                .get()
                .zip(duration)
                .and_then(|(sum, duration)| sum.checked_add(duration));
            self.history_sum.set(sum);
            control.calls.history_cost.store(
                call_word(
                    CallWordKind::Cost,
                    [
                        duration
                            .and_then(|n| u64::try_from(n).ok())
                            .unwrap_or(u64::MAX),
                        sum.and_then(|n| u64::try_from(n).ok()).unwrap_or(u64::MAX),
                        u64::from(history.serial),
                        0,
                        0,
                        0,
                        0,
                    ],
                ),
                Ordering::Release,
            );
        }
        self.pending.set(None);
    }

    fn progress_return(
        &self,
        control: &Control,
        result: &Result<Vec<u64>, Code>,
        missing: bool,
        returned: Instant,
    ) {
        let (class, count) = match result {
            Ok(_) if missing => (1, 0),
            Ok(counters) => (2, u64::try_from(counters.len()).unwrap_or(u64::MAX)),
            Err(Code::Expired) => (4, 0),
            Err(_code) => (3, 0),
        };
        control.calls.last_progress.store(
            call_word(
                CallWordKind::Progress,
                [
                    count,
                    u64::from(self.history.get()),
                    u64::from(self.epoch.get()),
                    observed_micros(control.entered, returned),
                    u64::from(self.stage.get()),
                    class,
                    0,
                ],
            ),
            Ordering::Release,
        );
    }
}

// BEGIN paired daemon diagnostic domains (independent of CLI call words).
const STORE_MARKER: &[u8] = b"windows-store-open";
const STORE_STAGES: [&str; 18] = [
    "directory-root",
    "directory-outputs",
    "directory-tmp",
    "state-guard",
    "database-open",
    "database-create-new",
    "database-admission",
    "wal-open",
    "wal-create-new",
    "shm-open",
    "shm-create-new",
    "sqlite-connection",
    "sqlite-configure-wal",
    "sqlite-configure-settings",
    "sqlite-migrate",
    "database-final-open",
    "wal-final-open",
    "shm-final-open",
];
const STORE_IO_KINDS: [&str; 44] = [
    "NotFound",
    "PermissionDenied",
    "ConnectionRefused",
    "ConnectionReset",
    "HostUnreachable",
    "NetworkUnreachable",
    "ConnectionAborted",
    "NotConnected",
    "AddrInUse",
    "AddrNotAvailable",
    "NetworkDown",
    "BrokenPipe",
    "AlreadyExists",
    "WouldBlock",
    "NotADirectory",
    "IsADirectory",
    "DirectoryNotEmpty",
    "ReadOnlyFilesystem",
    "FilesystemLoop",
    "StaleNetworkFileHandle",
    "InvalidInput",
    "InvalidData",
    "TimedOut",
    "WriteZero",
    "StorageFull",
    "NotSeekable",
    "QuotaExceeded",
    "FileTooLarge",
    "ResourceBusy",
    "ExecutableFileBusy",
    "Deadlock",
    "CrossesDevices",
    "TooManyLinks",
    "InvalidFilename",
    "ArgumentListTooLong",
    "Interrupted",
    "Unsupported",
    "UnexpectedEof",
    "OutOfMemory",
    "InProgress",
    "Other",
    "Uncategorized",
    "TooManyOpenFiles",
    "InputOutputError",
];
const STORE_SQLITE_KINDS: [&str; 24] = [
    "InternalMalfunction",
    "PermissionDenied",
    "OperationAborted",
    "DatabaseBusy",
    "DatabaseLocked",
    "OutOfMemory",
    "ReadOnly",
    "OperationInterrupted",
    "SystemIoFailure",
    "DatabaseCorrupt",
    "NotFound",
    "DiskFull",
    "CannotOpen",
    "FileLockingProtocolFailed",
    "SchemaChanged",
    "TooBig",
    "ConstraintViolation",
    "TypeMismatch",
    "ApiMisuse",
    "NoLargeFileSupport",
    "AuthorizationForStatementDenied",
    "ParameterOutOfRange",
    "NotADatabase",
    "Unknown",
];
const STORE_GATES: [&str; 8] = [
    "entry",
    "after-zero",
    "before-attempt",
    "after-wal",
    "after-busy",
    "before-settings",
    "after-settings",
    "after-restore",
];
const STORE_PHASES: [&str; 15] = [
    "entry",
    "busy-zero-enter",
    "busy-zero-return",
    "settings-enter",
    "settings-return",
    "timeout-restore-enter",
    "timeout-restore-return",
    "prepare-enter",
    "prepare-return",
    "query-enter",
    "row-enter",
    "row-return",
    "query-return",
    "finalize-enter",
    "finalize-return",
];
const STORE_MODES: [&str; 4] = ["unobserved", "wal", "memory", "other"];

#[derive(Clone, Copy)]
enum StoreReturned {
    Store,
    Io {
        kind: &'static str,
        raw: Option<i32>,
    },
    Sqlite,
    SqliteCode {
        primary: &'static str,
        extended: i32,
    },
}

#[derive(Clone, Copy)]
struct StoreRecord {
    stage: &'static str,
    returned: StoreReturned,
}

#[derive(Clone, Copy)]
enum PairRecognition {
    Unobserved,
    Recognized(StoreRecord),
    Unrecognized,
    Ambiguous,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PairReadState {
    Unobserved,
    Complete,
    Oversized,
    Missing,
    IoRefused,
    IdentityRefused,
}

impl PairReadState {
    fn label(self) -> &'static str {
        match self {
            Self::Unobserved => "unobserved",
            Self::Complete => "complete",
            Self::Oversized => "oversized",
            Self::Missing => "missing",
            Self::IoRefused => "io_refused",
            Self::IdentityRefused => "identity_refused",
        }
    }
}

#[derive(Clone, Copy)]
struct PairFact {
    state: PairReadState,
    bytes: Option<u32>,
    record: PairRecognition,
}

impl PairFact {
    const UNOBSERVED: Self = Self {
        state: PairReadState::Unobserved,
        bytes: None,
        record: PairRecognition::Unobserved,
    };
}

#[derive(Clone, Copy, Default)]
struct PairCollisionProof {
    no_child: bool,
    collision_preserved: bool,
}

#[derive(Clone, Copy, Default)]
struct PairProof {
    // Private fixed-size scalars for the four-original-object test, never rendered.
    identities: [Option<locron_core::filesystem::FileIdentity>; 2],
    zero_cursors: [bool; 2],
    root_status: Option<i32>,
    duplicates_held: bool,
    live_seen: bool,
    collision: PairCollisionProof,
}

#[derive(Clone, Copy)]
struct PairSnapshot {
    facts: [PairFact; 2],
    proof: PairProof,
}

struct PairSummary(PairSnapshot);

impl fmt::Display for PairSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (stream, fact) in ["stdout", "stderr"].into_iter().zip(self.0.facts) {
            write!(
                formatter,
                "daemon_output stream={stream} capture={} bytes=",
                fact.state.label()
            )?;
            if let Some(bytes) = fact.bytes {
                write!(formatter, "{bytes}")?;
            } else {
                formatter.write_str("unobserved")?;
            }
            match fact.record {
                PairRecognition::Unobserved => formatter.write_str(" record=unobserved")?,
                PairRecognition::Unrecognized => formatter.write_str(" record=unrecognized")?,
                PairRecognition::Ambiguous => formatter.write_str(" record=ambiguous")?,
                PairRecognition::Recognized(record) => {
                    write!(formatter, " record=recognized stage={}", record.stage)?;
                    match record.returned {
                        StoreReturned::Store => formatter.write_str(" category=store")?,
                        StoreReturned::Io { kind, raw } => {
                            write!(formatter, " category=io kind={kind} raw_os=")?;
                            if let Some(raw) = raw {
                                write!(formatter, "Some({raw})")?;
                            } else {
                                formatter.write_str("None")?;
                            }
                        }
                        StoreReturned::Sqlite => formatter.write_str(" category=sqlite")?,
                        StoreReturned::SqliteCode { primary, extended } => {
                            write!(
                                formatter,
                                " category=sqlite primary={primary} extended={extended}"
                            )?;
                        }
                    }
                }
            }
            formatter.write_str("\n")?;
        }
        Ok(())
    }
}

fn closed_label(value: &str, labels: &[&'static str]) -> Option<&'static str> {
    labels.iter().copied().find(|label| *label == value)
}

fn canonical_unsigned(value: &str, maximum: u64) -> Option<u64> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return None;
    }
    value
        .parse::<u64>()
        .ok()
        .filter(|number| *number <= maximum)
}

fn canonical_signed(value: &str) -> Option<i32> {
    let magnitude = value.strip_prefix('-').unwrap_or(value);
    canonical_unsigned(magnitude, u64::MAX)?;
    if value.starts_with('-') && magnitude == "0" {
        return None;
    }
    value.parse::<i32>().ok()
}

fn optional_unsigned(value: &str, maximum: u64) -> Option<()> {
    if value == "unobserved" {
        Some(())
    } else {
        canonical_unsigned(value, maximum).map(|_| ())
    }
}

fn optional_signed(value: &str) -> Option<()> {
    if value == "unobserved" {
        Some(())
    } else {
        canonical_signed(value).map(|_| ())
    }
}

fn optional_bool(value: &str) -> Option<()> {
    matches!(value, "unobserved" | "true" | "false").then_some(())
}

fn timestamp_pair(value: &str) -> Option<()> {
    let (first, second) = value.split_once(',')?;
    optional_unsigned(first, u64::MAX)?;
    optional_unsigned(second, u64::MAX)
}

struct StoreTokens<'a>(std::iter::Peekable<std::str::Split<'a, char>>);

impl<'a> StoreTokens<'a> {
    fn field(&mut self, key: &str) -> Option<&'a str> {
        let (actual, value) = self.0.next()?.split_once('=')?;
        (actual == key).then_some(value)
    }

    fn configuration(&mut self) -> Option<()> {
        canonical_unsigned(self.field("pid")?, u64::from(u32::MAX))?;
        if self.0.peek().copied() == Some("configuration=unobserved") {
            self.0.next();
            return self.0.next().is_none().then_some(());
        }
        let gate = self.field("gate")?;
        if gate != "unobserved" {
            closed_label(gate, &STORE_GATES)?;
        }
        closed_label(self.field("phase")?, &STORE_PHASES)?;
        canonical_unsigned(self.field("attempts")?, u64::MAX)?;
        canonical_unsigned(self.field("entry_rem_us")?, u64::MAX)?;
        canonical_unsigned(self.field("phase_us")?, u64::MAX)?;
        optional_unsigned(self.field("gate_us")?, u64::MAX)?;
        optional_unsigned(self.field("remaining_us")?, u64::MAX)?;
        optional_bool(self.field("autocommit")?)?;
        closed_label(self.field("mode")?, &STORE_MODES)?;
        optional_bool(self.field("done")?)?;
        optional_unsigned(self.field("observed_primary")?, u64::from(u8::MAX))?;
        optional_signed(self.field("observed_extended")?)?;
        for key in ["prepare_us", "query_us", "row_us", "finalize_us"] {
            timestamp_pair(self.field(key)?)?;
        }
        self.0.next().is_none().then_some(())
    }
}

fn parse_store_record(body: &[u8]) -> Option<StoreRecord> {
    if body.len() > 768 || body.iter().any(|byte| !(b' '..=b'~').contains(byte)) {
        return None;
    }
    let text = std::str::from_utf8(body).ok()?;
    let mut tokens = StoreTokens(text.split(' ').peekable());
    if tokens.0.next()? != "windows-store-open" {
        return None;
    }
    canonical_unsigned(tokens.field("operation")?, u64::MAX)?;
    let stage = closed_label(tokens.field("stage")?, &STORE_STAGES)?;
    let returned = match tokens.field("category")? {
        "store" => StoreReturned::Store,
        "io" => {
            let kind = closed_label(tokens.field("kind")?, &STORE_IO_KINDS)?;
            let raw = match tokens.field("raw_os")? {
                "None" => None,
                value => Some(canonical_signed(
                    value.strip_prefix("Some(")?.strip_suffix(')')?,
                )?),
            };
            StoreReturned::Io { kind, raw }
        }
        "sqlite" => {
            if tokens
                .0
                .peek()
                .is_some_and(|token| token.starts_with("primary="))
            {
                let primary = closed_label(tokens.field("primary")?, &STORE_SQLITE_KINDS)?;
                let extended = canonical_signed(tokens.field("extended")?)?;
                StoreReturned::SqliteCode { primary, extended }
            } else {
                StoreReturned::Sqlite
            }
        }
        _ => return None,
    };
    if matches!(stage, "sqlite-configure-wal" | "sqlite-configure-settings") {
        tokens.configuration()?;
    } else if tokens.0.next().is_some() {
        return None;
    }
    Some(StoreRecord { stage, returned })
}

fn recognize_store_record(bytes: &[u8]) -> PairRecognition {
    let mut candidates = 0_u32;
    let mut result = PairRecognition::Unobserved;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if line
            .windows(STORE_MARKER.len())
            .any(|window| window == STORE_MARKER)
        {
            candidates += u32::try_from(
                line.windows(STORE_MARKER.len())
                    .filter(|window| *window == STORE_MARKER)
                    .count(),
            )
            .unwrap_or(u32::MAX);
            result = line
                .strip_suffix(b"\n")
                .and_then(parse_store_record)
                .map_or(PairRecognition::Unrecognized, PairRecognition::Recognized);
        }
    }
    if candidates > 1 {
        PairRecognition::Ambiguous
    } else {
        result
    }
}

// Holds only the exact files/Command on the admitted native worker. The Command
// owns both stdout/stderr duplicates through the actual child root's exit.
struct PairedCapture {
    writers: [Option<GuardedFile>; 2],
    command: Option<Command>,
    reader: Option<GuardedFile>,
    lock: Option<DaemonLock>,
    sentinel: Option<GuardedFile>,
    proof: PairProof,
    root_exited: bool,
    collected: bool,
    attached: u8,
}

impl PairedCapture {
    fn empty() -> Self {
        Self {
            writers: [None, None],
            command: None,
            reader: None,
            lock: None,
            sentinel: None,
            proof: PairProof::default(),
            root_exited: false,
            collected: false,
            attached: 0,
        }
    }

    fn has_owners(&self) -> bool {
        self.writers.iter().any(Option::is_some)
            || self.command.is_some()
            || self.reader.is_some()
            || self.lock.is_some()
            || self.sentinel.is_some()
    }
}
// END paired daemon diagnostic domains.

// BEGIN independent producer domains. No raw carrier/key enters a public result.
#[cfg(debug_assertions)]
const WAKE_OPS: [&str; 22] = [
    "role_lock",
    "store_open",
    "settings",
    "enqueue",
    "hint",
    "begin_lifetime",
    "tick",
    "reconcile",
    "maintain",
    "capacity",
    "admit",
    "delay",
    "wait",
    "run_return",
    "attempt_begin",
    "mark_running",
    "execution",
    "cancel_poll",
    "cancel_signal",
    "completion",
    "failure_complete",
    "limit",
];
#[cfg(debug_assertions)]
const WAKE_EDGES: [&str; 9] = [
    "enter", "ok", "err", "some", "none", "wake", "timer", "external", "signal",
];

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum ProducerCapture {
    Complete,
    Empty,
    Truncated,
    Malformed,
    Oversized,
    Overflow,
    BindingRefused,
    Late,
    ReadError,
    Unreaped,
    Unobserved,
}
#[cfg(debug_assertions)]
impl ProducerCapture {
    fn label(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Empty => "empty",
            Self::Truncated => "truncated",
            Self::Malformed => "malformed",
            Self::Oversized => "oversized",
            Self::Overflow => "overflow",
            Self::BindingRefused => "binding_refused",
            Self::Late => "late",
            Self::ReadError => "read_error",
            Self::Unreaped => "unreaped",
            Self::Unobserved => "unobserved",
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Default)]
struct ProducerOp {
    enter: u16,
    returned: u16,
    edge: Option<u8>,
    value: Option<u64>,
    pending: bool,
}
#[cfg(debug_assertions)]
impl ProducerOp {
    fn observe(&mut self, row: WakeRow, retry: bool) -> bool {
        if row.edge == 0 {
            if self.pending || (self.returned != 0 && !retry) {
                return false;
            }
            self.enter = row.sequence;
            self.pending = true;
        } else {
            if !self.pending {
                return false;
            }
            self.returned = row.sequence;
            self.edge = Some(row.edge);
            self.value = row.value;
            self.pending = false;
        }
        true
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
struct WakeRow {
    sequence: u16,
    tick: u8,
    attempt: u8,
    op: u8,
    edge: u8,
    value: Option<u64>,
    kind: &'static str,
    raw: Option<i32>,
    time: u64,
}
#[cfg(debug_assertions)]
#[derive(Clone, Copy, Default)]
struct AttemptLedger {
    digest: [u8; 32],
    ordinal: u32,
    binding: u16,
    begun: bool,
    mark: ProducerOp,
    execution: ProducerOp,
    poll: ProducerOp,
    signal: u16,
    signalled_poll: u16,
    completion: ProducerOp,
    failure: ProducerOp,
    failure_kind: Option<u64>,
}
#[cfg(debug_assertions)]
impl AttemptLedger {
    fn observe(&mut self, row: WakeRow) -> bool {
        match row.op {
            14 => {
                if self.begun || row.edge != 0 {
                    return false;
                }
                self.begun = true;
                true
            }
            15 => {
                self.begun
                    && self.execution.enter == 0
                    && self.mark.observe(row, self.mark.edge == Some(2))
            }
            16 => {
                self.begun
                    && !self.mark.pending
                    && self.mark.edge == Some(1)
                    && self.mark.value == Some(1)
                    && (row.edge == 0 || !self.poll.pending)
                    && self.execution.observe(row, false)
            }
            17 => self.execution.pending && self.poll.observe(row, self.poll.returned != 0),
            18 => {
                if !self.execution.pending
                    || self.poll.pending
                    || self.poll.edge != Some(1)
                    || self.poll.value != Some(1)
                    || self.poll.returned == self.signalled_poll
                {
                    return false;
                }
                self.signal = row.sequence;
                self.signalled_poll = self.poll.returned;
                true
            }
            19 => {
                !self.execution.pending
                    && self.execution.edge == Some(1)
                    && self.completion.observe(
                        row,
                        self.completion.edge == Some(2) && self.completion.value == Some(1),
                    )
            }
            20 => {
                let runner_error = !self.execution.pending && self.execution.edge == Some(2);
                let conflict = !self.completion.pending
                    && self.completion.edge == Some(2)
                    && self.completion.value == Some(2);
                if row.edge == 0 {
                    if self
                        .failure_kind
                        .is_some_and(|kind| row.value != Some(kind))
                    {
                        return false;
                    }
                    self.failure_kind = row.value;
                }
                (runner_error || conflict)
                    && (!conflict || row.edge != 0 || row.value == Some(2))
                    && self.failure.observe(
                        row,
                        runner_error
                            && self.failure.edge == Some(2)
                            && self.failure.value == Some(1),
                    )
            }
            _ => false,
        }
    }
}

#[cfg(debug_assertions)]
struct WakeLedger {
    sequence: u16,
    tick: u8,
    count: u8,
    awaiting_begin: Option<u8>,
    time: Option<u64>,
    latest: Option<WakeRow>,
    ops: [ProducerOp; 14],
    attempts: [AttemptLedger; 64],
    hint_kind: &'static str,
    hint_raw: Option<i32>,
}
#[cfg(debug_assertions)]
impl WakeLedger {
    fn empty() -> Self {
        Self {
            sequence: 0,
            tick: 0,
            count: 0,
            awaiting_begin: None,
            time: None,
            latest: None,
            ops: [ProducerOp::default(); 14],
            attempts: [AttemptLedger::default(); 64],
            hint_kind: "none",
            hint_raw: None,
        }
    }
    fn sequence(&mut self, sequence: u16, time: u64) -> bool {
        if sequence != self.sequence + 1
            || sequence > 256
            || self.time.is_some_and(|previous| time < previous)
        {
            return false;
        }
        self.sequence = sequence;
        self.time = Some(time);
        true
    }
    fn observe(&mut self, row: WakeRow, run: bool) -> bool {
        if self
            .awaiting_begin
            .is_some_and(|epoch| row.op != 14 || row.attempt != epoch)
        {
            return false;
        }
        if row.op >= 14 && row.op <= 20 {
            if run || row.tick != 0 || row.attempt == 0 || row.attempt > self.count {
                return false;
            }
            if !self.attempts[usize::from(row.attempt - 1)].observe(row) {
                return false;
            }
            if row.op == 14 {
                self.awaiting_begin = None;
            }
        } else {
            if row.attempt != 0 {
                return false;
            }
            let op = usize::from(row.op);
            if run {
                if row.tick != 0 || ![3, 4, 13].contains(&op) {
                    return false;
                }
                if op == 13 {
                    if self.ops[13].returned != 0
                        || (row.edge == 1
                            && (self.ops[3].edge != Some(1) || self.ops[4].returned == 0))
                    {
                        return false;
                    }
                    self.ops[13].returned = row.sequence;
                    self.ops[13].edge = Some(row.edge);
                } else {
                    if op == 4 && self.ops[3].edge != Some(1) {
                        return false;
                    }
                    if !self.ops[op].observe(row, false) {
                        return false;
                    }
                    if op == 4 && row.edge != 0 {
                        self.hint_kind = row.kind;
                        self.hint_raw = row.raw;
                    }
                }
            } else if [0, 1, 2, 5].contains(&op) {
                if row.tick != 0 || self.tick != 0 {
                    return false;
                }
                let preceding = match op {
                    1 => Some(0),
                    2 => Some(1),
                    5 => Some(2),
                    _ => None,
                };
                if preceding.is_some_and(|index| self.ops[index].edge != Some(1))
                    || !self.ops[op].observe(row, false)
                {
                    return false;
                }
            } else {
                if ![6, 7, 8, 9, 10, 11, 12].contains(&op) || self.ops[5].edge != Some(1) {
                    return false;
                }
                if op == 6 && row.edge == 0 {
                    if self.tick == 255
                        || row.tick != self.tick + 1
                        || self.ops[6].pending
                        || (self.tick > 0
                            && (self.ops[12].pending || !matches!(self.ops[12].edge, Some(5 | 6))))
                    {
                        return false;
                    }
                    self.tick = row.tick;
                    for index in [7, 8, 9, 10, 11, 12] {
                        self.ops[index] = ProducerOp::default();
                    }
                }
                if row.tick == 0 || row.tick != self.tick {
                    return false;
                }
                if [7, 8, 9, 10].contains(&op) && !self.ops[6].pending {
                    return false;
                }
                match op {
                    6 if row.edge != 0 => {
                        if self.ops[7].pending
                            || self.ops[8].pending
                            || self.ops[10].pending
                            || self.ops[7].returned == 0
                        {
                            return false;
                        }
                        if row.edge == 1
                            && (self.ops[9].returned == 0
                                || (self.ops[9].value != Some(0) && self.ops[10].edge != Some(1)))
                        {
                            return false;
                        }
                    }
                    8 if self.ops[7].edge != Some(1) => return false,
                    9 if self.ops[8].returned == 0 => return false,
                    10 if self.ops[9].returned == 0 || self.ops[9].value == Some(0) => {
                        return false;
                    }
                    11 if self.ops[6].pending || self.ops[6].returned == 0 => return false,
                    12 if self.ops[11].pending
                        || self.ops[11].returned == 0
                        || (row.edge == 0 && row.value != self.ops[11].value) =>
                    {
                        return false;
                    }
                    _ => {}
                }
                if op == 9 {
                    if self.ops[9].returned != 0 {
                        return false;
                    }
                    self.ops[9].returned = row.sequence;
                    self.ops[9].edge = Some(row.edge);
                    self.ops[9].value = row.value;
                } else if !self.ops[op].observe(row, op == 6 && self.ops[6].returned != 0) {
                    return false;
                }
            }
        }
        self.latest = Some(row);
        true
    }
}

#[cfg(debug_assertions)]
fn wake_unsigned(text: &str, maximum: u64) -> Option<u64> {
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    text.parse::<u64>().ok().filter(|value| *value <= maximum)
}
#[cfg(debug_assertions)]
fn wake_value(text: &str) -> Result<Option<u64>, InvalidScalar> {
    if text == "None" {
        Ok(None)
    } else {
        let number = text
            .strip_prefix("Some(")
            .ok_or(InvalidScalar)?
            .strip_suffix(')')
            .ok_or(InvalidScalar)?;
        Ok(Some(wake_unsigned(number, u64::MAX).ok_or(InvalidScalar)?))
    }
}
#[cfg(debug_assertions)]
fn wake_signed(text: &str) -> Result<Option<i32>, InvalidScalar> {
    if text == "None" {
        return Ok(None);
    }
    let number = text
        .strip_prefix("Some(")
        .ok_or(InvalidScalar)?
        .strip_suffix(')')
        .ok_or(InvalidScalar)?;
    let absolute = number.strip_prefix('-').unwrap_or(number);
    let value = wake_unsigned(absolute, 2_147_483_648).ok_or(InvalidScalar)?;
    if number == "-0" {
        return Err(InvalidScalar);
    }
    let signed = if number.starts_with('-') {
        -i64::try_from(value).map_err(|_| InvalidScalar)?
    } else {
        i64::try_from(value).map_err(|_| InvalidScalar)?
    };
    Ok(Some(i32::try_from(signed).map_err(|_| InvalidScalar)?))
}
#[cfg(debug_assertions)]
fn wake_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut result = [0; N];
    for (destination, pair) in result
        .iter_mut()
        .zip(text.as_bytes().as_chunks::<2>().0.iter())
    {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        *destination = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(result)
}
#[cfg(debug_assertions)]
fn wake_fields<'a, const N: usize>(
    line: &'a str,
    marker: &str,
    names: [&str; N],
) -> Option<[&'a str; N]> {
    let mut tokens = line.split(' ');
    if tokens.next()? != marker {
        return None;
    }
    let mut values = [""; N];
    for (value, name) in values.iter_mut().zip(names) {
        let token = tokens.next()?;
        let (key, text) = token.split_once('=')?;
        if key != name || text.is_empty() {
            return None;
        }
        *value = text;
    }
    if tokens.next().is_some() {
        return None;
    }
    Some(values)
}

#[cfg(debug_assertions)]
fn wake_shape(row: WakeRow) -> bool {
    let absent = row.value.is_none();
    if (row.op != 4 || row.edge != 2) && (row.kind != "none" || row.raw.is_some()) {
        return false;
    }
    match row.op {
        14 => row.edge == 0 && absent,
        18 => row.edge == 1 && absent,
        15 | 17 => match row.edge {
            0 | 2 => absent,
            1 => matches!(row.value, Some(0 | 1)),
            _ => false,
        },
        16 => match row.edge {
            0 | 2 => absent,
            1 => row.value.is_some_and(|value| (1..=6).contains(&value)),
            _ => false,
        },
        19 | 20 => match row.edge {
            0 if row.op == 20 => matches!(row.value, Some(1 | 2)),
            0 | 1 => absent,
            2 => matches!(row.value, Some(1 | 2)),
            _ => false,
        },
        9 => row.edge == 1 && row.value.is_some(),
        7 | 10 if row.edge == 1 => row.value.is_some(),
        11 => {
            if row.edge == 0 {
                absent
            } else {
                [2, 3, 4].contains(&row.edge) && row.value.is_some_and(|value| value <= 30_000_000)
            }
        }
        12 => {
            if row.edge == 0 {
                row.value.is_some_and(|value| value <= 30_000_000)
            } else {
                (5..=8).contains(&row.edge) && absent
            }
        }
        13 => [1, 2].contains(&row.edge) && absent,
        21 => row.edge == 2 && absent && row.tick == 0 && row.attempt == 0,
        _ => [0, 1, 2].contains(&row.edge) && absent,
    }
}

#[cfg(debug_assertions)]
fn wake_digest(context: [u8; 16], uuid: [u8; 16], ordinal: u32) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"locron-wake-attempt/v1\0");
    hash.update(context);
    hash.update(uuid);
    hash.update(ordinal.to_be_bytes());
    hash.finalize().into()
}

#[cfg(debug_assertions)]
fn decode_wake(bytes: &[u8], context: [u8; 16], run: bool) -> Result<WakeLedger, ProducerCapture> {
    if bytes.len() > 65_536 {
        return Err(ProducerCapture::Oversized);
    }
    if !bytes.is_empty() && !bytes.ends_with(b"\n") {
        return Err(ProducerCapture::Truncated);
    }
    let mut ledger = WakeLedger::empty();
    let mut terminal = false;
    for line in bytes.split_inclusive(|byte| *byte == b'\n') {
        if !line
            .windows(b"locron_wake_".len())
            .any(|window| window == b"locron_wake_")
        {
            continue;
        }
        if terminal || !line.is_ascii() || line.contains(&b'\r') {
            return Err(ProducerCapture::Malformed);
        }
        let text = std::str::from_utf8(line.strip_suffix(b"\n").ok_or(ProducerCapture::Truncated)?)
            .map_err(|_| ProducerCapture::Malformed)?;
        if text.starts_with("locron_wake_bind/v1 ") {
            if run || line.len() > 203 || ledger.awaiting_begin.is_some() {
                return Err(ProducerCapture::BindingRefused);
            }
            let fields = wake_fields(
                text,
                "locron_wake_bind/v1",
                ["ctx", "role", "seq", "attempt", "ordinal", "corr", "t_us"],
            )
            .ok_or(ProducerCapture::Malformed)?;
            let (
                Some(observed_context),
                "daemon",
                Some(sequence),
                Some(epoch),
                Some(ordinal),
                Some(digest),
                Some(time),
            ) = (
                wake_hex::<16>(fields[0]),
                fields[1],
                wake_unsigned(fields[2], 255),
                wake_unsigned(fields[3], 64),
                wake_unsigned(fields[4], u64::from(u32::MAX)),
                wake_hex::<32>(fields[5]),
                wake_unsigned(fields[6], u64::MAX),
            )
            else {
                return Err(ProducerCapture::BindingRefused);
            };
            if observed_context != context
                || epoch == 0
                || epoch != u64::from(ledger.count) + 1
                || ordinal == 0
                || ledger.attempts[..usize::from(ledger.count)]
                    .iter()
                    .any(|entry| entry.digest == digest && u64::from(entry.ordinal) == ordinal)
                || !ledger.sequence(
                    u16::try_from(sequence).map_err(|_| ProducerCapture::Malformed)?,
                    time,
                )
            {
                return Err(ProducerCapture::BindingRefused);
            }
            ledger.attempts[usize::from(ledger.count)] = AttemptLedger {
                digest,
                ordinal: u32::try_from(ordinal).map_err(|_| ProducerCapture::BindingRefused)?,
                binding: u16::try_from(sequence).map_err(|_| ProducerCapture::Malformed)?,
                ..AttemptLedger::default()
            };
            ledger.count += 1;
            ledger.awaiting_begin = Some(ledger.count);
            continue;
        }
        if line.len() > 241 {
            return Err(ProducerCapture::Malformed);
        }
        let fields = wake_fields(
            text,
            "locron_wake_phase/v2",
            [
                "ctx", "role", "seq", "tick", "attempt", "op", "edge", "value", "kind", "raw",
                "t_us",
            ],
        )
        .ok_or(ProducerCapture::Malformed)?;
        let Some(observed_context) = wake_hex::<16>(fields[0]) else {
            return Err(ProducerCapture::BindingRefused);
        };
        if observed_context != context || fields[1] != if run { "run" } else { "daemon" } {
            return Err(ProducerCapture::BindingRefused);
        }
        let row = WakeRow {
            sequence: u16::try_from(
                wake_unsigned(fields[2], 256).ok_or(ProducerCapture::Malformed)?,
            )
            .map_err(|_| ProducerCapture::Malformed)?,
            tick: u8::try_from(wake_unsigned(fields[3], 255).ok_or(ProducerCapture::Malformed)?)
                .map_err(|_| ProducerCapture::Malformed)?,
            attempt: u8::try_from(wake_unsigned(fields[4], 64).ok_or(ProducerCapture::Malformed)?)
                .map_err(|_| ProducerCapture::Malformed)?,
            op: u8::try_from(
                WAKE_OPS
                    .iter()
                    .position(|op| *op == fields[5])
                    .ok_or(ProducerCapture::Malformed)?,
            )
            .map_err(|_| ProducerCapture::Malformed)?,
            edge: u8::try_from(
                WAKE_EDGES
                    .iter()
                    .position(|edge| *edge == fields[6])
                    .ok_or(ProducerCapture::Malformed)?,
            )
            .map_err(|_| ProducerCapture::Malformed)?,
            value: wake_value(fields[7]).map_err(|_| ProducerCapture::Malformed)?,
            kind: if ["none", "unknown"].contains(&fields[8]) {
                if fields[8] == "none" {
                    "none"
                } else {
                    "unknown"
                }
            } else {
                STORE_IO_KINDS
                    .iter()
                    .copied()
                    .find(|kind| *kind == fields[8])
                    .ok_or(ProducerCapture::Malformed)?
            },
            raw: wake_signed(fields[9]).map_err(|_| ProducerCapture::Malformed)?,
            time: wake_unsigned(fields[10], u64::MAX).ok_or(ProducerCapture::Malformed)?,
        };
        if !wake_shape(row) || !ledger.sequence(row.sequence, row.time) {
            return Err(ProducerCapture::Malformed);
        }
        if row.op == 21 {
            terminal = true;
            continue;
        }
        if row.sequence == 256 || !ledger.observe(row, run) {
            return Err(ProducerCapture::Malformed);
        }
    }
    if terminal {
        Err(ProducerCapture::Overflow)
    } else {
        Ok(ledger)
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
struct ProducerFact {
    capture: ProducerCapture,
    bytes: Option<u32>,
    records: Option<u16>,
    time: Option<u64>,
    latest: Option<WakeRow>,
    enqueue: ProducerOp,
    hint: ProducerOp,
    kind: &'static str,
    raw: Option<i32>,
    matched: Option<(u8, AttemptLedger)>,
    binding_refused: bool,
}
#[cfg(debug_assertions)]
impl ProducerFact {
    const UNOBSERVED: Self = Self {
        capture: ProducerCapture::Unobserved,
        bytes: None,
        records: None,
        time: None,
        latest: None,
        enqueue: ProducerOp {
            enter: 0,
            returned: 0,
            edge: None,
            value: None,
            pending: false,
        },
        hint: ProducerOp {
            enter: 0,
            returned: 0,
            edge: None,
            value: None,
            pending: false,
        },
        kind: "none",
        raw: None,
        matched: None,
        binding_refused: false,
    };
    fn from_bytes(bytes: &[u8], context: [u8; 16], run: bool, uuid: Option<[u8; 16]>) -> Self {
        let count = u32::try_from(bytes.len()).ok();
        let ledger = match decode_wake(bytes, context, run) {
            Ok(ledger) => ledger,
            Err(capture) => {
                return Self {
                    capture,
                    bytes: count,
                    binding_refused: capture == ProducerCapture::BindingRefused,
                    ..Self::UNOBSERVED
                };
            }
        };
        let matched = uuid.and_then(|uuid| {
            ledger.attempts[..usize::from(ledger.count)]
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.digest == wake_digest(context, uuid, entry.ordinal))
                .max_by_key(|(_, entry)| entry.binding)
                .and_then(|(index, entry)| {
                    u8::try_from(index + 1).ok().map(|epoch| (epoch, *entry))
                })
        });
        Self {
            capture: if ledger.sequence == 0 {
                ProducerCapture::Empty
            } else {
                ProducerCapture::Complete
            },
            bytes: count,
            records: Some(ledger.sequence),
            time: ledger.time,
            latest: ledger.latest,
            enqueue: ledger.ops[3],
            hint: ledger.ops[4],
            kind: ledger.hint_kind,
            raw: ledger.hint_raw,
            matched,
            binding_refused: false,
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
enum CancelReply {
    Unobserved,
    Requested,
    BeforeExecution,
}
#[cfg(debug_assertions)]
impl CancelReply {
    fn label(self) -> &'static str {
        match self {
            Self::Unobserved => "unobserved",
            Self::Requested => "requested",
            Self::BeforeExecution => "before_execution",
        }
    }
}
#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
struct ProducerSnapshot {
    facts: [ProducerFact; 2],
    cancel: CancelReply,
    released: bool,
    identities: [Option<locron_core::filesystem::FileIdentity>; 2],
    zero_cursors: [bool; 2],
    run_status: Option<i32>,
    run_queued: bool,
}
#[cfg(debug_assertions)]
impl ProducerSnapshot {
    const UNOBSERVED: Self = Self {
        facts: [ProducerFact::UNOBSERVED; 2],
        cancel: CancelReply::Unobserved,
        released: false,
        identities: [None; 2],
        zero_cursors: [false; 2],
        run_status: None,
        run_queued: false,
    };
}

#[cfg(debug_assertions)]
struct ProducerSummary {
    snapshot: ProducerSnapshot,
    cancel: bool,
}
#[cfg(debug_assertions)]
fn producer_number(
    formatter: &mut fmt::Formatter<'_>,
    value: Option<impl fmt::Display>,
) -> fmt::Result {
    if let Some(value) = value {
        write!(formatter, "{value}")
    } else {
        formatter.write_str("unobserved")
    }
}
#[cfg(debug_assertions)]
fn producer_edge(op: ProducerOp) -> (&'static str, u16) {
    if op.enter > op.returned {
        ("enter", op.enter)
    } else if let Some(edge) = op.edge {
        (WAKE_EDGES[usize::from(edge)], op.returned)
    } else {
        ("unobserved", 0)
    }
}
#[cfg(debug_assertions)]
fn producer_result(op: ProducerOp, domain: u8) -> &'static str {
    if op.returned == 0 {
        return "unobserved";
    }
    if op.edge == Some(2) {
        return if domain == 2 {
            match op.value {
                Some(1) => "transient",
                Some(2) => "conflict",
                _ => "unobserved",
            }
        } else {
            "err"
        };
    }
    match domain {
        0 => match op.value {
            Some(0) => "false",
            Some(1) => "true",
            _ => "unobserved",
        },
        1 => match op.value {
            Some(1) => "succeeded",
            Some(2) => "failed_retryable",
            Some(3) => "failed",
            Some(4) => "timed_out",
            Some(5) => "cancelled",
            Some(6) => "unconfirmed",
            _ => "unobserved",
        },
        _ => "ok",
    }
}
#[cfg(debug_assertions)]
impl fmt::Display for ProducerSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let run = self.snapshot.facts[1];
        let (enqueue, enqueue_sequence) = producer_edge(run.enqueue);
        let (hint, hint_sequence) = producer_edge(run.hint);
        if self.cancel {
            write!(
                formatter,
                "wake_request role=cancel lane=cleanup_diagnostic outcome={} run_capture={} bytes=",
                self.snapshot.cancel.label(),
                run.capture.label()
            )?;
        } else {
            write!(
                formatter,
                "wake_producer role=run lane=cleanup_diagnostic capture={} bytes=",
                run.capture.label()
            )?;
        }
        producer_number(formatter, run.bytes)?;
        formatter.write_str(" records=")?;
        producer_number(formatter, run.records)?;
        if self.cancel {
            write!(
                formatter,
                " run_enqueue={enqueue}@{enqueue_sequence} run_hint={hint}@{hint_sequence} run_kind={} run_raw=",
                run.kind
            )?;
        } else {
            write!(
                formatter,
                " enqueue={enqueue}@{enqueue_sequence} hint={hint}@{hint_sequence} kind={} raw=",
                run.kind
            )?;
        }
        if let Some(raw) = run.raw {
            write!(formatter, "Some({raw})")?;
        } else {
            formatter.write_str("None")?;
        }
        if self.cancel {
            formatter.write_str(" cancel_hint=unobserved")?;
        }
        formatter.write_str(" t_us=")?;
        producer_number(formatter, run.time)?;
        formatter.write_str("\n")?;
        let daemon = self.snapshot.facts[0];
        write!(
            formatter,
            "wake_attempt role=daemon lane=cleanup_diagnostic capture={} bytes=",
            daemon.capture.label()
        )?;
        producer_number(formatter, daemon.bytes)?;
        formatter.write_str(" records=")?;
        producer_number(formatter, daemon.records)?;
        if let Some(row) = daemon.latest {
            write!(
                formatter,
                " producer={}/{}@{}:{}",
                WAKE_OPS[usize::from(row.op)],
                WAKE_EDGES[usize::from(row.edge)],
                row.tick,
                row.sequence
            )?;
        } else {
            formatter.write_str(" producer=unobserved/unobserved@0:0")?;
        }
        let bound = if daemon.binding_refused {
            "refused"
        } else if daemon.matched.is_some() {
            "matched"
        } else {
            "unobserved"
        };
        write!(formatter, " bound={bound} a=")?;
        producer_number(formatter, daemon.matched.map(|(epoch, _)| epoch))?;
        let attempt = daemon
            .matched
            .map(|(_, attempt)| attempt)
            .unwrap_or_default();
        for (name, op, domain) in [
            ("mark", attempt.mark, 0),
            ("exec", attempt.execution, 1),
            ("poll", attempt.poll, 0),
            ("completion", attempt.completion, 2),
            ("failure", attempt.failure, 2),
        ] {
            if name == "completion" {
                formatter.write_str(" signal=")?;
                producer_number(formatter, (attempt.signal != 0).then_some(attempt.signal))?;
            }
            write!(
                formatter,
                " {name}={}@{}:{}",
                producer_result(op, domain),
                op.enter,
                op.returned
            )?;
        }
        formatter.write_str(" t_us=")?;
        producer_number(formatter, daemon.time)
    }
}

#[cfg(debug_assertions)]
struct Producers {
    enabled: bool,
    positive: bool,
    context: [u8; 16],
    writers: [Option<GuardedFile>; 2],
    readers: [Option<GuardedFile>; 2],
    commands: [Option<Command>; 2],
    snapshot: ProducerSnapshot,
    uuid: Option<[u8; 16]>,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
enum ProducerKind {
    Wake,
    Cancel,
    QueuedControl,
}
#[cfg(debug_assertions)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum InitialRunState {
    Partial,
    Prepared,
    Dispatched,
}
#[cfg(debug_assertions)]
struct PreparedInitialRunCommand {
    command: Command,
    state: InitialRunState,
}
#[cfg(debug_assertions)]
impl PreparedInitialRunCommand {
    fn unused(&self) -> Result<(), Code> {
        if self.state == InitialRunState::Prepared {
            Ok(())
        } else {
            Err(Code::Native)
        }
    }
    fn dispatch(&mut self) -> Result<&mut Command, Code> {
        self.unused()?;
        self.state = InitialRunState::Dispatched;
        Ok(&mut self.command)
    }
}
enum CaptureSource<'a> {
    Default(&'a mut Command),
    #[cfg(debug_assertions)]
    InitialRunOwner,
}
#[cfg(debug_assertions)]
impl Producers {
    fn empty() -> Self {
        Self {
            enabled: false,
            positive: false,
            context: [0; 16],
            writers: [None, None],
            readers: [None, None],
            commands: [None, None],
            snapshot: ProducerSnapshot::UNOBSERVED,
            uuid: None,
        }
    }
    fn has_owners(&self) -> bool {
        self.writers.iter().any(Option::is_some)
            || self.readers.iter().any(Option::is_some)
            || self.commands.iter().any(Option::is_some)
    }
}
#[cfg(debug_assertions)]
enum ProducerLane {
    FailureDiagnostic,
    PositiveControl,
}
#[cfg(debug_assertions)]
struct ProducerPermit {
    deadline: Instant,
    lane: ProducerLane,
}
#[cfg(debug_assertions)]
impl ProducerPermit {
    fn check(&self, control: &Control) -> Result<(), Code> {
        if matches!(self.lane, ProducerLane::PositiveControl) {
            control.check(self.deadline)?;
        }
        if Instant::now() >= self.deadline {
            Err(Code::Expired)
        } else {
            Ok(())
        }
    }
}
// END independent producer domains.

/// Fixed non-sensitive result; a late result cannot qualify successful cleanup.
pub struct CaseResult {
    code: Code,
    phase: Phase,
    flags: u8,
    frame_bytes: u32,
    elapsed_us: u128,
    observations: ObservationSnapshot,
    stdout_bytes: u32,
    cli_live_seen: bool,
    calls: CallSnapshot,
    capture_proof: u8,
    paired: PairSnapshot,
}

impl CaseResult {
    /// Separate closed scalar summaries; the current CaseResult line stays literal.
    #[must_use]
    pub fn daemon_output(&self) -> impl fmt::Display {
        PairSummary(self.paired)
    }

    /// True only for work and confirmed owned cleanup inside the caller's clock.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.code == Code::Success && self.flags & (REAPED | CLEANED) == (REAPED | CLEANED)
    }
}

impl fmt::Display for CaseResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "code={:?} phase={:?} flags={} frame_bytes={} elapsed_us={}",
            self.code, self.phase, self.flags, self.frame_bytes, self.elapsed_us
        )?;
        fmt::Display::fmt(&self.observations, formatter)?;
        formatter.write_str(" stdout_bytes=")?;
        if self.stdout_bytes == UNOBSERVED_BYTES {
            formatter.write_str("unobserved")?;
        } else {
            write!(formatter, "{}", self.stdout_bytes)?;
        }
        write!(
            formatter,
            " cli_live_seen={}",
            if self.cli_live_seen {
                "true"
            } else {
                "unobserved"
            }
        )?;
        fmt::Display::fmt(&self.calls, formatter)
    }
}

impl fmt::Debug for CaseResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

struct Control {
    deadline: Instant,
    entered: Instant,
    admitted: AtomicBool,
    phase: AtomicU8,
    flags: AtomicU8,
    frame_bytes: AtomicU32,
    case_expiry: AtomicU64,
    probe_expiry: AtomicU64,
    probe_complete: AtomicU64,
    observations: Observations,
    stdout_bytes: AtomicU32,
    cli_live_seen: AtomicBool,
    calls: CallObservations,
    paired_facts: [OnceLock<PairFact>; 2],
    paired_proof: OnceLock<PairProof>,
    #[cfg(debug_assertions)]
    producer_diagnostic: OnceLock<ProducerSnapshot>,
}

impl Control {
    fn new(entered: Instant, deadline: Instant) -> Arc<Self> {
        Arc::new(Self {
            deadline,
            entered,
            admitted: AtomicBool::new(true),
            phase: AtomicU8::new(Phase::Setup as u8),
            flags: AtomicU8::new(0),
            frame_bytes: AtomicU32::new(0),
            case_expiry: AtomicU64::new(0),
            probe_expiry: AtomicU64::new(0),
            probe_complete: AtomicU64::new(0),
            observations: Observations::new(),
            stdout_bytes: AtomicU32::new(UNOBSERVED_BYTES),
            cli_live_seen: AtomicBool::new(false),
            calls: CallObservations::new(),
            paired_facts: [OnceLock::new(), OnceLock::new()],
            paired_proof: OnceLock::new(),
            #[cfg(debug_assertions)]
            producer_diagnostic: OnceLock::new(),
        })
    }

    fn refuse(&self, code: Code) -> Code {
        self.admitted.store(false, Ordering::Release);
        code
    }

    fn encode(&self, instant: Instant) -> Result<u64, Code> {
        let nanos = instant
            .checked_duration_since(self.entered)
            .and_then(|duration| u64::try_from(duration.as_nanos()).ok())
            .filter(|nanos| *nanos <= OUTER_NANOS)
            .ok_or_else(|| self.refuse(Code::Native))?;
        Ok(nanos + 1)
    }

    fn decode(&self, slot: u64) -> Result<Instant, Code> {
        let nanos = slot
            .checked_sub(1)
            .filter(|nanos| *nanos <= OUTER_NANOS)
            .ok_or_else(|| self.refuse(Code::Native))?;
        self.entered
            .checked_add(Duration::from_nanos(nanos))
            .filter(|deadline| *deadline <= self.deadline)
            .ok_or_else(|| self.refuse(Code::Native))
    }

    fn publish_case(&self, deadline: Instant) -> Result<(), Code> {
        let slot = self.encode(deadline)?;
        self.case_expiry
            .compare_exchange(0, slot, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        self.check(self.deadline)
    }

    fn effective_deadline(&self) -> Result<Instant, Code> {
        let slot = self.case_expiry.load(Ordering::Acquire);
        if slot == 0 {
            Ok(self.deadline)
        } else {
            self.decode(slot)
        }
    }

    fn publish_probe(&self, deadline: Instant) -> Result<(), Code> {
        let case = self.effective_deadline()?;
        if self.case_expiry.load(Ordering::Acquire) == 0 || deadline > case {
            return Err(self.refuse(Code::Native));
        }
        let slot = self.encode(deadline)?;
        self.probe_expiry
            .compare_exchange(0, slot, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        self.check(deadline)
    }

    fn observation_deadline(&self, requested: Instant) -> Result<Instant, Code> {
        let case = self.effective_deadline()?;
        let mut deadline = requested.min(case).min(self.deadline);
        let seal = self.probe_complete.load(Ordering::Acquire);
        // Acquiring the commit first also observes its preceding expiry write.
        let expiry = self.probe_expiry.load(Ordering::Acquire);
        if expiry == 0 {
            if seal != 0 {
                return Err(self.refuse(Code::Native));
            }
        } else {
            let probe = self.decode(expiry)?;
            if probe > case {
                return Err(self.refuse(Code::Native));
            }
            if seal == 0 {
                deadline = deadline.min(probe);
            } else if self.decode(seal)? >= probe {
                return Err(self.refuse(Code::Expired));
            }
        }
        Ok(deadline)
    }

    fn seal_probe(&self, completed: Instant) -> Result<(), Code> {
        self.check(self.deadline)?;
        let expiry = self.probe_expiry.load(Ordering::Acquire);
        if expiry == 0 || completed >= self.decode(expiry)? {
            return Err(self.refuse(Code::Expired));
        }
        let seal = self.encode(completed)?;
        self.probe_complete
            .compare_exchange(0, seal, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| self.refuse(Code::Native))?;
        // A seal never reopens admission after the driver has observed expiry.
        self.check(self.deadline)
    }

    fn check(&self, deadline: Instant) -> Result<(), Code> {
        let observed_deadline = self.observation_deadline(deadline)?;
        if !self.admitted.load(Ordering::Acquire) || Instant::now() >= observed_deadline {
            return Err(self.refuse(Code::Expired));
        }
        Ok(())
    }

    fn step(&self, phase: Phase, deadline: Instant) -> Result<(), Code> {
        self.phase.store(phase as u8, Ordering::Release);
        self.check(deadline)
    }

    fn flag(&self, flag: u8) {
        self.flags.fetch_or(flag, Ordering::Release);
    }

    fn snapshot(&self, code: Code) -> CaseResult {
        CaseResult {
            code,
            phase: Phase::from_slot(self.phase.load(Ordering::Acquire)),
            flags: self.flags.load(Ordering::Acquire),
            frame_bytes: self.frame_bytes.load(Ordering::Acquire),
            elapsed_us: self.entered.elapsed().as_micros(),
            observations: self.observations.snapshot(),
            stdout_bytes: self.stdout_bytes.load(Ordering::Acquire),
            cli_live_seen: self.cli_live_seen.load(Ordering::Acquire),
            calls: self.calls.snapshot(),
            capture_proof: self.calls.capture_proof.load(Ordering::Acquire),
            paired: PairSnapshot {
                facts: std::array::from_fn(|index| {
                    self.paired_facts[index]
                        .get()
                        .copied()
                        .unwrap_or(PairFact::UNOBSERVED)
                }),
                proof: self.paired_proof.get().copied().unwrap_or_default(),
            },
        }
    }
}

// The only driver-visible state is bounded counters/result. Exact native owners
// remain on this thread, including while a synchronous operation cannot return.
struct CaseAdmission {
    control: Arc<Control>,
    command: Option<SyncSender<CaseKind>>,
    result: Receiver<CaseResult>,
    thread: Option<thread::JoinHandle<()>>,
}

impl CaseAdmission {
    fn dispatch(&mut self, kind: CaseKind) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        let sender = self
            .command
            .take()
            .ok_or_else(|| self.control.refuse(Code::Native))?;
        // The sole payload is a fixed scalar. Full/disconnect owns no fixture.
        let sent = sender.try_send(kind);
        self.control.check(self.control.deadline)?;
        sent.map_err(|_| self.control.refuse(Code::Native))
    }

    fn receive_until(&self, deadline: Instant) -> CaseResult {
        loop {
            if let Err(code) = self.control.check(deadline) {
                return self.control.snapshot(code);
            }
            let limit = match self.control.observation_deadline(deadline) {
                Ok(limit) => limit,
                Err(code) => return self.control.snapshot(code),
            };
            let slice =
                Duration::from_millis(1).min(limit.saturating_duration_since(Instant::now()));
            let observed = self.result.recv_timeout(slice);
            if let Err(code) = self.control.check(deadline) {
                return self.control.snapshot(code);
            }
            match observed {
                Ok(result) => return result,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return self.control.snapshot(self.control.refuse(Code::Native));
                }
            }
        }
    }

    fn wait_cleanup_until(&self, deadline: Instant) -> CaseResult {
        self.control.admitted.store(false, Ordering::Release);
        let limit = deadline.min(self.control.deadline);
        loop {
            if Instant::now() >= limit {
                return self.control.snapshot(Code::Expired);
            }
            let slice =
                Duration::from_millis(1).min(limit.saturating_duration_since(Instant::now()));
            let observed = self.result.recv_timeout(slice);
            if Instant::now() >= limit {
                return self.control.snapshot(Code::Expired);
            }
            match observed {
                Ok(mut result) => {
                    if result.code == Code::Success {
                        result.code = Code::Expired;
                    }
                    return result;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return self.control.snapshot(Code::Cleanup);
                }
            }
        }
    }

    fn finish_if_returned(mut self) {
        // No polling/joining an unfinished owner after the 200 ms refusal.
        if self
            .thread
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
            && let Some(thread) = self.thread.take()
        {
            let _ = thread.join();
        }
        // Otherwise detach: the same worker still owns its exact native context.
    }
}

impl Drop for CaseAdmission {
    fn drop(&mut self) {
        self.control.admitted.store(false, Ordering::Release);
    }
}

struct Owner {
    control: Arc<Control>,
    state: Option<PrivateState>,
    children: [Option<Child>; 2],
    child_roles: [ChildRole; 2],
    active_cli: Option<Child>,
    guard: Option<DirectoryGuard>,
    runtime: Option<Runtime>,
    client: Option<NamedPipeClient>,
    metadata: Option<MetadataPipe>,
    query_handle: Option<OwnedHandle>,
    captures: Vec<GuardedFile>,
    capture_reader: Option<GuardedFile>,
    capture_duplicate: Option<File>,
    capture_count: u16,
    completed_cli: bool,
    reaped: bool,
    uncertain_cleanup: bool,
    call_context: CallContext,
    paired: PairedCapture,
    #[cfg(debug_assertions)]
    producers: Producers,
    #[cfg(debug_assertions)]
    initial_run: Option<PreparedInitialRunCommand>,
}

impl Owner {
    fn empty(control: Arc<Control>) -> Self {
        Self {
            control,
            state: None,
            // Reserved before any actual spawn; anchoring cannot allocate.
            children: [None, None],
            child_roles: [ChildRole::NoChild; 2],
            active_cli: None,
            guard: None,
            runtime: None,
            client: None,
            metadata: None,
            query_handle: None,
            captures: Vec::with_capacity(usize::from(MAX_CLI_CAPTURES)),
            capture_reader: None,
            capture_duplicate: None,
            capture_count: 0,
            completed_cli: false,
            reaped: false,
            uncertain_cleanup: false,
            call_context: CallContext::new(),
            paired: PairedCapture::empty(),
            #[cfg(debug_assertions)]
            producers: Producers::empty(),
            #[cfg(debug_assertions)]
            initial_run: None,
        }
    }

    fn root(&self) -> Result<&Path, Code> {
        self.state
            .as_ref()
            .map(PrivateState::path)
            .ok_or(Code::Cleanup)
    }

    fn initialize_state(&mut self) -> Result<(), Code> {
        self.control.step(Phase::Setup, self.control.deadline)?;
        // The unchanged factory is invoked only on this already-admitted worker.
        self.control.observations.intent(Operation::StateSetup);
        self.state = Some(private_state_fixture());
        self.control.check(self.control.deadline)
    }

    fn spawn_child(
        &mut self,
        index: usize,
        command: &mut Command,
        post_spawn: Option<Duration>,
        role: ChildRole,
    ) -> Result<(), Code> {
        self.control.step(Phase::Setup, self.control.deadline)?;
        let slot = self.children.get_mut(index).ok_or(Code::Native)?;
        if slot.is_some() {
            return Err(Code::Native);
        }
        let operation = match role {
            ChildRole::NegativePeer => Operation::PeerSpawn,
            _ => Operation::DaemonSpawn,
        };
        self.control.observations.intent(operation);
        let spawned = command.spawn();
        let returned = Instant::now();
        match spawned {
            Ok(child) => {
                *slot = Some(child);
                if let Some(anchored_role) = self.child_roles.get_mut(index) {
                    *anchored_role = role;
                }
            }
            Err(error) => {
                self.control.observations.error(operation, role, &error);
                self.control.check(self.control.deadline)?;
                return Err(Code::Native);
            }
        }
        // Anchor the actual handle before publication/checks or any allocation.
        if role == ChildRole::Daemon {
            let word = match returned
                .checked_duration_since(self.control.entered)
                .and_then(|elapsed| u64::try_from(elapsed.as_micros()).ok())
            {
                Some(us) if us <= CALL_TIME_US => us + 1,
                Some(_) => DAEMON_OVERFLOW,
                None => DAEMON_INVALID,
            };
            self.control
                .calls
                .daemon_spawn
                .store(word, Ordering::Release);
        }
        if let Some(duration) = post_spawn {
            self.control
                .publish_case((returned + duration).min(self.control.deadline))?;
        }
        self.control.check(self.control.deadline)
    }

    fn daemon(&mut self, index: usize, post_spawn: Option<Duration>) -> Result<(), Code> {
        self.spawn_child(
            index,
            self.command()?
                .args(["daemon", "run"])
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
            post_spawn,
            ChildRole::Daemon,
        )
    }

    // BEGIN selected producer owners; native objects never enter a driver/channel.
    #[cfg(debug_assertions)]
    fn prepare_producers(&mut self, kind: ProducerKind) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        if self.producers.enabled || self.initial_run.is_some() {
            return Err(Code::Native);
        }
        self.producers.enabled = true;
        self.producers.positive = matches!(kind, ProducerKind::QueuedControl);
        self.producers.context = *uuid::Uuid::now_v7().as_bytes();
        let context = uuid::Uuid::from_bytes(self.producers.context)
            .simple()
            .to_string();
        let job = match kind {
            ProducerKind::Wake => "wake",
            ProducerKind::Cancel => "cancel",
            ProducerKind::QueuedControl => "producer-queued",
        };
        let mut command = self.command()?;
        command.args(["--json", "run", job]);
        self.initial_run = Some(PreparedInitialRunCommand {
            command,
            state: InitialRunState::Partial,
        });
        self.control.check(self.control.deadline)?;
        let paired = matches!(kind, ProducerKind::Cancel);
        if matches!(kind, ProducerKind::Wake) {
            let mut command = self.command()?;
            command.args(["daemon", "run"]).stdout(Stdio::null());
            self.producers.commands[0] = Some(command);
            self.control.check(self.control.deadline)?;
        }
        for index in 0..2 {
            if index == 0 && matches!(kind, ProducerKind::QueuedControl) {
                continue;
            }
            self.control.check(self.control.deadline)?;
            if !(index == 0 && paired) {
                let path = self.root()?.join(if index == 0 {
                    "wake-daemon-stderr"
                } else {
                    "wake-run-stderr"
                });
                let opened = create_private_new(&path);
                match opened {
                    Ok(writer) => self.producers.writers[index] = Some(writer),
                    Err(error) => {
                        self.control.check(self.control.deadline)?;
                        return Err(if error.kind() == io::ErrorKind::AlreadyExists {
                            Code::CaptureCollision
                        } else {
                            Code::Cli
                        });
                    }
                }
                self.control.check(self.control.deadline)?;
            }
            let writer = if index == 0 && paired {
                self.paired.writers[1].as_ref()
            } else {
                self.producers.writers[index].as_ref()
            }
            .ok_or(Code::Native)?;
            let path = writer.normalized_path().to_path_buf();
            let identity = file_identity(writer);
            if let Ok(identity) = &identity {
                self.producers.snapshot.identities[index] = Some(*identity);
            }
            self.control.check(self.control.deadline)?;
            identity.map_err(|_| Code::Cli)?;
            if !(index == 0 && paired) {
                let duplicate = self.producers.writers[index]
                    .as_ref()
                    .ok_or(Code::Native)?
                    .try_clone();
                if let Ok(duplicate) = duplicate {
                    let command = if index == 0 {
                        self.producers.commands[0].as_mut()
                    } else {
                        self.initial_run
                            .as_mut()
                            .map(|prepared| &mut prepared.command)
                    }
                    .ok_or(Code::Native)?;
                    command.stderr(Stdio::from(duplicate));
                } else {
                    self.control.check(self.control.deadline)?;
                    return Err(Code::Cli);
                }
                self.control.check(self.control.deadline)?;
            }
            // This is an independent cursor, prepared before any selected root.
            let opened = locron_core::filesystem::open_private(
                &path,
                std::fs::OpenOptions::new().read(true),
            );
            if let Ok(reader) = opened {
                self.producers.readers[index] = Some(reader);
            } else {
                self.control.check(self.control.deadline)?;
                return Err(Code::Cli);
            }
            self.control.check(self.control.deadline)?;
            let reader_id =
                file_identity(self.producers.readers[index].as_ref().ok_or(Code::Native)?);
            self.control.check(self.control.deadline)?;
            if Some(reader_id.map_err(|_| Code::Cli)?) != self.producers.snapshot.identities[index]
            {
                return Err(Code::Cli);
            }
            let cursor = self.producers.readers[index]
                .as_mut()
                .ok_or(Code::Native)?
                .stream_position();
            self.control.check(self.control.deadline)?;
            if cursor.map_err(|_| Code::Cli)? != 0 {
                return Err(Code::Cli);
            }
            let command = if index == 1 {
                self.initial_run
                    .as_mut()
                    .map(|prepared| &mut prepared.command)
            } else if paired {
                self.paired.command.as_mut()
            } else {
                self.producers.commands[0].as_mut()
            }
            .ok_or(Code::Native)?;
            command
                .env("LOCRON_WINDOWS_DIAGNOSTIC_VERSION", "2")
                .env(
                    "LOCRON_WINDOWS_DIAGNOSTIC_ROLE",
                    if index == 0 { "daemon" } else { "run" },
                )
                .env("LOCRON_WINDOWS_DIAGNOSTIC_CONTEXT", &context);
        }
        if self.producers.snapshot.identities[0].is_some()
            && self.producers.snapshot.identities[0] == self.producers.snapshot.identities[1]
        {
            return Err(Code::Cli);
        }
        self.control.check(self.control.deadline)?;
        self.initial_run.as_mut().ok_or(Code::Native)?.state = InitialRunState::Prepared;
        Ok(())
    }

    #[cfg(debug_assertions)]
    fn wake_daemon(&mut self) -> Result<(), Code> {
        self.prepare_producers(ProducerKind::Wake)?;
        // The existing paired-spawn ownership pattern is used only for this daemon.
        // Restore its stack-owned Command before resuming an unwind to outer Owner.
        let mut command = self.producers.commands[0].take().ok_or(Code::Native)?;
        let spawned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.spawn_child(
                0,
                &mut command,
                Some(Duration::from_secs(5)),
                ChildRole::Daemon,
            )
        }));
        self.producers.commands[0] = Some(command);
        match spawned {
            Ok(result) => result,
            Err(payload) => std::panic::resume_unwind(payload),
        }
    }

    #[cfg(debug_assertions)]
    fn initial_run_output(&mut self) -> Result<Output, Code> {
        // A Phase/env/string cannot elect this path. Pure fixed-slot validation
        // happens before stdout creation, native spawn or any fallback.
        self.initial_run.as_ref().ok_or(Code::Native)?.unused()?;
        self.call_context.begin(&self.control, Phase::Run);
        let result =
            self.captured_output_body(Phase::Run, CaptureSource::InitialRunOwner, false, None);
        self.call_context.returned(&self.control, &result);
        result
    }

    #[cfg(debug_assertions)]
    fn initial_submit(&mut self) -> Result<String, Code> {
        let output = self.initial_run_output()?;
        self.producers.snapshot.run_status = output.status.code();
        self.control.observations.intent(Operation::SubmitJson);
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Code::Run)?;
        self.control.check(self.control.deadline)?;
        self.control.observations.intent(Operation::SubmitSelect);
        self.producers.snapshot.run_queued = envelope["data"]["state"] == "queued";
        let result = envelope["data"]["run_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or(Code::Run);
        if let Ok(run_id) = &result {
            self.producers.uuid = uuid::Uuid::parse_str(run_id)
                .ok()
                .filter(|uuid| !uuid.is_nil() && uuid.hyphenated().to_string() == *run_id)
                .map(uuid::Uuid::into_bytes);
        }
        result
    }

    #[cfg(debug_assertions)]
    fn classify_cancel(&mut self, bytes: &[u8], run_id: &str) {
        self.producers.snapshot.cancel = if bytes
            == format!("cancellation requested: {run_id}\n").as_bytes()
        {
            CancelReply::Requested
        } else if bytes
            == format!("cancellation requested: {run_id} (cancelled before execution)\n").as_bytes()
        {
            CancelReply::BeforeExecution
        } else {
            CancelReply::Unobserved
        };
    }

    #[cfg(debug_assertions)]
    fn collect_producers(&mut self, permit: &ProducerPermit) -> Result<(), Code> {
        if !self.reaped {
            return Err(Code::Cleanup);
        }
        for index in 0..2 {
            if self.producers.readers[index].is_none() {
                continue;
            }
            permit.check(&self.control)?;
            let identity =
                file_identity(self.producers.readers[index].as_ref().ok_or(Code::Native)?);
            permit.check(&self.control)?;
            if identity.ok() != self.producers.snapshot.identities[index] {
                self.producers.snapshot.facts[index].capture = ProducerCapture::BindingRefused;
                continue;
            }
            let cursor = self.producers.readers[index]
                .as_mut()
                .ok_or(Code::Native)?
                .stream_position();
            permit.check(&self.control)?;
            if cursor.ok() != Some(0) {
                self.producers.snapshot.facts[index].capture = ProducerCapture::BindingRefused;
                continue;
            }
            self.producers.snapshot.zero_cursors[index] = true;
            let mut bytes = Vec::with_capacity(65_537);
            let mut buffer = [0; 4096];
            let state = loop {
                permit.check(&self.control)?;
                let count = buffer.len().min(65_537 - bytes.len());
                let read = self.producers.readers[index]
                    .as_mut()
                    .ok_or(Code::Native)?
                    .read(&mut buffer[..count]);
                if let Ok(count) = &read {
                    bytes.extend_from_slice(&buffer[..*count]);
                }
                permit.check(&self.control)?;
                match read {
                    Err(_) => break ProducerCapture::ReadError,
                    Ok(_) if bytes.len() == 65_537 => break ProducerCapture::Oversized,
                    Ok(0) => break ProducerCapture::Complete,
                    Ok(_) => {}
                }
            };
            self.producers.snapshot.facts[index] = if state == ProducerCapture::Complete {
                ProducerFact::from_bytes(
                    &bytes,
                    self.producers.context,
                    index == 1,
                    self.producers.uuid,
                )
            } else {
                ProducerFact {
                    capture: state,
                    bytes: u32::try_from(bytes.len()).ok(),
                    ..ProducerFact::UNOBSERVED
                }
            };
            permit.check(&self.control)?;
        }
        permit.check(&self.control)
    }

    #[cfg(debug_assertions)]
    fn release_producers(&mut self) -> Result<(), Code> {
        cleanup_gate(&self.control)?;
        drop(self.initial_run.take());
        cleanup_gate(&self.control)?;
        for index in 0..2 {
            drop(self.producers.readers[index].take());
            cleanup_gate(&self.control)?;
            drop(self.producers.commands[index].take());
            cleanup_gate(&self.control)?;
            drop(self.producers.writers[index].take());
            cleanup_gate(&self.control)?;
        }
        Ok(())
    }
    // END selected producer owners.

    // BEGIN additive paired methods; no CLI CallContext observation here.
    fn pair_path(&self, index: usize) -> Result<PathBuf, Code> {
        let name = ["daemon-stdout", "daemon-stderr"]
            .get(index)
            .ok_or(Code::Native)?;
        Ok(self.root()?.join(name))
    }

    fn prepare_pair(&mut self, command: Command) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        if self.paired.command.is_some() || self.paired.writers.iter().any(Option::is_some) {
            return Err(Code::Native);
        }
        // The two actual Stdio duplicates are retained in this exact Command.
        self.paired.command = Some(command);
        for index in 0..2 {
            self.control.check(self.control.deadline)?;
            let path = self.pair_path(index)?;
            let opened = create_private_new(&path);
            match opened {
                Ok(writer) => self.paired.writers[index] = Some(writer),
                Err(error) => {
                    self.control.check(self.control.deadline)?;
                    return Err(if error.kind() == io::ErrorKind::AlreadyExists {
                        Code::CaptureCollision
                    } else {
                        Code::Cli
                    });
                }
            }
            self.control.check(self.control.deadline)?;
            let identity = file_identity(self.paired.writers[index].as_ref().ok_or(Code::Cli)?);
            if let Ok(identity) = &identity {
                self.paired.proof.identities[index] = Some(*identity);
            }
            self.control.check(self.control.deadline)?;
            identity.map_err(|_| Code::Cli)?;
            self.control.check(self.control.deadline)?;
            let cloned = self.paired.writers[index]
                .as_ref()
                .ok_or(Code::Cli)?
                .try_clone();
            if let Ok(duplicate) = cloned {
                let command = self.paired.command.as_mut().ok_or(Code::Cli)?;
                if index == 0 {
                    command.stdout(Stdio::from(duplicate));
                } else {
                    command.stderr(Stdio::from(duplicate));
                }
                self.paired.attached |= 1 << index;
            } else {
                self.control.check(self.control.deadline)?;
                return Err(Code::Cli);
            }
            self.control.check(self.control.deadline)?;
        }
        if self.paired.proof.identities[0] == self.paired.proof.identities[1] {
            return Err(Code::Cli);
        }
        Ok(())
    }

    fn spawn_pair(&mut self, post_spawn: Option<Duration>) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        if self.paired.attached != 3 {
            return Err(Code::Cli);
        }
        // Taking the Command avoids aliasing self during the unchanged primitive.
        // A blocked call retains it on this same worker's native stack.
        let mut command = self.paired.command.take().ok_or(Code::Cli)?;
        let spawned = self.spawn_child(0, &mut command, post_spawn, ChildRole::Daemon);
        self.paired.command = Some(command);
        spawned
    }

    fn cancellation_daemon(&mut self) -> Result<(), Code> {
        let mut command = self.command()?;
        command.args(["daemon", "run"]);
        self.prepare_pair(command)?;
        #[cfg(debug_assertions)]
        self.prepare_producers(ProducerKind::Cancel)?;
        self.spawn_pair(Some(Duration::from_secs(8)))
    }

    fn pair_fact(&mut self, index: usize, fact: PairFact) -> Result<(), Code> {
        self.control.check(self.control.deadline)?;
        self.control
            .paired_facts
            .get(index)
            .ok_or(Code::Cli)?
            .set(fact)
            .map_err(|_| Code::Cli)
    }

    fn pair_read(&mut self, index: usize, sentinel: bool) -> Result<(PairFact, Vec<u8>), Code> {
        self.control.check(self.control.deadline)?;
        if !(self.paired.root_exited || sentinel && self.paired.proof.collision.no_child)
            || self.paired.reader.is_some()
        {
            return Err(Code::Cli);
        }
        let original = if sentinel {
            self.paired.sentinel.as_ref()
        } else {
            self.paired.writers.get(index).and_then(Option::as_ref)
        }
        .ok_or(Code::Cli)?;
        let path = original.normalized_path().to_path_buf();
        let original_id = file_identity(original);
        self.control.check(self.control.deadline)?;
        let original_id = original_id.map_err(|_| Code::Cli)?;
        self.control.check(self.control.deadline)?;
        let opened = open_read_no_follow(&path);
        match opened {
            Ok(reader) => self.paired.reader = Some(reader),
            Err(error) => {
                self.control.check(self.control.deadline)?;
                return Ok((
                    PairFact {
                        state: if error.kind() == io::ErrorKind::NotFound {
                            PairReadState::Missing
                        } else {
                            PairReadState::IoRefused
                        },
                        bytes: None,
                        record: PairRecognition::Unobserved,
                    },
                    Vec::new(),
                ));
            }
        }
        self.control.check(self.control.deadline)?;
        let reader_id = file_identity(self.paired.reader.as_ref().ok_or(Code::Cli)?);
        self.control.check(self.control.deadline)?;
        let reader_id = reader_id.map_err(|_| Code::Cli)?;
        if reader_id != original_id {
            return Ok((
                PairFact {
                    state: PairReadState::IdentityRefused,
                    bytes: None,
                    record: PairRecognition::Unobserved,
                },
                Vec::new(),
            ));
        }
        self.control.check(self.control.deadline)?;
        let cursor = self
            .paired
            .reader
            .as_mut()
            .ok_or(Code::Cli)?
            .stream_position();
        self.control.check(self.control.deadline)?;
        if cursor.map_err(|_| Code::Cli)? != 0 {
            return Ok((
                PairFact {
                    state: PairReadState::IdentityRefused,
                    bytes: None,
                    record: PairRecognition::Unobserved,
                },
                Vec::new(),
            ));
        }
        if !sentinel {
            self.paired.proof.zero_cursors[index] = true;
        }
        let limit = usize::try_from(READ_LIMIT + 1).map_err(|_| Code::Cli)?;
        let mut bytes = Vec::with_capacity(limit);
        let mut buffer = [0_u8; 4096];
        let state = loop {
            self.control.check(self.control.deadline)?;
            let count = buffer.len().min(limit - bytes.len());
            let read = self
                .paired
                .reader
                .as_mut()
                .ok_or(Code::Cli)?
                .read(&mut buffer[..count]);
            if let Ok(count) = &read {
                bytes.extend_from_slice(&buffer[..*count]);
            }
            self.control.check(self.control.deadline)?;
            let Ok(count) = read else {
                break PairReadState::IoRefused;
            };
            if bytes.len() == limit {
                break PairReadState::Oversized;
            }
            if count == 0 {
                break PairReadState::Complete;
            }
        };
        let record = if state == PairReadState::Complete {
            recognize_store_record(&bytes)
        } else {
            PairRecognition::Unobserved
        };
        self.control.check(self.control.deadline)?;
        let fact = PairFact {
            state,
            bytes: Some(u32::try_from(bytes.len()).map_err(|_| Code::Cli)?),
            record,
        };
        self.control.check(self.control.deadline)?;
        drop(self.paired.reader.take());
        self.control.check(self.control.deadline)?;
        Ok((fact, bytes))
    }

    fn collect_pair(&mut self) -> Result<[Vec<u8>; 2], Code> {
        self.control.check(self.control.deadline)?;
        if self.paired.collected {
            return Err(Code::Cli);
        }
        let (stdout_fact, stdout) = self.pair_read(0, false)?;
        self.pair_fact(0, stdout_fact)?;
        let (stderr_fact, stderr) = self.pair_read(1, false)?;
        self.pair_fact(1, stderr_fact)?;
        self.paired.collected = true;
        self.control.check(self.control.deadline)?;
        Ok([stdout, stderr])
    }

    fn observe_pair_after_reap(&mut self) {
        if self.paired.command.is_none() {
            return;
        }
        if self.children[0].is_some() && self.reaped {
            self.paired.root_exited = true;
            self.paired.proof.duplicates_held =
                self.paired.attached == 3 && self.paired.command.is_some();
            if let Some((_, ChildRole::Daemon, payload)) =
                event_header(self.control.observations.snapshot().statuses[0], 2)
            {
                self.paired.proof.root_status = decode_signed(payload).ok().flatten();
            }
        }
        if self.paired.root_exited
            && !self.paired.collected
            && self.control.check(self.control.deadline).is_ok()
        {
            // Optional output never replaces work or uses emergency admission.
            let _ = self.collect_pair();
        }
    }

    fn release_pair(&mut self) -> Result<(), Code> {
        if !self.paired.has_owners() {
            return Ok(());
        }
        cleanup_gate(&self.control)?;
        drop(self.paired.reader.take());
        cleanup_gate(&self.control)?;
        drop(self.paired.command.take());
        cleanup_gate(&self.control)?;
        for writer in &mut self.paired.writers {
            cleanup_gate(&self.control)?;
            drop(writer.take());
            cleanup_gate(&self.control)?;
        }
        drop(self.paired.sentinel.take());
        cleanup_gate(&self.control)?;
        drop(self.paired.lock.take());
        cleanup_gate(&self.control)
    }
    // END additive paired methods.

    fn retain_guard(&mut self) -> Result<(), Code> {
        self.control.step(Phase::Guard, self.control.deadline)?;
        self.control.observations.intent(Operation::StateGuard);
        let guarded = DirectoryGuard::existing_private(self.root()?);
        let guard = match guarded {
            Ok(guard) => guard,
            Err(error) => {
                self.control
                    .observations
                    .error(Operation::StateGuard, ChildRole::NoChild, &error);
                self.control.check(self.control.deadline)?;
                return Err(Code::Native);
            }
        };
        self.guard = Some(guard);
        self.control.flag(GUARD);
        self.control.check(self.control.deadline)
    }

    fn live(&mut self, index: usize, deadline: Instant) -> Result<(), Code> {
        let child = self
            .children
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or(Code::Native)?;
        child_live(
            &self.control,
            child,
            deadline,
            self.child_roles
                .get(index)
                .copied()
                .unwrap_or(ChildRole::NoChild),
        )
    }

    fn expected_role(&mut self, index: usize) -> Result<(), Code> {
        let path = self.root()?.join("daemon.lock");
        loop {
            self.control.step(Phase::Role, self.control.deadline)?;
            self.live(index, self.control.deadline)?;
            self.control
                .observations
                .intent(Operation::RoleMetadataRead);
            let metadata = DaemonLock::read_role_metadata(&path);
            self.control.check(self.control.deadline)?;
            let metadata = metadata.map_err(|_| Code::Role)?;
            if let Some(metadata) = metadata {
                let child = self
                    .children
                    .get(index)
                    .and_then(Option::as_ref)
                    .ok_or(Code::Native)?;
                if metadata.metadata.pid != child.id() {
                    return Err(Code::Role);
                }
                self.control.check(self.control.deadline)?;
                self.control.observations.intent(Operation::RoleLockProbe);
                let held = DaemonLock::probe_existing(&path);
                self.control.check(self.control.deadline)?;
                let held = held.map_err(|_| Code::Role)?;
                if held != LockProbe::Held {
                    return Err(Code::Role);
                }
                self.control
                    .observations
                    .intent(Operation::RoleMetadataRepeat);
                let repeated = DaemonLock::read_role_metadata(&path);
                self.control.check(self.control.deadline)?;
                let repeated = repeated.map_err(|_| Code::Role)?;
                if repeated.as_ref() != Some(&metadata) {
                    return Err(Code::Role);
                }
                self.live(index, self.control.deadline)?;
                return Ok(());
            }
            pause(
                &self.control,
                self.control.deadline,
                Duration::from_millis(5),
            )?;
        }
    }

    fn command(&self) -> Result<Command, Code> {
        let mut command = Command::new(assert_cmd::cargo::cargo_bin!("locron"));
        command.arg("--state-dir").arg(self.root()?);
        Ok(command)
    }

    fn output(&mut self, phase: Phase, command: &mut Command) -> Result<Output, Code> {
        self.captured_output(phase, command, false, None)
    }

    fn capture_path(&self, number: u16) -> Result<PathBuf, Code> {
        if number == 0 || number > MAX_CLI_CAPTURES {
            return Err(Code::Cli);
        }
        Ok(self.root()?.join(format!("cli-stdout-{number:04}")))
    }

    fn new_capture(&mut self) -> Result<usize, Code> {
        self.control.check(self.control.deadline)?;
        if self.captures.len() >= usize::from(MAX_CLI_CAPTURES) {
            return Err(Code::Cli);
        }
        let number = self.capture_count.checked_add(1).ok_or(Code::Cli)?;
        let path = self.capture_path(number)?;
        self.capture_count = number;
        self.control.observations.intent(Operation::StdoutCreate);
        let step = self.call_context.enter(
            &self.control,
            Operation::StdoutCreate,
            ChildRole::ControlCli,
        );
        let opened = create_private_new(&path);
        match opened {
            Ok(file) => {
                let index = self.captures.len();
                self.captures.push(file);
                self.call_context
                    .io_return(&self.control, step, Operation::StdoutCreate, true);
                self.control.check(self.control.deadline)?;
                Ok(index)
            }
            Err(error) => {
                self.call_context
                    .io_return(&self.control, step, Operation::StdoutCreate, false);
                self.control.observations.error(
                    Operation::StdoutCreate,
                    ChildRole::ControlCli,
                    &error,
                );
                self.control.check(self.control.deadline)?;
                if error.kind() == io::ErrorKind::AlreadyExists {
                    Err(Code::CaptureCollision)
                } else {
                    Err(Code::Cli)
                }
            }
        }
    }

    fn collect_capture(&mut self, index: usize, operation: Operation) -> Result<Vec<u8>, Code> {
        self.control.check(self.control.deadline)?;
        self.control
            .observations
            .intent(Operation::StdoutReaderOpen);
        let capture = self.captures.get(index).ok_or(Code::Cli)?;
        let reader_step = self.call_context.enter(
            &self.control,
            Operation::StdoutReaderOpen,
            ChildRole::ControlCli,
        );
        let opened = open_read_no_follow(capture.normalized_path());
        match opened {
            Ok(reader) => {
                self.capture_reader = Some(reader);
                self.call_context.io_return(
                    &self.control,
                    reader_step,
                    Operation::StdoutReaderOpen,
                    true,
                );
            }
            Err(error) => {
                self.call_context.io_return(
                    &self.control,
                    reader_step,
                    Operation::StdoutReaderOpen,
                    false,
                );
                self.control.observations.error(
                    Operation::StdoutReaderOpen,
                    ChildRole::ControlCli,
                    &error,
                );
                self.control.check(self.control.deadline)?;
                return Err(Code::Cli);
            }
        }
        self.control.check(self.control.deadline)?;
        let original = native(&self.control, self.control.deadline, Code::Cli, || {
            let step = self.call_context.enter(
                &self.control,
                Operation::StdoutIdentity,
                ChildRole::ControlCli,
            );
            self.control.observations.intent(Operation::StdoutIdentity);
            let identity = file_identity(
                self.captures
                    .get(index)
                    .ok_or_else(|| io::Error::other("missing owned capture"))?,
            );
            self.control.observations.io(
                Operation::StdoutIdentity,
                ChildRole::ControlCli,
                &identity,
            );
            self.call_context.io_return(
                &self.control,
                step,
                Operation::StdoutIdentity,
                identity.is_ok(),
            );
            identity
        })?;
        let reader = native(&self.control, self.control.deadline, Code::Cli, || {
            let step = self.call_context.enter(
                &self.control,
                Operation::StdoutIdentity,
                ChildRole::ControlCli,
            );
            self.control.observations.intent(Operation::StdoutIdentity);
            let identity = file_identity(
                self.capture_reader
                    .as_ref()
                    .ok_or_else(|| io::Error::other("missing owned reader"))?,
            );
            self.control.observations.io(
                Operation::StdoutIdentity,
                ChildRole::ControlCli,
                &identity,
            );
            self.call_context.io_return(
                &self.control,
                step,
                Operation::StdoutIdentity,
                identity.is_ok(),
            );
            identity
        })?;
        if original != reader {
            return Err(Code::Cli);
        }
        // This reader has its own offset; never seek the shared stdout duplicate.
        let limit = usize::try_from(READ_LIMIT + 1).map_err(|_| Code::Native)?;
        let mut bytes = Vec::with_capacity(limit);
        let mut buffer = [0_u8; 4096];
        loop {
            self.control.check(self.control.deadline)?;
            self.control.observations.intent(operation);
            let count = buffer.len().min(limit - bytes.len());
            let reader = self.capture_reader.as_mut().ok_or(Code::Cli)?;
            let read_step =
                self.call_context
                    .enter(&self.control, operation, ChildRole::ControlCli);
            let read = (&mut **reader).read(&mut buffer[..count]);
            self.control
                .observations
                .io(operation, ChildRole::ControlCli, &read);
            self.call_context
                .io_return(&self.control, read_step, operation, read.is_ok());
            if let Ok(count) = &read {
                bytes.extend_from_slice(&buffer[..*count]);
                self.control.stdout_bytes.store(
                    u32::try_from(bytes.len()).map_err(|_| Code::Native)?,
                    Ordering::Release,
                );
                self.call_context
                    .read_return(&self.control, read_step, bytes.len());
            }
            self.control.check(self.control.deadline)?;
            let count = read.map_err(|_| Code::Cli)?;
            if bytes.len() == limit {
                return Err(Code::CaptureOversized);
            }
            if count == 0 {
                return Ok(bytes);
            }
        }
    }

    fn captured_output(
        &mut self,
        phase: Phase,
        command: &mut Command,
        hold_duplicate: bool,
        poll_span: Option<Duration>,
    ) -> Result<Output, Code> {
        self.call_context.begin(&self.control, phase);
        let result = self.captured_output_body(
            phase,
            CaptureSource::Default(command),
            hold_duplicate,
            poll_span,
        );
        self.call_context.returned(&self.control, &result);
        result
    }

    fn captured_output_body(
        &mut self,
        phase: Phase,
        source: CaptureSource<'_>,
        hold_duplicate: bool,
        poll_span: Option<Duration>,
    ) -> Result<Output, Code> {
        self.control.step(phase, self.control.deadline)?;
        if self.active_cli.is_some()
            || self.capture_reader.is_some()
            || (poll_span.is_some() && !matches!(phase, Phase::Capture))
        {
            return Err(Code::Native);
        }
        self.control
            .stdout_bytes
            .store(UNOBSERVED_BYTES, Ordering::Release);
        self.control.cli_live_seen.store(false, Ordering::Release);
        let index = self.new_capture()?;
        let duplicate = native(&self.control, self.control.deadline, Code::Cli, || {
            let step = self.call_context.enter(
                &self.control,
                Operation::StdoutClone,
                ChildRole::ControlCli,
            );
            self.control.observations.intent(Operation::StdoutClone);
            let cloned = self
                .captures
                .get(index)
                .ok_or_else(|| io::Error::other("missing owned capture"))?
                .try_clone();
            self.control
                .observations
                .io(Operation::StdoutClone, ChildRole::ControlCli, &cloned);
            self.call_context.io_return(
                &self.control,
                step,
                Operation::StdoutClone,
                cloned.is_ok(),
            );
            cloned
        })?;
        if hold_duplicate {
            if self.capture_duplicate.is_some() {
                return Err(Code::Native);
            }
            self.control.check(self.control.deadline)?;
            self.control.observations.intent(Operation::StdoutClone);
            let capture = self.captures.get(index).ok_or(Code::Cli)?;
            let clone_step = self.call_context.enter(
                &self.control,
                Operation::StdoutClone,
                ChildRole::ControlCli,
            );
            let cloned = capture.try_clone();
            match cloned {
                Ok(cloned) => {
                    self.capture_duplicate = Some(cloned);
                    self.call_context.io_return(
                        &self.control,
                        clone_step,
                        Operation::StdoutClone,
                        true,
                    );
                }
                Err(error) => {
                    self.call_context.io_return(
                        &self.control,
                        clone_step,
                        Operation::StdoutClone,
                        false,
                    );
                    self.control.observations.error(
                        Operation::StdoutClone,
                        ChildRole::ControlCli,
                        &error,
                    );
                    self.control.check(self.control.deadline)?;
                    return Err(Code::Cli);
                }
            }
            self.control.check(self.control.deadline)?;
        }
        let (spawn, read, wait) = cli_operations(phase).ok_or(Code::Cli)?;
        self.control.check(self.control.deadline)?;
        self.control.observations.intent(spawn);
        let spawn_step = self
            .call_context
            .enter(&self.control, spawn, ChildRole::ControlCli);
        let command = match source {
            CaptureSource::Default(command) => {
                command.stdout(Stdio::from(duplicate)).stderr(Stdio::null())
            }
            #[cfg(debug_assertions)]
            CaptureSource::InitialRunOwner => self
                .initial_run
                .as_mut()
                .ok_or(Code::Native)?
                .dispatch()?
                .stdout(Stdio::from(duplicate)),
        };
        let child = command.spawn();
        let returned = Instant::now();
        let child = match child {
            Ok(child) => child,
            Err(error) => {
                self.call_context
                    .io_return(&self.control, spawn_step, spawn, false);
                self.control
                    .observations
                    .error(spawn, ChildRole::ControlCli, &error);
                self.control.check(self.control.deadline)?;
                return Err(Code::Cli);
            }
        };
        self.active_cli = Some(child);
        self.call_context
            .io_return(&self.control, spawn_step, spawn, true);
        // Only the independent live control supplies a new, shorter poll horizon.
        if let Some(span) = poll_span {
            self.control
                .publish_case((returned + span).min(self.control.deadline))?;
        }
        self.control.check(self.control.deadline)?;
        let status = loop {
            self.control.check(self.control.deadline)?;
            self.control.observations.intent(wait);
            let child = self.active_cli.as_mut().ok_or(Code::Cli)?;
            let wait_step = self
                .call_context
                .enter(&self.control, wait, ChildRole::ControlCli);
            let waited = child.try_wait();
            self.control
                .observations
                .io(wait, ChildRole::ControlCli, &waited);
            match &waited {
                Ok(Some(status)) => {
                    self.control
                        .observations
                        .status(wait, ChildRole::ControlCli, *status)
                }
                Ok(None) => self.control.cli_live_seen.store(true, Ordering::Release),
                Err(_) => {}
            }
            self.call_context
                .wait_return(&self.control, wait_step, wait, &waited);
            self.control.check(self.control.deadline)?;
            if let Some(status) = waited.map_err(|_| Code::Cli)? {
                break status;
            }
            pause(
                &self.control,
                self.control.deadline,
                Duration::from_millis(5),
            )?;
        };
        let bytes = self.collect_capture(index, read)?;
        self.control.check(self.control.deadline)?;
        // All original capture guards survive this actual CLI's confirmed reap.
        self.control.observations.intent(Operation::DropStdout);
        drop(self.capture_reader.take());
        self.completed_cli = true;
        drop(self.active_cli.take());
        self.control.check(self.control.deadline)?;
        if !status.success() {
            return Err(Code::Cli);
        }
        Ok(Output {
            status,
            stdout: bytes,
            stderr: Vec::new(),
        })
    }

    fn history(&mut self, name: &str, run_id: &str) -> Result<String, Code> {
        self.call_context.begin_history();
        let result = self.history_body(name, run_id);
        let returned = Instant::now();
        self.call_context
            .history_return(&self.control, &result, returned);
        result
    }

    fn history_body(&mut self, name: &str, run_id: &str) -> Result<String, Code> {
        self.live(0, self.control.deadline)?;
        let output = self.output(
            Phase::History,
            self.command()?.args(["--json", "history", name]),
        )?;
        self.control.observations.intent(Operation::HistoryJson);
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Code::Run)?;
        self.control.check(self.control.deadline)?;
        self.control.observations.intent(Operation::HistorySelect);
        envelope["data"]
            .as_array()
            .and_then(|runs| runs.iter().find(|run| run["id"] == run_id))
            .and_then(|run| run["state"].as_str())
            .map(str::to_owned)
            .ok_or(Code::Run)
    }

    #[cfg(not(debug_assertions))]
    fn submit(&mut self, name: &str) -> Result<String, Code> {
        let output = self.output(Phase::Run, self.command()?.args(["--json", "run", name]))?;
        self.control.observations.intent(Operation::SubmitJson);
        let envelope: serde_json::Value =
            serde_json::from_slice(&output.stdout).map_err(|_| Code::Run)?;
        self.control.check(self.control.deadline)?;
        self.control.observations.intent(Operation::SubmitSelect);
        envelope["data"]["run_id"]
            .as_str()
            .map(str::to_owned)
            .ok_or(Code::Run)
    }

    fn probe(&mut self, index: usize, hook: Option<ReturnGate>) -> Result<(), Code> {
        // This entry is before runtime creation, open, duplication, native query or I/O.
        let entry = Instant::now();
        let deadline = self
            .control
            .effective_deadline()?
            .min(entry + Duration::from_millis(200));
        self.control.publish_probe(deadline)?;
        self.control.step(Phase::Connect, deadline)?;
        self.live(index, deadline)?;
        let guard = self.guard.as_ref().ok_or(Code::Native)?;
        let endpoint = native(&self.control, deadline, Code::Native, || {
            self.control.observations.intent(Operation::EndpointName);
            let result = endpoint_name_guarded(guard, "wake", None);
            self.control
                .observations
                .io(Operation::EndpointName, ChildRole::NoChild, &result);
            result
        })?;
        self.control.check(deadline)?;
        self.control.observations.intent(Operation::RuntimeBuild);
        let built = Builder::new_current_thread().enable_all().build();
        self.runtime = match built {
            Ok(runtime) => Some(runtime),
            Err(error) => {
                self.control.observations.error(
                    Operation::RuntimeBuild,
                    ChildRole::NoChild,
                    &error,
                );
                self.control.check(deadline)?;
                return Err(Code::Native);
            }
        };
        self.control.check(deadline)?;
        let gate_root = self.root()?.to_path_buf();
        let runtime = self.runtime.as_ref().ok_or(Code::Native)?;
        let control = &self.control;
        let role = self
            .child_roles
            .get(index)
            .copied()
            .unwrap_or(ChildRole::NoChild);
        let child = self
            .children
            .get_mut(index)
            .and_then(Option::as_mut)
            .ok_or(Code::Native)?;
        let client = &mut self.client;
        let metadata = &mut self.metadata;
        let query_handle = &mut self.query_handle;
        let result = runtime.block_on(async {
            loop {
                control.check(deadline)?;
                control.observations.intent(Operation::PipeOpen);
                let opened = ClientOptions::new()
                    .read(true)
                    .write(true)
                    .pipe_mode(PipeMode::Byte)
                    .security_qos_flags(0x0001_0000)
                    .open(&endpoint);
                match opened {
                    Ok(opened) => {
                        *client = Some(opened);
                        control.flag(CLIENT);
                        control.check(deadline)?;
                        break;
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::NotFound
                            || error.raw_os_error() == Some(231) =>
                    {
                        control.observations.error(Operation::PipeOpen, role, &error);
                        control.check(deadline)?;
                        gated(control, deadline, async {
                            tokio::time::sleep(Duration::from_millis(5)).await;
                            Ok(())
                        })
                        .await?;
                    }
                    Err(error) => {
                        control.observations.error(Operation::PipeOpen, role, &error);
                        control.check(deadline)?;
                        return Err(Code::Native);
                    }
                }
            }
            child_live(control, child, deadline, role)?;
            control.step(Phase::Query, deadline)?;
            let original = client.as_mut().ok_or(Code::Native)?;
            control.check(deadline)?;
            control.observations.intent(Operation::PipeClone);
            let duplicated = original.as_handle().try_clone_to_owned();
            match duplicated {
                Ok(handle) => *query_handle = Some(handle),
                Err(error) => {
                    control.observations.error(Operation::PipeClone, role, &error);
                    control.check(deadline)?;
                    return Err(Code::Native);
                }
            }
            control.check(deadline)?;
            let handle = query_handle.take().ok_or(Code::Native)?;
            control.observations.intent(Operation::PipeConvert);
            *metadata = match PipeStream::<pipe_mode::Bytes, pipe_mode::Bytes>::try_from(handle) {
                Ok(wrapper) => Some(MetadataPipe(Some(wrapper))),
                Err(error) => {
                    // Plain returned handle only; never enter interprocess default limbo.
                    *query_handle = error.source;
                    control.check(deadline)?;
                    return Err(Code::Native);
                }
            };
            control.check(deadline)?;
            let pipe = metadata.as_ref().and_then(|wrapper| wrapper.0.as_ref()).ok_or(Code::Native)?;
            control.observations.intent(Operation::PipeDirection);
            let is_client = pipe.is_client();
            control.check(deadline)?;
            control.observations.intent(Operation::PipePid);
            let actual_peer = pipe.server_process_id();
            control.observations.io(Operation::PipePid, role, &actual_peer);
            control.flag(QUERIED);
            control.check(deadline)?;
            child_live(control, child, deadline, role)?;
            if let Some(hook) = hook {
                if !is_client
                    || !matches!(actual_peer.as_ref(), Ok(peer) if *peer != 0 && *peer == child.id())
                {
                    return Err(Code::Native);
                }
                // The actual native query has returned. Deliberately withhold its
                // result/return; the real original client/root/child remain owned.
                let notice = HeldNotice {
                    deadline,
                    root: gate_root,
                };
                let sent = hook.entered.try_send(notice);
                control.check(deadline)?;
                sent.map_err(|_| Code::Native)?;
                let released = hook.release
                    .recv_timeout(control.deadline.saturating_duration_since(Instant::now()));
                control.check(deadline)?;
                released.map_err(|_| Code::Expired)?;
            }
            // Metadata-only duplicate stays in the owner through child reaping.
            // Its every-outcome Drop consumes it with evade_limbo, never I/O.
            control.check(deadline)?;
            child_live(control, child, deadline, role)?;
            if !is_client {
                return Err(Code::WrongDirection);
            }
            let actual_peer = actual_peer.map_err(|_| Code::Native)?;
            if actual_peer == 0 || actual_peer != child.id() {
                return Err(Code::ForeignPeer);
            }
            control.flag(MATCHED);
            let mut frame = [0_u8; 16];
            frame[0] = 15;
            frame[1..].copy_from_slice(WAKE_MESSAGE);
            control.step(Phase::Frame, deadline)?;
            control.frame_bytes.store(16, Ordering::Release);
            gated(control, deadline, observed_io(control, Operation::PipeFrameWrite, role, original.write_all(&frame))).await?;
            child_live(control, child, deadline, role)?;
            control.step(Phase::Ack, deadline)?;
            let mut acknowledgement = [0_u8; 14];
            gated(control, deadline, observed_io(control, Operation::PipeAckRead, role, original.read_exact(&mut acknowledgement))).await?;
            child_live(control, child, deadline, role)?;
            if acknowledgement != ACK_MESSAGE {
                return Err(Code::BadAck);
            }
            control.flag(ACK);
            control.step(Phase::Receipt, deadline)?;
            control.frame_bytes.store(31, Ordering::Release);
            gated(control, deadline, observed_io(control, Operation::PipeReceiptWrite, role, original.write_all(&[0xff]))).await?;
            child_live(control, child, deadline, role)?;
            control.check(deadline)
        });
        // Seal completed successes AND on-time refusals; it witnesses completion,
        // never peer trust. Incomplete/late work leaves the expiry intact.
        self.control.check(deadline)?;
        self.control.seal_probe(Instant::now())?;
        result
    }

    fn reap(&mut self) -> Result<(), Code> {
        let mut actual_child = self.completed_cli;
        for (index, child) in self
            .children
            .iter_mut()
            .chain(std::iter::once(&mut self.active_cli))
            .enumerate()
            .filter_map(|(index, child)| child.as_mut().map(|child| (index, child)))
        {
            actual_child = true;
            let role = self
                .child_roles
                .get(index)
                .copied()
                .unwrap_or(ChildRole::ControlCli);
            // Normal checks include native cleanup in the original clock. After
            // expiry, only emergency cleanup of these exact handles is admitted.
            let _ = self.control.check(self.control.deadline);
            cleanup_gate(&self.control)?;
            self.control.observations.intent(Operation::CleanupTryWait);
            let exited = child.try_wait();
            self.control
                .observations
                .io(Operation::CleanupTryWait, role, &exited);
            if let Ok(Some(status)) = &exited {
                self.control
                    .observations
                    .status(Operation::CleanupTryWait, role, *status);
            }
            cleanup_gate(&self.control)?;
            let exited = exited.map_err(|_| Code::Cleanup)?.is_some();
            if !exited {
                cleanup_gate(&self.control)?;
                self.control.observations.intent(Operation::CleanupKill);
                let killed = child.kill();
                self.control
                    .observations
                    .io(Operation::CleanupKill, role, &killed);
                let _ = killed;
                cleanup_gate(&self.control)?;
                self.control.observations.intent(Operation::CleanupWait);
                let waited = child.wait();
                self.control
                    .observations
                    .io(Operation::CleanupWait, role, &waited);
                if let Ok(status) = &waited {
                    self.control
                        .observations
                        .status(Operation::CleanupWait, role, *status);
                }
                cleanup_gate(&self.control)?;
                waited.map_err(|_| Code::Cleanup)?;
            }
        }
        self.reaped = true;
        if actual_child {
            self.control.flag(REAPED);
        }
        Ok(())
    }

    fn release(&mut self) -> Result<(), Code> {
        #[cfg(debug_assertions)]
        if self.producers.has_owners() || self.initial_run.is_some() {
            self.release_producers()?;
        }
        // BEGIN pair release before unchanged owner release.
        self.release_pair()?;
        // END pair release.
        let root = self.state.as_ref().map(|state| state.path().to_path_buf());
        // Reaping, not cancellation/Drop, authorizes removal of private state.
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropStdout);
        drop(self.capture_reader.take());
        cleanup_gate(&self.control)?;
        drop(self.capture_duplicate.take());
        cleanup_gate(&self.control)?;
        while !self.captures.is_empty() {
            cleanup_gate(&self.control)?;
            drop(self.captures.pop());
            cleanup_gate(&self.control)?;
        }
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropMetadata);
        drop(self.metadata.take());
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropQueryHandle);
        drop(self.query_handle.take());
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropClient);
        drop(self.client.take());
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropRuntime);
        drop(self.runtime.take());
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropGuard);
        drop(self.guard.take());
        cleanup_gate(&self.control)?;
        self.control.observations.intent(Operation::DropState);
        drop(self.state.take());
        // Cleanup of an already reaped exact owner may follow a refused probe.
        // It still has the helper/caller ORIGINAL clock, never a new duration.
        cleanup_gate(&self.control)?;
        if let Some(root) = root {
            self.control
                .observations
                .intent(Operation::CleanupStateExists);
            let remains = root.try_exists();
            self.control.observations.io(
                Operation::CleanupStateExists,
                ChildRole::NoChild,
                &remains,
            );
            cleanup_gate(&self.control)?;
            if remains.map_err(|_| Code::Cleanup)? {
                return Err(Code::Cleanup);
            }
            self.control.flag(CLEANED);
        }
        Ok(())
    }

    fn quarantine(&self) -> ! {
        self.control.admitted.store(false, Ordering::Release);
        loop {
            thread::park();
        }
    }

    fn complete(&mut self, work: Result<(), Code>) -> CaseResult {
        self.call_context.active.set(false);
        let retained = self
            .call_context
            .version_control
            .get()
            .then(|| self.control.calls.snapshot());
        let previous_phase = self.control.phase.load(Ordering::Acquire);
        self.control.observations.work(work, previous_phase);
        self.control
            .phase
            .store(Phase::Cleanup as u8, Ordering::Release);
        if self.reap().is_err() {
            self.uncertain_cleanup = true;
            #[cfg(debug_assertions)]
            if self.producers.enabled {
                self.producers.snapshot.facts = [ProducerFact {
                    capture: ProducerCapture::Unreaped,
                    ..ProducerFact::UNOBSERVED
                }; 2];
                let _ = self
                    .control
                    .producer_diagnostic
                    .set(self.producers.snapshot);
            }
            // No result/field destruction while exact root exit is unknown.
            self.quarantine();
        }
        // BEGIN normal-only optional pair observation after exact reap.
        self.observe_pair_after_reap();
        // END optional pair observation.
        #[cfg(debug_assertions)]
        if self.producers.enabled
            && (self.producers.positive
                || work.is_err()
                || !self.control.admitted.load(Ordering::Acquire))
        {
            let permit = ProducerPermit {
                deadline: self.control.deadline,
                lane: if self.producers.positive {
                    ProducerLane::PositiveControl
                } else {
                    ProducerLane::FailureDiagnostic
                },
            };
            if self.collect_producers(&permit).is_err() {
                self.producers.snapshot.facts = [ProducerFact {
                    capture: ProducerCapture::Late,
                    ..ProducerFact::UNOBSERVED
                }; 2];
                let _ = self
                    .control
                    .producer_diagnostic
                    .set(self.producers.snapshot);
                self.quarantine();
            }
        }
        let cleanup = self.release();
        #[cfg(debug_assertions)]
        if self.producers.enabled {
            self.producers.snapshot.released =
                cleanup.is_ok() && self.control.flags.load(Ordering::Acquire) & CLEANED != 0;
            let _ = self
                .control
                .producer_diagnostic
                .set(self.producers.snapshot);
        }
        let _ = self.control.paired_proof.set(self.paired.proof);
        let mut code = cleanup.and(work).err().unwrap_or(Code::Success);
        if code == Code::Success && self.control.check(self.control.deadline).is_err() {
            code = Code::Expired;
        }
        self.control.phase.store(previous_phase, Ordering::Release);
        if retained.is_some_and(|before| before != self.control.calls.snapshot()) {
            self.control
                .calls
                .capture_proof
                .fetch_and(!16, Ordering::Release);
        }
        self.control.snapshot(code)
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        // Includes unwinding. Never let automatic field/TempDir destruction run
        // while actual owned-child exit is unknown. No replay/replacement here.
        if !self.reaped && !self.uncertain_cleanup && self.reap().is_err() {
            self.uncertain_cleanup = true;
        }
        if self.uncertain_cleanup {
            self.quarantine();
        }
        // Includes empty/partial setup. An unfinished native context stays here,
        // never on an admission/error/channel/driver path.
        if (self.state.is_some()
            || self.guard.is_some()
            || self.runtime.is_some()
            || self.client.is_some()
            || self.metadata.is_some()
            || self.query_handle.is_some()
            || self.capture_reader.is_some()
            || self.capture_duplicate.is_some()
            || !self.captures.is_empty()
            || self.paired.has_owners()
            || {
                #[cfg(debug_assertions)]
                {
                    self.producers.has_owners() || self.initial_run.is_some()
                }
                #[cfg(not(debug_assertions))]
                {
                    false
                }
            })
            && self.release().is_err()
        {
            self.quarantine();
        }
    }
}

fn cleanup_gate(control: &Control) -> Result<(), Code> {
    // Closing admission does not authorize new work; only the same owner's
    // teardown may proceed inside the one original outer horizon.
    let _ = control.check(control.deadline);
    if Instant::now() >= control.deadline {
        Err(Code::Expired)
    } else {
        Ok(())
    }
}

fn cli_operations(phase: Phase) -> Option<(Operation, Operation, Operation)> {
    match phase {
        Phase::Add => Some((
            Operation::AddCliSpawn,
            Operation::AddCliRead,
            Operation::AddCliWait,
        )),
        Phase::Run => Some((
            Operation::RunCliSpawn,
            Operation::RunCliRead,
            Operation::RunCliWait,
        )),
        Phase::History => Some((
            Operation::HistoryCliSpawn,
            Operation::HistoryCliRead,
            Operation::HistoryCliWait,
        )),
        Phase::Cancel => Some((
            Operation::CancelCliSpawn,
            Operation::CancelCliRead,
            Operation::CancelCliWait,
        )),
        Phase::Capture => Some((
            Operation::CaptureCliSpawn,
            Operation::CaptureCliRead,
            Operation::CaptureCliWait,
        )),
        _ => None,
    }
}

struct MetadataPipe(Option<PipeStream<pipe_mode::Bytes, pipe_mode::Bytes>>);

impl Drop for MetadataPipe {
    fn drop(&mut self) {
        if let Some(wrapper) = self.0.take() {
            wrapper.evade_limbo();
        }
    }
}

fn native<T, E>(
    control: &Control,
    deadline: Instant,
    error: Code,
    operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, Code> {
    control.check(deadline)?;
    let result = operation();
    control.check(deadline)?;
    result.map_err(|_| error)
}

fn child_live(
    control: &Control,
    child: &mut Child,
    deadline: Instant,
    role: ChildRole,
) -> Result<(), Code> {
    control.check(deadline)?;
    let exited = native(control, deadline, Code::Native, || {
        control.observations.intent(Operation::ChildLiveness);
        let result = child.try_wait();
        control
            .observations
            .io(Operation::ChildLiveness, role, &result);
        if let Ok(Some(status)) = &result {
            control
                .observations
                .status(Operation::ChildLiveness, role, *status);
        }
        result
    })?
    .is_some();
    control.check(deadline)?;
    if exited {
        Err(Code::ChildExited)
    } else {
        Ok(())
    }
}

fn pause(control: &Control, deadline: Instant, duration: Duration) -> Result<(), Code> {
    control.check(deadline)?;
    thread::sleep(duration.min(deadline.saturating_duration_since(Instant::now())));
    control.check(deadline)
}

async fn gated<T>(
    control: &Control,
    deadline: Instant,
    future: impl Future<Output = io::Result<T>>,
) -> Result<T, Code> {
    let mut future = pin!(future);
    let guarded = poll_fn(|context| {
        if let Err(error) = control.check(deadline) {
            return Poll::Ready(Err(error));
        }
        let result = future.as_mut().poll(context);
        if let Err(error) = control.check(deadline) {
            return Poll::Ready(Err(error));
        }
        match result {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => Poll::Ready(result.map_err(|_| Code::Native)),
        }
    });
    let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), guarded)
        .await
        .map_err(|_| {
            control.admitted.store(false, Ordering::Release);
            Code::Expired
        })?;
    control.check(deadline)?;
    result
}

#[derive(Clone, Copy)]
enum CaptureCase {
    Version,
    Oversized,
    Live,
}

#[derive(Clone, Copy)]
enum PairedCase {
    Human,
    Json,
    OversizedStderr,
    LiveStderr,
    Collision,
}

#[derive(Clone, Copy)]
enum CaseKind {
    Wake,
    Cancel,
    Peer(PeerMode),
    Capture(CaptureCase),
    Paired(PairedCase),
    #[cfg(debug_assertions)]
    Producer(ProducerControl),
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
enum ProducerControl {
    Queued,
    Cancellation,
}

fn admit(hook: Option<ReturnGate>) -> Result<CaseAdmission, Box<CaseResult>> {
    // The only origin/outer horizon is born before even empty OS admission.
    let entered = Instant::now();
    let control = Control::new(entered, entered + Duration::from_secs(30));
    admit_control(control, hook)
}

fn admit_control(
    control: Arc<Control>,
    hook: Option<ReturnGate>,
) -> Result<CaseAdmission, Box<CaseResult>> {
    let (command, commands) = mpsc::sync_channel(1);
    let (sender, result) = mpsc::sync_channel(1);
    let worker_control = Arc::clone(&control);
    // Captures contain no fixture state, native handle, Child or owner callback.
    let admitted = thread::Builder::new()
        .name("windows-cli-case".to_owned())
        .spawn(move || {
            let mut owner = Owner::empty(worker_control);
            let mut hook = hook;
            let work = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                owner.control.check(owner.control.deadline)?;
                let received = commands.recv_timeout(
                    owner
                        .control
                        .deadline
                        .saturating_duration_since(Instant::now()),
                );
                owner.control.check(owner.control.deadline)?;
                let kind = received.map_err(|_| Code::Native)?;
                if matches!(kind, CaseKind::Peer(_)) {
                    owner.control.publish_case(owner.control.deadline)?;
                }
                owner.initialize_state()?;
                match kind {
                    CaseKind::Wake => {
                        #[cfg(not(debug_assertions))]
                        owner.daemon(0, Some(Duration::from_secs(5)))?;
                        #[cfg(debug_assertions)]
                        owner.wake_daemon()?;
                        wake_work(&mut owner)
                    }
                    CaseKind::Cancel => {
                        owner.cancellation_daemon()?;
                        cancel_work(&mut owner)
                    }
                    CaseKind::Peer(mode) => {
                        setup_peer(&mut owner, mode)?;
                        let index = if matches!(mode, PeerMode::Foreign) {
                            owner.daemon(1, None)?;
                            owner.expected_role(1)?;
                            1
                        } else {
                            0
                        };
                        owner.probe(index, hook.take())
                    }
                    CaseKind::Capture(case) => capture_work(&mut owner, case),
                    CaseKind::Paired(case) => paired_work(&mut owner, case),
                    #[cfg(debug_assertions)]
                    CaseKind::Producer(ProducerControl::Queued) => {
                        owner.prepare_producers(ProducerKind::QueuedControl)?;
                        producer_queued_work(&mut owner)
                    }
                    #[cfg(debug_assertions)]
                    CaseKind::Producer(ProducerControl::Cancellation) => {
                        owner.cancellation_daemon()?;
                        owner.producers.positive = true;
                        cancel_work(&mut owner)
                    }
                }
            }))
            .unwrap_or(Err(Code::Panicked));
            // Owner stays outside the caught setup/work closure on this one worker.
            let completed = owner.complete(work);
            let _ = sender.try_send(completed);
        });
    let Ok(thread) = admitted else {
        return Err(Box::new(control.snapshot(control.refuse(Code::Native))));
    };
    Ok(CaseAdmission {
        control,
        command: Some(command),
        result,
        thread: Some(thread),
    })
}

#[cfg(not(debug_assertions))]
fn drive(kind: CaseKind) -> CaseResult {
    let mut driver = match admit(None) {
        Ok(driver) => driver,
        Err(result) => return *result,
    };
    if let Err(code) = driver.dispatch(kind) {
        let result = driver.control.snapshot(code);
        driver.finish_if_returned();
        return result;
    }
    let mut result = driver.receive_until(driver.control.deadline);
    let limit = driver.control.observation_deadline(driver.control.deadline);
    driver.finish_if_returned();
    if limit.is_err() || limit.is_ok_and(|deadline| Instant::now() >= deadline) {
        result.code = Code::Expired;
    }
    result
}

/// The unchanged first work result and separately admitted closed diagnostics.
#[cfg(debug_assertions)]
pub struct ProducerCaseResult {
    first: CaseResult,
    diagnostic: ProducerSnapshot,
    cancel: bool,
}
#[cfg(debug_assertions)]
impl ProducerCaseResult {
    /// Only the original work/cleanup oracle can supply success.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        self.first.succeeded()
    }
    /// The original independent daemon-output summary stays literal.
    #[must_use]
    pub fn daemon_output(&self) -> impl fmt::Display {
        self.first.daemon_output()
    }
    /// Two closed lines; private context, UUID, digest and logger bytes are absent.
    #[must_use]
    pub fn producer_output(&self) -> impl fmt::Display {
        ProducerSummary {
            snapshot: self.diagnostic,
            cancel: self.cancel,
        }
    }
}
#[cfg(debug_assertions)]
impl fmt::Display for ProducerCaseResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.first, formatter)
    }
}

#[cfg(debug_assertions)]
fn wait_producer_until(control: &Control) -> ProducerSnapshot {
    // Same irreversible close and outer horizon as wait_cleanup_until. The
    // ordinary result channel/late CaseResult is deliberately not consulted.
    control.admitted.store(false, Ordering::Release);
    loop {
        if Instant::now() >= control.deadline {
            return ProducerSnapshot::UNOBSERVED;
        }
        if let Some(snapshot) = control.producer_diagnostic.get().copied() {
            if Instant::now() >= control.deadline {
                return ProducerSnapshot::UNOBSERVED;
            }
            return snapshot;
        }
        thread::sleep(
            Duration::from_millis(1)
                .min(control.deadline.saturating_duration_since(Instant::now())),
        );
    }
}

#[cfg(debug_assertions)]
fn drive_producer(kind: CaseKind, cancel: bool) -> ProducerCaseResult {
    let mut driver = match admit(None) {
        Ok(driver) => driver,
        Err(result) => {
            return ProducerCaseResult {
                first: *result,
                diagnostic: ProducerSnapshot::UNOBSERVED,
                cancel,
            };
        }
    };
    // Primitive slots only: no native owner, channel error or cleanup callback.
    let control = Arc::clone(&driver.control);
    if let Err(code) = driver.dispatch(kind) {
        let result = driver.control.snapshot(code);
        driver.finish_if_returned();
        return ProducerCaseResult {
            first: result,
            diagnostic: ProducerSnapshot::UNOBSERVED,
            cancel,
        };
    }
    let mut result = driver.receive_until(driver.control.deadline);
    let limit = driver.control.observation_deadline(driver.control.deadline);
    driver.finish_if_returned();
    if limit.is_err() || limit.is_ok_and(|deadline| Instant::now() >= deadline) {
        result.code = Code::Expired;
    }
    // Freeze before any diagnostic wait; never substitute a late normal result.
    let first = result;
    let diagnostic = if first.succeeded() {
        ProducerSnapshot::UNOBSERVED
    } else {
        wait_producer_until(&control)
    };
    ProducerCaseResult {
        first,
        diagnostic,
        cancel,
    }
}

/// Tests the original genuine Wake oracle, with failure-only diagnostics.
#[cfg(debug_assertions)]
pub fn run_wake_case() -> ProducerCaseResult {
    drive_producer(CaseKind::Wake, false)
}
/// Tests the original genuine Cancel oracle, with failure-only diagnostics.
#[cfg(debug_assertions)]
pub fn run_cancel_case() -> ProducerCaseResult {
    drive_producer(CaseKind::Cancel, true)
}

#[cfg(debug_assertions)]
fn producer_queued_work(owner: &mut Owner) -> Result<(), Code> {
    owner.output(
        Phase::Add,
        owner
            .command()?
            .args(["add", "producer-queued", "--every", "1h", "--"])
            .args(super::success_process_args()),
    )?;
    let _run_id = owner.initial_submit()?;
    if owner.producers.snapshot.run_status != Some(0) || !owner.producers.snapshot.run_queued {
        return Err(Code::Run);
    }
    Ok(())
}

/// Tests secured actual-daemon delivery and manual admission within original5s.
#[cfg(not(debug_assertions))]
pub fn run_wake_case() -> CaseResult {
    drive(CaseKind::Wake)
}

fn wake_work(owner: &mut Owner) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.expected_role(0)?;
    owner.probe(0, None)?;
    owner.output(
        Phase::Add,
        owner
            .command()?
            .args(["add", "wake", "--every", "1h", "--"])
            .args(super::success_process_args()),
    )?;
    #[cfg(not(debug_assertions))]
    let run_id = owner.submit("wake")?;
    #[cfg(debug_assertions)]
    let run_id = owner.initial_submit()?;
    owner.call_context.stage.set(1);
    loop {
        match owner.history("wake", &run_id)?.as_str() {
            "succeeded" => break Ok(()),
            "queued" | "running" => {}
            _ => break Err(Code::Run),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
}

/// Tests actual Engine native progress and durable cancellation within original8s.
#[cfg(not(debug_assertions))]
pub fn run_cancel_case() -> CaseResult {
    drive(CaseKind::Cancel)
}

fn cancel_work(owner: &mut Owner) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.live(0, owner.control.deadline)?;
    owner.probe(0, None)?;
    owner.control.check(owner.control.deadline)?;
    owner.control.observations.intent(Operation::CaseCurrentExe);
    let executable = std::env::current_exe();
    owner
        .control
        .observations
        .io(Operation::CaseCurrentExe, ChildRole::NoChild, &executable);
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    if !executable.is_absolute() || executable.to_str().is_none() {
        return Err(Code::Native);
    }
    let progress = owner.root()?.join("cancel-progress");
    let progress_env = format!(
        "WINDOWS_CLI_CANCEL_PROGRESS={}",
        progress.to_str().ok_or(Code::Native)?
    );
    owner.output(
        Phase::Add,
        owner
            .command()?
            .args(["add", "cancel", "--every", "1h", "--cwd"])
            .arg(owner.root()?)
            .args(["--env", "WINDOWS_CLI_CANCEL_ROLE=native-cancel-v1", "--env"])
            .arg(progress_env)
            .arg("--")
            .arg(executable)
            .args([
                "--exact",
                TARGET_SELECTOR,
                "--nocapture",
                "--test-threads=1",
            ]),
    )?;
    #[cfg(not(debug_assertions))]
    let run_id = owner.submit("cancel")?;
    #[cfg(debug_assertions)]
    let run_id = owner.initial_submit()?;
    owner.call_context.stage.set(2);
    loop {
        let state = owner.history("cancel", &run_id)?;
        let counters = read_progress(&owner.control, &progress, &owner.call_context)?;
        if state == "running" && counters.len() >= 2 {
            owner.live(0, owner.control.deadline)?;
            break;
        }
        if !matches!(state.as_str(), "queued" | "running") {
            return Err(Code::Run);
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
    owner.call_context.stage.set(3);
    #[cfg(not(debug_assertions))]
    owner.output(Phase::Cancel, owner.command()?.args(["cancel", &run_id]))?;
    #[cfg(debug_assertions)]
    {
        let output = owner.output(Phase::Cancel, owner.command()?.args(["cancel", &run_id]))?;
        owner.classify_cancel(&output.stdout, &run_id);
    }
    owner.call_context.stage.set(4);
    loop {
        match owner.history("cancel", &run_id)?.as_str() {
            "cancelled" => break,
            "queued" | "running" => {}
            _ => return Err(Code::Run),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(25),
        )?;
    }
    owner.call_context.stage.set(5);
    let stopped = read_progress(&owner.control, &progress, &owner.call_context)?;
    if stopped.len() < 2 {
        return Err(Code::Progress);
    }
    pause(
        &owner.control,
        owner.control.deadline,
        Duration::from_millis(25),
    )?;
    if read_progress(&owner.control, &progress, &owner.call_context)? != stopped {
        return Err(Code::Progress);
    }
    owner.live(0, owner.control.deadline)
}

fn read_progress(control: &Control, path: &Path, context: &CallContext) -> Result<Vec<u64>, Code> {
    let mut missing = false;
    let result = read_progress_body(control, path, &mut missing);
    let returned = Instant::now();
    context.progress_return(control, &result, missing, returned);
    result
}

fn read_progress_body(
    control: &Control,
    path: &Path,
    missing: &mut bool,
) -> Result<Vec<u64>, Code> {
    control.step(Phase::Progress, control.deadline)?;
    control.observations.intent(Operation::ProgressOpen);
    let opened = open_read_no_follow(path);
    control
        .observations
        .io(Operation::ProgressOpen, ChildRole::NoChild, &opened);
    control.check(control.deadline)?;
    let mut file = match opened {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            *missing = true;
            return Ok(Vec::new());
        }
        Err(_) => return Err(Code::Progress),
    };
    let mut bytes = Vec::new();
    control.check(control.deadline)?;
    control.observations.intent(Operation::ProgressRead);
    let read = (&mut *file).take(READ_LIMIT + 1).read_to_end(&mut bytes);
    control
        .observations
        .io(Operation::ProgressRead, ChildRole::NoChild, &read);
    control.check(control.deadline)?;
    read.map_err(|_| Code::Progress)?;
    control.observations.intent(Operation::ProgressValidate);
    if bytes.len() > READ_LIMIT as usize || bytes.len() % 17 != 0 {
        return Err(Code::Progress);
    }
    let mut counters = Vec::new();
    for record in bytes.as_chunks::<17>().0 {
        if record[16] != b'\n' || !record[..16].iter().all(u8::is_ascii_hexdigit) {
            return Err(Code::Progress);
        }
        let counter = u64::from_str_radix(
            std::str::from_utf8(&record[..16]).map_err(|_| Code::Progress)?,
            16,
        )
        .map_err(|_| Code::Progress)?;
        if counters.last().is_some_and(|previous| counter <= *previous) {
            return Err(Code::Progress);
        }
        counters.push(counter);
    }
    control.check(control.deadline)?;
    Ok(counters)
}

fn capture_work(owner: &mut Owner, case: CaptureCase) -> Result<(), Code> {
    owner.retain_guard()?;
    if matches!(case, CaptureCase::Version) {
        return version_capture_work(owner);
    }
    owner.control.check(owner.control.deadline)?;
    owner.control.observations.intent(Operation::CaseCurrentExe);
    let executable = std::env::current_exe();
    owner.control.observations.io(
        Operation::CaseCurrentExe,
        ChildRole::ControlCli,
        &executable,
    );
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    let role = match case {
        CaptureCase::Oversized => "oversized-v1",
        CaptureCase::Live => "live-v1",
        CaptureCase::Version => return Err(Code::Native),
    };
    let mut command = Command::new(executable);
    command
        .args([
            "--exact",
            OUTPUT_SELECTOR,
            "--nocapture",
            "--test-threads=1",
        ])
        .env("WINDOWS_CLI_OUTPUT_ROLE", role);
    let poll_span = matches!(case, CaptureCase::Live).then_some(Duration::from_secs(1));
    owner
        .captured_output(Phase::Capture, &mut command, false, poll_span)
        .map(|_| ())
}

fn version_capture_work(owner: &mut Owner) -> Result<(), Code> {
    let expected = concat!("locron ", env!("CARGO_PKG_VERSION"), "\n").as_bytes();
    owner.call_context.version_control.set(true);
    let first = owner.captured_output(
        Phase::Capture,
        owner.command()?.arg("--version"),
        true,
        None,
    )?;
    if first.status.code() != Some(0)
        || first.stdout != expected
        || owner.capture_duplicate.is_none()
    {
        return Err(Code::Native);
    }
    owner.control.calls.prove(
        1,
        owner.control.calls.snapshot().output_returned(
            1,
            Code::Success,
            u64::try_from(expected.len()).unwrap_or(u64::MAX),
        ),
    );
    let second = owner.output(Phase::Capture, owner.command()?.arg("--version"))?;
    if second.status.code() != Some(0) || second.stdout != expected {
        return Err(Code::Native);
    }
    owner.control.calls.prove(
        4,
        owner.control.calls.snapshot().output_returned(
            2,
            Code::Success,
            u64::try_from(expected.len()).unwrap_or(u64::MAX),
        ),
    );
    let first_identity = native(&owner.control, owner.control.deadline, Code::Native, || {
        owner.control.observations.intent(Operation::StdoutIdentity);
        let identity = file_identity(
            owner
                .captures
                .first()
                .ok_or_else(|| io::Error::other("missing first capture"))?,
        );
        owner
            .control
            .observations
            .io(Operation::StdoutIdentity, ChildRole::ControlCli, &identity);
        identity
    })?;
    let second_identity = native(&owner.control, owner.control.deadline, Code::Native, || {
        owner.control.observations.intent(Operation::StdoutIdentity);
        let identity = file_identity(
            owner
                .captures
                .get(1)
                .ok_or_else(|| io::Error::other("missing second capture"))?,
        );
        owner
            .control
            .observations
            .io(Operation::StdoutIdentity, ChildRole::ControlCli, &identity);
        identity
    })?;
    if first_identity == second_identity {
        return Err(Code::Native);
    }
    owner.control.check(owner.control.deadline)?;
    let number = owner.capture_count.checked_add(1).ok_or(Code::Native)?;
    let path = owner.capture_path(number)?;
    owner.control.observations.intent(Operation::StdoutCreate);
    let opened = create_private_new(&path);
    let index = owner.captures.len();
    match opened {
        Ok(file) => owner.captures.push(file),
        Err(error) => {
            owner.control.observations.error(
                Operation::StdoutCreate,
                ChildRole::ControlCli,
                &error,
            );
            owner.control.check(owner.control.deadline)?;
            return Err(Code::Native);
        }
    }
    owner.control.check(owner.control.deadline)?;
    owner
        .control
        .observations
        .intent(Operation::StdoutControlWrite);
    let written = owner
        .captures
        .get_mut(index)
        .ok_or(Code::Native)?
        .write_all(b"0");
    owner.control.observations.io(
        Operation::StdoutControlWrite,
        ChildRole::ControlCli,
        &written,
    );
    owner.control.check(owner.control.deadline)?;
    written.map_err(|_| Code::Native)?;
    owner.control.check(owner.control.deadline)?;
    owner
        .control
        .observations
        .intent(Operation::StdoutControlFlush);
    let flushed = owner.captures.get_mut(index).ok_or(Code::Native)?.flush();
    owner.control.observations.io(
        Operation::StdoutControlFlush,
        ChildRole::ControlCli,
        &flushed,
    );
    owner.control.check(owner.control.deadline)?;
    flushed.map_err(|_| Code::Native)?;
    let collision = owner.output(Phase::Capture, owner.command()?.arg("--version"));
    if !matches!(collision, Err(Code::CaptureCollision)) || owner.active_cli.is_some() {
        return Err(Code::Native);
    }
    owner
        .control
        .calls
        .prove(8, owner.control.calls.snapshot().collision_returned());
    let collision_observations = owner.control.calls.snapshot();
    let preserved = owner.collect_capture(index, Operation::CaptureCliRead)?;
    if preserved != b"0" || owner.capture_duplicate.is_none() {
        return Err(Code::Native);
    }
    owner.control.check(owner.control.deadline)?;
    owner.control.observations.intent(Operation::DropStdout);
    drop(owner.capture_reader.take());
    owner
        .control
        .calls
        .prove(16, owner.control.calls.snapshot() == collision_observations);
    owner.control.check(owner.control.deadline)
}

// BEGIN genuine paired controls (same admitted worker and original clock).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HeldLockEnvelope {
    schema: String,
    ok: bool,
    command: String,
    error: HeldLockError,
    warnings: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HeldLockError {
    code: String,
    message: String,
}

fn paired_poll_exit(owner: &mut Owner) -> Result<(), Code> {
    loop {
        owner.control.check(owner.control.deadline)?;
        let waited = owner.children[0].as_mut().ok_or(Code::Cli)?.try_wait();
        owner.control.check(owner.control.deadline)?;
        let waited = waited.map_err(|_| Code::Cli)?;
        if let Some(status) = waited {
            owner.paired.root_exited = true;
            owner.paired.proof.root_status = status.code();
            owner.paired.proof.duplicates_held =
                owner.paired.attached == 3 && owner.paired.command.is_some();
            return Ok(());
        }
        owner.paired.proof.live_seen = true;
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(5),
        )?;
    }
}

fn held_lock_pair_work(owner: &mut Owner, json: bool) -> Result<(), Code> {
    owner.control.check(owner.control.deadline)?;
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| Code::Native)?;
    owner.control.check(owner.control.deadline)?;
    owner.control.check(owner.control.deadline)?;
    let pid = std::process::id();
    owner.control.check(owner.control.deadline)?;
    let lifetime_id = uuid::Uuid::now_v7();
    owner.control.check(owner.control.deadline)?;
    let metadata = locron_store::LockMetadata {
        pid,
        lifetime_id: lifetime_id.to_string(),
        started_at_us: i64::try_from(started.as_micros()).map_err(|_| Code::Native)?,
        binary_version: env!("CARGO_PKG_VERSION").to_owned(),
    };
    let path = owner.root()?.join("daemon.lock");
    owner.control.check(owner.control.deadline)?;
    let held = DaemonLock::acquire_role(&path, &metadata, false);
    if let Ok(lock) = held {
        owner.paired.lock = Some(lock);
    } else {
        owner.control.check(owner.control.deadline)?;
        return Err(Code::Role);
    }
    owner.control.check(owner.control.deadline)?;
    let mut command = owner.command()?;
    if json {
        command.arg("--json");
    }
    command.args(["daemon", "run"]);
    owner.prepare_pair(command)?;
    owner.spawn_pair(None)?;
    owner.control.step(Phase::Capture, owner.control.deadline)?;
    paired_poll_exit(owner)?;
    let [stdout, stderr] = owner.collect_pair()?;
    if owner.paired.proof.root_status != Some(4) {
        return Err(Code::Cli);
    }
    let facts = owner.control.snapshot(Code::Success).paired.facts;
    if facts
        .iter()
        .any(|fact| fact.state != PairReadState::Complete)
    {
        return Err(Code::Cli);
    }
    if json {
        if !stderr.is_empty() || stdout.split(|byte| *byte == b'\n').count() != 2 {
            return Err(Code::Cli);
        }
        let body = stdout.strip_suffix(b"\n").ok_or(Code::Cli)?;
        let envelope: HeldLockEnvelope = serde_json::from_slice(body).map_err(|_| Code::Cli)?;
        if envelope.schema != "locron.cli/v1"
            || envelope.ok
            || envelope.command != "daemon"
            || envelope.error.code != "daemon_already_running"
            || envelope.error.message != "another locron daemon owns this state directory"
            || !envelope.warnings.is_empty()
        {
            return Err(Code::Cli);
        }
    } else if !stdout.is_empty()
        || stderr != b"error: another locron daemon owns this state directory\n"
    {
        return Err(Code::Cli);
    }
    owner.control.check(owner.control.deadline)
}

fn stderr_pair_work(owner: &mut Owner, live: bool) -> Result<(), Code> {
    owner.control.check(owner.control.deadline)?;
    let executable = std::env::current_exe();
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    let mut command = Command::new(executable);
    command
        .args([
            "--exact",
            OUTPUT_SELECTOR,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(
            "WINDOWS_CLI_OUTPUT_ROLE",
            if live {
                "live-stderr-v1"
            } else {
                "oversized-stderr-v1"
            },
        );
    owner.prepare_pair(command)?;
    owner.spawn_pair(live.then_some(Duration::from_secs(1)))?;
    owner.control.step(Phase::Capture, owner.control.deadline)?;
    // Live cannot enter collection: normal admission expires while real None
    // polls are observed. Existing complete/reap owns kill and nonzero status.
    paired_poll_exit(owner)?;
    if live {
        return Err(Code::ChildExited);
    }
    let _private_bytes = owner.collect_pair()?;
    let facts = owner.control.snapshot(Code::Success).paired.facts;
    if owner.paired.proof.root_status != Some(0)
        || facts[0].state != PairReadState::Complete
        || facts[0].bytes.is_none()
        || facts[1].state != PairReadState::Oversized
        || facts[1].bytes != Some(65_537)
    {
        return Err(Code::Cli);
    }
    Err(Code::CaptureOversized)
}

fn pair_collision_work(owner: &mut Owner) -> Result<(), Code> {
    owner.control.check(owner.control.deadline)?;
    let path = owner.pair_path(1)?;
    let sentinel = create_private_new(&path);
    if let Ok(sentinel) = sentinel {
        owner.paired.sentinel = Some(sentinel);
    } else {
        owner.control.check(owner.control.deadline)?;
        return Err(Code::Cli);
    }
    owner.control.check(owner.control.deadline)?;
    let before = file_identity(owner.paired.sentinel.as_ref().ok_or(Code::Cli)?);
    owner.control.check(owner.control.deadline)?;
    let before = before.map_err(|_| Code::Cli)?;
    owner.control.check(owner.control.deadline)?;
    let written = owner
        .paired
        .sentinel
        .as_mut()
        .ok_or(Code::Cli)?
        .write_all(b"0");
    owner.control.check(owner.control.deadline)?;
    written.map_err(|_| Code::Cli)?;
    owner.control.check(owner.control.deadline)?;
    let flushed = owner.paired.sentinel.as_mut().ok_or(Code::Cli)?.flush();
    owner.control.check(owner.control.deadline)?;
    flushed.map_err(|_| Code::Cli)?;
    let command = owner.command()?;
    let collision = owner.prepare_pair(command);
    if !matches!(collision, Err(Code::CaptureCollision))
        || owner.paired.writers[0].is_none()
        || owner.paired.writers[1].is_some()
        || owner.children.iter().any(Option::is_some)
        || owner.active_cli.is_some()
    {
        return Err(Code::Cli);
    }
    // This is no-child proof, not a fabricated root-exit/reap fact.
    owner.paired.proof.collision.no_child = true;
    let (fact, bytes) = owner.pair_read(1, true)?;
    if fact.state != PairReadState::Complete || bytes != b"0" {
        return Err(Code::Cli);
    }
    owner.control.check(owner.control.deadline)?;
    let after = file_identity(owner.paired.sentinel.as_ref().ok_or(Code::Cli)?);
    owner.control.check(owner.control.deadline)?;
    if after.map_err(|_| Code::Cli)? != before {
        return Err(Code::Cli);
    }
    owner.paired.proof.collision.collision_preserved = true;
    Err(Code::CaptureCollision)
}

fn paired_work(owner: &mut Owner, case: PairedCase) -> Result<(), Code> {
    owner.retain_guard()?;
    match case {
        PairedCase::Human => held_lock_pair_work(owner, false),
        PairedCase::Json => held_lock_pair_work(owner, true),
        PairedCase::OversizedStderr => stderr_pair_work(owner, false),
        PairedCase::LiveStderr => stderr_pair_work(owner, true),
        PairedCase::Collision => pair_collision_work(owner),
    }
}

// Independent complete literal records exercise only the private decoder, not
// an invented daemon error or native cause. No test selector is added.
fn assert_pair_decoder_controls() {
    let simple = b"windows-store-open operation=0 stage=database-open category=io kind=Uncategorized raw_os=Some(-2147483648)\n";
    let config = concat!(
        "windows-store-open operation=18446744073709551615 stage=sqlite-configure-wal",
        " category=sqlite primary=DatabaseBusy extended=261 pid=4294967295",
        " gate=after-busy phase=finalize-return attempts=18446744073709551615",
        " entry_rem_us=0 phase_us=18446744073709551615 gate_us=unobserved remaining_us=0",
        " autocommit=true mode=wal done=false observed_primary=255 observed_extended=5",
        " prepare_us=0,unobserved query_us=unobserved,0 row_us=1,2 finalize_us=3,4\n",
    )
    .as_bytes();
    for valid in [
        simple.as_slice(), config,
        b"windows-store-open operation=1 stage=shm-open category=store\n",
        b"windows-store-open operation=1 stage=wal-open category=sqlite\n",
        b"windows-store-open operation=1 stage=database-open category=io kind=InputOutputError raw_os=None\n",
        b"windows-store-open operation=1 stage=sqlite-configure-settings category=io kind=TimedOut raw_os=None pid=0 configuration=unobserved\n",
    ] {
        assert!(matches!(recognize_store_record(valid), PairRecognition::Recognized(_)), "complete literal record refused");
    }
    let PairRecognition::Recognized(observed) = recognize_store_record(config) else {
        panic!("literal configuration refused");
    };
    assert!(
        matches!(
            observed.returned,
            StoreReturned::SqliteCode {
                primary: "DatabaseBusy",
                extended: 261
            }
        ),
        "returned fields replaced by observed configuration"
    );
    let text = std::str::from_utf8(config).expect("ASCII literal");
    for invalid in [
        text.replace(
            "operation=18446744073709551615",
            "operation=18446744073709551616",
        ),
        text.replace("operation=18446744073709551615", "operation=01"),
        text.replace("stage=sqlite-configure-wal", "stage=unknown"),
        text.replace("primary=DatabaseBusy", "primary=unknown"),
        text.replace("extended=261", "extended=-0"),
        text.replace("extended=261", "extended=2147483648"),
        text.replace("gate=after-busy", "gate=unknown"),
        text.replace("phase=finalize-return", "phase=unknown"),
        text.replace("mode=wal", "mode=unknown"),
        text.replace("done=false", "done=False"),
        text.replace("observed_primary=255", "observed_primary=256"),
        text.replace("pid=4294967295", "pid=4294967296"),
        text.replace("row_us=1,2", "row_us=1"),
        text.replace("row_us=1,2", "row_us=1,2,3"),
        text.replace("category=sqlite", "category=sqlite category=sqlite"),
        text.replace(
            "pid=4294967295 gate=after-busy",
            "gate=after-busy pid=4294967295",
        ),
        text.replace(" finalize_us=3,4", ""),
        text.replace('\n', "\r\n"),
        text.trim_end_matches('\n').to_owned(),
        format!("prefix {text}"),
        format!("\u{1b}[0m{text}"),
        text.replace('\n', " suffix\n"),
    ] {
        assert!(
            matches!(
                recognize_store_record(invalid.as_bytes()),
                PairRecognition::Unrecognized
            ),
            "malformed record accepted"
        );
    }
    for raw in [
        "Some(+1)",
        "Some(-0)",
        "Some(2147483648)",
        "Some(01)",
        "unknown",
    ] {
        let invalid = std::str::from_utf8(simple)
            .expect("ASCII literal")
            .replace("Some(-2147483648)", raw);
        assert!(matches!(
            recognize_store_record(invalid.as_bytes()),
            PairRecognition::Unrecognized
        ));
    }
    let mut ambiguous = simple.to_vec();
    ambiguous.extend_from_slice(simple);
    assert!(matches!(
        recognize_store_record(&ambiguous),
        PairRecognition::Ambiguous
    ));
    assert!(matches!(
        recognize_store_record(b"private arbitrary text\n"),
        PairRecognition::Unobserved
    ));
    let overlong = format!("windows-store-open {}\n", "x".repeat(769));
    assert!(matches!(
        recognize_store_record(overlong.as_bytes()),
        PairRecognition::Unrecognized
    ));
}
// END genuine paired controls.

// Pure masks/decoding controls are not native producer or History evidence.
fn assert_call_word_domains() {
    for (kind, fields, expected) in [
        (
            CallWordKind::Cli,
            [513, 1023, 14, 5, 30_000_000, 0, 0],
            0x800e_4e1c_05ef_fe01_u64,
        ),
        (
            CallWordKind::Cli,
            [1, 0, 0, 0, 0, 0, 0],
            0x8000_0000_0000_0001_u64,
        ),
        (
            CallWordKind::NativeEnter,
            [30_000, 513, 262_143, 61, 14, 3, 0],
            0x807d_efff_ff00_f530_u64,
        ),
        (
            CallWordKind::NativeReturn,
            [30_000, 513, 262_143, 59, 14, 3, 4],
            0x847d_dfff_ff00_f530_u64,
        ),
        (
            CallWordKind::Wait,
            [2_147_483_648, 1, 513, 262_143, 2, 0, 0],
            0xdfff_fc03_8000_0000_u64,
        ),
        (
            CallWordKind::Wait,
            [2_147_483_647, 1, 513, 262_143, 2, 0, 0],
            0xdfff_fc03_7fff_ffff_u64,
        ),
        (
            CallWordKind::Wait,
            [0, 0, 1, 1, 1, 0, 0],
            0xa000_0802_0000_0000_u64,
        ),
        (
            CallWordKind::Wait,
            [0, 0, 1, 1, 2, 0, 0],
            0xc000_0802_0000_0000_u64,
        ),
        (
            CallWordKind::Wait,
            [0, 0, 1, 1, 3, 0, 0],
            0xe000_0802_0000_0000_u64,
        ),
        (
            CallWordKind::Wrapper,
            [14, 513, 1023, 14, 5, 30_000_000, 0],
            0x80e4_e1c0_5eff_e01e_u64,
        ),
        (
            CallWordKind::Read,
            [65_537, 513, 262_143, 0, 0, 0, 0],
            0x8000_1fff_fc03_0001_u64,
        ),
        (
            CallWordKind::Read,
            [0, 1, 1, 0, 0, 0, 0],
            0x8000_0000_0802_0000_u64,
        ),
        (
            CallWordKind::History,
            [5, 513, 1023, 5, 30_000_000, 0, 0],
            0x8007_270e_02ff_f00d_u64,
        ),
        (
            CallWordKind::Cost,
            [30_000_000, 30_000_000, 1023, 0, 0, 0, 0],
            0x8fff_9387_01c9_c380_u64,
        ),
        (
            CallWordKind::Progress,
            [3855, 1023, 513, 30_000_000, 5, 2, 0],
            0xabc9_c380_807f_ff0f_u64,
        ),
        (
            CallWordKind::Progress,
            [0, 0, 0, 0, 0, 1, 0],
            0x9000_0000_0000_0000_u64,
        ),
    ] {
        assert_eq!(
            call_word(kind, fields),
            expected,
            "independent literal encoding"
        );
        assert_eq!(
            call_fields(kind, expected),
            Some(fields),
            "independent literal decoding"
        );
        assert_ne!(expected, 0);
        assert_ne!(expected, CALL_INVALID);
        assert_ne!(expected, CALL_OVERFLOW);
    }
    for kind in [
        CallWordKind::Cli,
        CallWordKind::NativeEnter,
        CallWordKind::NativeReturn,
        CallWordKind::Wait,
        CallWordKind::Wrapper,
        CallWordKind::Read,
        CallWordKind::History,
        CallWordKind::Cost,
        CallWordKind::Progress,
    ] {
        for word in [0, 1, 2, u64::MAX] {
            assert!(
                call_fields(kind, word).is_none(),
                "sentinel/domain collision"
            );
        }
    }
    let mut association = CallSnapshot {
        current_cli: 0x8000_0000_02a0_0802,
        native_enter: 0,
        native_return: 0,
        current_wait: 0,
        wrapper_return: 0,
        capture_read: 0,
        last_history: 0x8000_0000_0100_200a,
        history_cost: 0,
        last_progress: 0xa400_0000_0040_1000,
        daemon_spawn: 0,
    };
    assert_eq!(
        association.relation(CallWordKind::History, association.last_history),
        "older"
    );
    assert_eq!(
        association.relation(CallWordKind::Progress, association.last_progress),
        "older"
    );
    association.current_cli = 0x8000_0000_02a0_0401;
    assert_eq!(
        association.relation(CallWordKind::History, association.last_history),
        "same"
    );
    association.current_cli = 0x8000_0000_02a0_0801;
    assert_eq!(
        association.relation(CallWordKind::History, association.last_history),
        "mixed"
    );
    assert_eq!(signed_payload(Some(i32::MIN)), 0x1_8000_0000);
    assert_eq!(signed_payload(Some(i32::MAX)), 0x1_7fff_ffff);
    assert_eq!(call_word(CallWordKind::Cli, [514, 0, 0, 0, 0, 0, 0]), 2);
    assert_eq!(call_word(CallWordKind::Cli, [1, 1024, 0, 0, 0, 0, 0]), 2);
    assert_eq!(
        call_word(CallWordKind::Cli, [1, 0, 0, 0, 30_000_001, 0, 0]),
        2
    );
    assert_eq!(
        call_word(CallWordKind::NativeEnter, [30_001, 1, 1, 53, 14, 3, 0]),
        2
    );
    assert_eq!(
        call_word(CallWordKind::NativeEnter, [0, 1, 262_144, 53, 14, 3, 0]),
        2
    );
    assert_eq!(call_word(CallWordKind::Read, [65_538, 1, 1, 0, 0, 0, 0]), 2);
    assert_eq!(
        call_word(CallWordKind::Progress, [3856, 0, 1, 0, 0, 2, 0]),
        2
    );
    assert_eq!(call_word(CallWordKind::Cli, [0, 0, 0, 0, 0, 0, 0]), 1);
    assert_eq!(call_word(CallWordKind::Cli, [1, 0, 15, 0, 0, 0, 0]), 1);
    assert_eq!(call_word(CallWordKind::Cli, [1, 0, 0, 6, 0, 0, 0]), 1);
    assert_eq!(call_word(CallWordKind::Cli, [1, 0, 0, 0, 0, 1, 0]), 1);
    assert_eq!(call_word(CallWordKind::Wait, [1, 0, 1, 1, 2, 0, 0]), 1);
    assert_eq!(call_word(CallWordKind::Wait, [1, 1, 1, 1, 1, 0, 0]), 1);
    assert_eq!(
        call_word(CallWordKind::NativeEnter, [0, 1, 1, 53, 14, 3, 1]),
        1
    );
    assert_eq!(call_word(CallWordKind::Progress, [1, 0, 1, 0, 0, 1, 0]), 1);
    assert_eq!(call_word(CallWordKind::Cost, [2, 1, 1, 0, 0, 0, 0]), 1);
    assert!(
        call_fields(CallWordKind::Cli, 0x8010_0000_0000_0001).is_none(),
        "reserved bit accepted"
    );
    assert!(
        call_fields(CallWordKind::NativeReturn, 0x8800_0000_0000_0000).is_none(),
        "reserved bit accepted"
    );
    for (kind, word, expected) in [
        (CallWordKind::Cli, 0, "unobserved"),
        (CallWordKind::Cli, 1, "invalid"),
        (CallWordKind::Cli, 2, "overflow"),
        (CallWordKind::Daemon, 0, "unobserved"),
        (CallWordKind::Daemon, 1, "0"),
        (CallWordKind::Daemon, 30_000_001, "30000000"),
        (CallWordKind::Daemon, u64::MAX, "invalid"),
        (CallWordKind::Daemon, u64::MAX - 1, "overflow"),
    ] {
        assert_eq!(CallWordDisplay { kind, word }.to_string(), expected);
    }
}

#[test]
fn native_cli_output_capture_contract() {
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(30);
    assert_call_word_domains();
    for case in [
        CaptureCase::Version,
        CaptureCase::Oversized,
        CaptureCase::Live,
    ] {
        assert!(
            Instant::now() < deadline,
            "capture gate expired before admission"
        );
        let mut driver = admit_control(Control::new(entered, deadline), None)
            .expect("resource-free capture admission failed");
        assert!(
            driver.dispatch(CaseKind::Capture(case)).is_ok(),
            "capture dispatch refused"
        );
        let observed = driver.receive_until(deadline);
        let completed = if observed.flags & CLEANED == 0 {
            driver.wait_cleanup_until(deadline)
        } else {
            observed
        };
        driver.finish_if_returned();
        assert!(
            Instant::now() < deadline,
            "capture gate cleanup exceeded original horizon"
        );
        assert_eq!(
            completed.flags & (REAPED | CLEANED),
            REAPED | CLEANED,
            "actual capture cleanup unconfirmed: {completed}"
        );
        let (operation, role, payload) = event_header(completed.observations.statuses[2], 2)
            .expect("actual capture producer status is unobserved");
        assert!(
            matches!(role, ChildRole::ControlCli),
            "wrong capture producer role"
        );
        let actual_code = decode_signed(payload).ok().flatten();
        match case {
            CaptureCase::Version => {
                assert!(
                    completed.capture_proof == 31 && completed.calls.collision_returned(),
                    "version epoch/reset/retention evidence failed: {completed}"
                );
                assert!(
                    completed.succeeded(),
                    "version/collision capture control failed: {completed}"
                );
                assert_eq!(
                    actual_code,
                    Some(0),
                    "actual version exit failed: {completed}"
                );
                assert_eq!(
                    completed.stdout_bytes, 1,
                    "collision bytes changed: {completed}"
                );
                println!(
                    "capture_control=version exit=0 held_writer=true fresh=2 collision=AlreadyExists preserved_bytes=1 cleanup=confirmed"
                );
            }
            CaptureCase::Oversized => {
                assert!(
                    completed
                        .calls
                        .output_returned(1, Code::CaptureOversized, 65_537),
                    "oversized epoch/actual return evidence failed: {completed}"
                );
                assert_eq!(
                    completed.code,
                    Code::CaptureOversized,
                    "explicit output cap refusal missing: {completed}"
                );
                assert_eq!(
                    actual_code,
                    Some(0),
                    "actual oversized producer exit failed: {completed}"
                );
                assert_eq!(
                    completed.stdout_bytes, 65_537,
                    "bounded sentinel read missing: {completed}"
                );
                println!(
                    "capture_control=oversized exit=0 read_count=65537 refusal=CaptureOversized cleanup=confirmed"
                );
            }
            CaptureCase::Live => {
                assert!(
                    completed.calls.live_returned(),
                    "live epoch/None/no-read retention failed: {completed}"
                );
                assert_eq!(
                    completed.code,
                    Code::Expired,
                    "live producer did not expire: {completed}"
                );
                assert!(
                    completed.cli_live_seen,
                    "actual live Child was never observed: {completed}"
                );
                assert_eq!(
                    completed.stdout_bytes, UNOBSERVED_BYTES,
                    "expired producer entered output read: {completed}"
                );
                assert_eq!(
                    operation,
                    Operation::CleanupWait as u8,
                    "actual kill/reap path missing: {completed}"
                );
                assert!(
                    actual_code.is_some_and(|code| code != 0),
                    "actual killed producer exit missing: {completed}"
                );
                assert_eq!(
                    completed.observations.io_error, 0,
                    "actual capture kill or I/O failed: {completed}"
                );
                assert_ne!(
                    completed.observations.first_work, 0,
                    "completed live refusal is unobserved"
                );
                println!(
                    "capture_control=live observed=None poll_horizon_ms=1000 refusal=Expired output_read=unobserved kill_reap=confirmed cleanup=confirmed"
                );
            }
        }
    }
    // BEGIN appended genuine pair cases after the entire original f90 loop.
    assert_pair_decoder_controls();
    let mut original_ids = Vec::with_capacity(4);
    for case in [
        PairedCase::Human,
        PairedCase::Json,
        PairedCase::OversizedStderr,
        PairedCase::LiveStderr,
        PairedCase::Collision,
    ] {
        assert!(
            Instant::now() < deadline,
            "paired control expired before admission"
        );
        let mut driver = admit_control(Control::new(entered, deadline), None)
            .expect("resource-free pair admission failed");
        assert!(
            driver.dispatch(CaseKind::Paired(case)).is_ok(),
            "pair dispatch refused"
        );
        let observed = driver.receive_until(deadline);
        let completed = if observed.flags & CLEANED == 0 {
            driver.wait_cleanup_until(deadline)
        } else {
            observed
        };
        driver.finish_if_returned();
        assert!(
            Instant::now() < deadline,
            "paired cleanup exceeded original horizon"
        );
        if matches!(case, PairedCase::Collision) {
            assert_eq!(
                completed.code,
                Code::CaptureCollision,
                "second-leaf collision refused incorrectly: {completed}"
            );
            assert!(
                completed.paired.proof.collision.no_child
                    && completed.paired.proof.collision.collision_preserved,
                "actual no-child/sentinel proof missing: {completed}"
            );
            assert_eq!(
                completed.flags & (REAPED | CLEANED),
                CLEANED,
                "no-child state cleanup unconfirmed: {completed}"
            );
            assert!(
                completed.paired.proof.root_status.is_none(),
                "collision invented root status"
            );
            println!(
                "paired_capture_control=second_leaf_collision refusal=AlreadyExists no_child=true sentinel_bytes=1 same_full_id=true cleanup=confirmed"
            );
            continue;
        }
        assert_eq!(
            completed.flags & (REAPED | CLEANED),
            REAPED | CLEANED,
            "actual paired root cleanup unconfirmed: {completed}"
        );
        assert!(
            completed.paired.proof.duplicates_held,
            "both real duplicates were not retained through root exit: {completed}"
        );
        match case {
            PairedCase::Human | PairedCase::Json => {
                assert!(
                    completed.succeeded(),
                    "actual held-lock route failed: {completed}"
                );
                assert_eq!(
                    completed.paired.proof.root_status,
                    Some(4),
                    "actual daemon code4 missing: {completed}"
                );
                assert_eq!(
                    completed.paired.proof.zero_cursors,
                    [true, true],
                    "independent zero-cursor proof missing"
                );
                assert!(
                    completed
                        .paired
                        .facts
                        .iter()
                        .all(|fact| fact.state == PairReadState::Complete),
                    "both actual streams were not complete"
                );
                for identity in completed.paired.proof.identities {
                    let identity = identity.expect("complete original file identity missing");
                    assert!(
                        !original_ids.contains(&identity),
                        "Human/JSON original objects collided"
                    );
                    original_ids.push(identity);
                }
                let route = if matches!(case, PairedCase::Json) {
                    "json"
                } else {
                    "human"
                };
                println!(
                    "paired_capture_control={route} exit=4 strict_route=true empty_counterpart=true held_duplicates=true same_reader_ids=true zero_cursors=true cleanup=confirmed"
                );
            }
            PairedCase::OversizedStderr => {
                assert_eq!(
                    completed.code,
                    Code::CaptureOversized,
                    "genuine stderr cap refusal missing: {completed}"
                );
                assert_eq!(
                    completed.paired.proof.root_status,
                    Some(0),
                    "actual stderr producer exit missing"
                );
                assert_eq!(
                    completed.paired.proof.zero_cursors,
                    [true, true],
                    "actual independent readers missing"
                );
                assert!(
                    completed.paired.facts[0].state == PairReadState::Complete
                        && completed.paired.facts[0]
                            .bytes
                            .is_some_and(|count| count <= 65_536),
                    "genuine harness stdout incomplete"
                );
                assert!(
                    completed.paired.facts[1].state == PairReadState::Oversized
                        && completed.paired.facts[1].bytes == Some(65_537),
                    "genuine stderr sentinel read missing"
                );
                println!(
                    "paired_capture_control=stderr_oversized exit=0 stderr_count=65537 refusal=CaptureOversized actual_harness_stdout=complete cleanup=confirmed"
                );
            }
            PairedCase::LiveStderr => {
                assert_eq!(
                    completed.code,
                    Code::Expired,
                    "live stderr did not refuse before read: {completed}"
                );
                assert!(completed.paired.proof.live_seen, "actual live None missing");
                assert!(
                    completed
                        .paired
                        .proof
                        .root_status
                        .is_some_and(|code| code != 0),
                    "actual killed root status missing"
                );
                assert!(completed.paired.facts.iter().all(|fact| fact.state == PairReadState::Unobserved && fact.bytes.is_none()), "live stream entered read");
                assert_eq!(
                    completed.paired.proof.zero_cursors,
                    [false, false],
                    "live stream opened a reader"
                );
                println!(
                    "paired_capture_control=stderr_live observed=None refusal=Expired both_reads=unobserved kill_reap=confirmed cleanup=confirmed"
                );
            }
            PairedCase::Collision => unreachable!("collision handled without a child"),
        }
    }
    assert_eq!(
        original_ids.len(),
        4,
        "four genuine original identities missing"
    );
    // END appended genuine pair cases.
    #[cfg(debug_assertions)]
    producer_controls(entered, deadline);
}

// Selected report-only projection: all fields below are fixed scalar facts.
#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
struct ProducerControlScalar {
    code: Code,
    phase: Phase,
    flags: u8,
    elapsed_us: u128,
    first_work: u64,
}

#[cfg(debug_assertions)]
impl ProducerControlScalar {
    fn from_result(result: &CaseResult) -> Self {
        Self {
            code: result.code,
            phase: result.phase,
            flags: result.flags,
            elapsed_us: result.elapsed_us,
            first_work: result.observations.first_work,
        }
    }
}

#[cfg(debug_assertions)]
impl fmt::Display for ProducerControlScalar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{{code={:?} phase={:?} flags={} elapsed_us={} first_work={}}}",
            self.code,
            self.phase,
            self.flags,
            self.elapsed_us,
            EventDisplay {
                word: self.first_work,
                tag: 3,
                expected_role: Some(ChildRole::NoChild),
            }
        )
    }
}

// BEGIN selected producer controls, after all original capture/pair cases.
#[cfg(debug_assertions)]
fn producer_controls(entered: Instant, deadline: Instant) {
    assert_wake_decoder_controls();
    for case in [ProducerControl::Queued, ProducerControl::Cancellation] {
        assert!(
            Instant::now() < deadline,
            "producer control expired before admission"
        );
        let mut driver = admit_control(Control::new(entered, deadline), None)
            .expect("resource-free producer admission failed");
        assert!(
            driver.dispatch(CaseKind::Producer(case)).is_ok(),
            "producer dispatch refused"
        );
        let observed = driver.receive_until(deadline);
        let first_observation = ProducerControlScalar::from_result(&observed);
        let completed = if observed.flags & CLEANED == 0 {
            driver.wait_cleanup_until(deadline)
        } else {
            observed
        };
        let snapshot = driver
            .control
            .producer_diagnostic
            .get()
            .copied()
            .unwrap_or(ProducerSnapshot::UNOBSERVED);
        driver.finish_if_returned();
        eprintln!(
            "wake_producer_case/v1 case={} first={} completion_snapshot={} released={}",
            match case {
                ProducerControl::Queued => "queued",
                ProducerControl::Cancellation => "cancel",
            },
            first_observation,
            ProducerControlScalar::from_result(&completed),
            snapshot.released,
        );
        if !completed.succeeded() {
            eprintln!(
                "{}",
                ProducerSummary {
                    snapshot,
                    cancel: matches!(case, ProducerControl::Cancellation)
                }
            );
        }
        assert!(
            Instant::now() < deadline,
            "producer control exceeded the SAME capture horizon"
        );
        assert!(
            completed.succeeded(),
            "genuine producer work/owned cleanup failed: {completed}"
        );
        assert!(
            snapshot.released,
            "producer carrier release was not confirmed"
        );
        assert_eq!(
            snapshot.run_status,
            Some(0),
            "genuine initial Run exit was not zero"
        );
        assert!(snapshot.run_queued, "genuine initial Run did not queue");
        let run = snapshot.facts[1];
        assert!(
            run.capture == ProducerCapture::Complete,
            "genuine Run carrier was not complete"
        );
        assert!(
            run.bytes.is_some_and(|count| count > 0 && count <= 65_536),
            "Run carrier byte bound failed"
        );
        assert!(
            run.records.is_some_and(|count| count > 0 && count <= 255),
            "Run carrier record bound failed"
        );
        assert!(
            snapshot.identities[1].is_some() && snapshot.zero_cursors[1],
            "actual independent Run reader proof missing"
        );
        assert!(
            run.enqueue.edge == Some(1) && !run.enqueue.pending,
            "actual enqueue Ready-success missing"
        );
        assert!(
            matches!(run.hint.edge, Some(1 | 2)) && !run.hint.pending,
            "actual hint Ready-result missing"
        );
        match case {
            ProducerControl::Queued => {
                assert!(
                    snapshot.identities[0].is_none(),
                    "listener-free control acquired a daemon carrier"
                );
                println!(
                    "wake_producer_control=queued run_exit=0 queued=true enqueue=ok hint=returned independent_reader=true zero_cursor=true cleanup=confirmed"
                );
            }
            ProducerControl::Cancellation => {
                let daemon = snapshot.facts[0];
                assert!(
                    daemon.capture == ProducerCapture::Complete,
                    "genuine daemon carrier was not complete"
                );
                assert!(
                    daemon
                        .bytes
                        .is_some_and(|count| count > 0 && count <= 65_536),
                    "daemon carrier byte bound failed"
                );
                assert!(
                    daemon
                        .records
                        .is_some_and(|count| count > 0 && count <= 255),
                    "daemon carrier record bound failed"
                );
                assert!(
                    snapshot.identities[0].is_some()
                        && snapshot.identities[0] != snapshot.identities[1]
                        && snapshot.zero_cursors[0],
                    "actual distinct daemon reader proof missing"
                );
                let (_, role, payload) = event_header(completed.observations.statuses[0], 2)
                    .expect("actual daemon returned status is unobserved");
                assert!(
                    role == ChildRole::Daemon && decode_signed(payload).ok().flatten().is_some(),
                    "actual daemon reap/status proof missing"
                );
                assert!(
                    matches!(snapshot.cancel, CancelReply::Requested),
                    "actual requested Cancel reply missing"
                );
                let Some((_, attempt)) = daemon.matched else {
                    panic!("genuine Run attempt binding missing");
                };
                assert!(
                    attempt.begun && attempt.mark.edge == Some(1) && attempt.mark.value == Some(1),
                    "actual matched mark-running true missing"
                );
                assert!(
                    attempt.poll.edge == Some(1) && attempt.poll.value == Some(1),
                    "actual matched cancellation-poll true missing"
                );
                assert!(
                    attempt.signal != 0 && attempt.signalled_poll == attempt.poll.returned,
                    "actual post-poll cancel signal missing"
                );
                assert!(
                    attempt.execution.edge == Some(1) && attempt.execution.value == Some(5),
                    "actual runner Cancelled result missing"
                );
                assert!(
                    attempt.completion.edge == Some(1) && !attempt.completion.pending,
                    "actual completion Ready-success missing"
                );
                println!(
                    "wake_producer_control=cancel run_exit=0 queued=true cancel_reply=requested mark=true poll=true signal=returned execution=cancelled completion=ok independent_readers=true zero_cursors=true cleanup=confirmed"
                );
            }
        }
    }
}

#[cfg(debug_assertions)]
fn wake_phase_vector(
    sequence: u16,
    tick: u8,
    attempt: u8,
    op: &str,
    edge: &str,
    value: Option<u64>,
) -> String {
    format!(
        "locron_wake_phase/v2 ctx={} role=daemon seq={sequence} tick={tick} attempt={attempt} op={op} edge={edge} value={value:?} kind=none raw=None t_us={sequence}\n",
        "00".repeat(16)
    )
}

#[cfg(debug_assertions)]
fn wake_binding_vector(sequence: u16, attempt: u8, uuid: [u8; 16], ordinal: u32) -> String {
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in wake_digest([0; 16], uuid, ordinal) {
        write!(&mut digest, "{byte:02x}").expect("fixed digest formatting failed");
    }
    format!(
        "locron_wake_bind/v1 ctx={} role=daemon seq={sequence} attempt={attempt} ordinal={ordinal} corr={digest} t_us={sequence}\n",
        "00".repeat(16)
    )
}

#[cfg(debug_assertions)]
fn assert_wake_decoder_controls() {
    // Closed byte/model controls have ZERO native/effect/causal acceptance.
    let mut rows = Vec::new();
    for op in ["role_lock", "store_open", "settings", "begin_lifetime"] {
        for edge in ["enter", "ok"] {
            rows.push(wake_phase_vector(
                u16::try_from(rows.len() + 1).expect("bounded vector"),
                0,
                0,
                op,
                edge,
                None,
            ));
        }
    }
    for (op, edge, value) in [
        ("tick", "enter", None),
        ("reconcile", "enter", None),
        ("reconcile", "ok", Some(0)),
        ("maintain", "enter", None),
        ("maintain", "ok", None),
        ("capacity", "ok", Some(0)),
        ("tick", "ok", None),
    ] {
        rows.push(wake_phase_vector(
            u16::try_from(rows.len() + 1).expect("bounded vector"),
            1,
            0,
            op,
            edge,
            value,
        ));
    }
    rows.push(wake_binding_vector(16, 1, [1; 16], 1));
    rows.push(wake_phase_vector(17, 0, 1, "attempt_begin", "enter", None));
    rows.push(wake_phase_vector(18, 0, 1, "mark_running", "enter", None));
    rows.push(wake_binding_vector(19, 2, [2; 16], 1));
    rows.push(wake_phase_vector(20, 0, 2, "attempt_begin", "enter", None));
    rows.push(wake_phase_vector(21, 0, 2, "mark_running", "enter", None));
    rows.push(wake_phase_vector(22, 0, 2, "mark_running", "ok", Some(1)));
    rows.push(wake_phase_vector(23, 0, 2, "execution", "enter", None));
    rows.push(wake_phase_vector(24, 0, 1, "mark_running", "ok", Some(1)));
    rows.push(wake_phase_vector(25, 0, 1, "execution", "enter", None));
    rows.push(wake_phase_vector(26, 0, 1, "cancel_poll", "enter", None));
    rows.push(wake_phase_vector(27, 0, 1, "cancel_poll", "ok", Some(1)));
    rows.push(wake_phase_vector(28, 0, 1, "cancel_signal", "ok", None));
    rows.push(wake_phase_vector(29, 0, 2, "execution", "ok", Some(1)));
    rows.push(wake_phase_vector(30, 0, 2, "completion", "enter", None));
    rows.push(wake_phase_vector(31, 0, 2, "completion", "ok", None));
    rows.push(wake_phase_vector(32, 0, 1, "execution", "ok", Some(5)));
    rows.push(wake_phase_vector(33, 0, 1, "completion", "enter", None));
    rows.push(wake_phase_vector(34, 0, 1, "completion", "ok", None));
    let valid = rows.concat();
    let Ok(ledger) = decode_wake(valid.as_bytes(), [0; 16], false) else {
        panic!("valid interleaved ledger refused");
    };
    assert_eq!(ledger.count, 2);
    let matched = ProducerFact::from_bytes(valid.as_bytes(), [0; 16], false, Some([1; 16]));
    let Some((epoch, attempt)) = matched.matched else {
        panic!("derived genuine-key vector did not match");
    };
    assert_eq!(epoch, 1);
    assert!(attempt.execution.value == Some(5) && attempt.completion.edge == Some(1));
    let foreign = ProducerFact::from_bytes(valid.as_bytes(), [0; 16], false, Some([3; 16]));
    assert!(foreign.matched.is_none(), "unrelated key became matched");
    let trailing = rows[..16].concat();
    let Ok(binding_only) = decode_wake(trailing.as_bytes(), [0; 16], false) else {
        panic!("trailing binding prefix refused");
    };
    assert!(
        !binding_only.attempts[0].begun,
        "binding fabricated task-body entry"
    );
    let mut retry = rows[..15].concat();
    retry.push_str(&wake_binding_vector(16, 1, [1; 16], 2));
    retry.push_str(&wake_phase_vector(17, 0, 1, "attempt_begin", "enter", None));
    retry.push_str(&wake_phase_vector(18, 0, 1, "mark_running", "enter", None));
    retry.push_str(&wake_phase_vector(19, 0, 1, "mark_running", "err", None));
    retry.push_str(&wake_phase_vector(20, 0, 1, "mark_running", "enter", None));
    let Ok(retried) = decode_wake(retry.as_bytes(), [0; 16], false) else {
        panic!("legal retry prefix refused");
    };
    assert_eq!(retried.attempts[0].mark.enter, 20);
    assert_eq!(retried.attempts[0].mark.returned, 19);
    assert!(
        retried.attempts[0].mark.pending,
        "pending retry replaced its earlier return"
    );
    for invalid in [
        valid.replacen("seq=1 ", "seq=01 ", 1),
        valid.replacen("seq=1 ", "seq=2 ", 1),
        valid.replacen("role=daemon", "role=run", 1),
        valid.replacen("ctx=00", "ctx=10", 1),
        valid.replacen("ordinal=1 ", "ordinal=0 ", 1),
        valid.replacen("ordinal=1 ", "ordinal=4294967296 ", 1),
        valid.replacen("attempt=2 ordinal=1", "attempt=1 ordinal=1", 1),
        valid.replacen("seq=19 attempt=2", "seq=19 attempt=65", 1),
        valid.replacen("seq=20 tick=0 attempt=2", "seq=20 tick=0 attempt=3", 1),
        valid.replacen(
            "op=mark_running edge=ok value=Some(1)",
            "op=mark_running edge=ok value=None",
            1,
        ),
        valid.replacen(
            "op=mark_running edge=ok value=Some(1)",
            "op=mark_running edge=ok value=Some(0)",
            1,
        ),
        valid.replacen(
            "op=cancel_poll edge=ok value=Some(1)",
            "op=cancel_poll edge=ok value=Some(0)",
            1,
        ),
        valid.replacen("op=cancel_signal edge=ok", "op=cancel_signal edge=enter", 1),
        valid.replacen(
            "op=completion edge=ok value=None",
            "op=completion edge=err value=Some(2)",
            1,
        ) + &wake_phase_vector(35, 0, 2, "completion", "enter", None),
        valid.replacen("raw=None", "raw=Some(-0)", 1),
        valid.replacen("raw=None", "raw=Some(2)", 1),
        valid.replacen(
            "t_us=1",
            "run_uuid=00000000-0000-0000-0000-000000000001 t_us=1",
            1,
        ),
        format!("\u{1b}[0m{valid}"),
        valid.replace('\n', "\r\n"),
        valid.trim_end_matches('\n').to_owned(),
        "x".repeat(65_537),
        rows[..19].concat() + &wake_phase_vector(20, 0, 1, "attempt_begin", "enter", None),
        rows[..28].concat() + &wake_phase_vector(29, 0, 1, "cancel_signal", "ok", None),
        valid.clone() + &wake_phase_vector(35, 0, 1, "execution", "enter", None),
        valid.clone() + &wake_phase_vector(35, 1, 0, "admit", "enter", None),
        valid.clone() + &wake_phase_vector(35, 2, 0, "tick", "enter", None),
        valid.clone() + &wake_phase_vector(256, 0, 1, "cancel_poll", "enter", None),
    ] {
        assert!(
            decode_wake(invalid.as_bytes(), [0; 16], false).is_err(),
            "illegal closed stream accepted"
        );
        assert!(
            ProducerFact::from_bytes(invalid.as_bytes(), [0; 16], false, Some([1; 16]))
                .matched
                .is_none(),
            "refused stream leaked selected facts"
        );
    }
    let mut duplicate = rows[..18].to_vec();
    duplicate.push(wake_binding_vector(19, 2, [1; 16], 1));
    assert!(
        decode_wake(duplicate.concat().as_bytes(), [0; 16], false).is_err(),
        "duplicate digest/ordinal remapped"
    );
    let mut terminal = valid.clone();
    terminal.push_str(&wake_phase_vector(35, 0, 0, "limit", "err", None));
    assert!(matches!(
        decode_wake(terminal.as_bytes(), [0; 16], false),
        Err(ProducerCapture::Overflow)
    ));
    terminal.push_str(&wake_phase_vector(36, 0, 1, "cancel_poll", "enter", None));
    assert!(matches!(
        decode_wake(terminal.as_bytes(), [0; 16], false),
        Err(ProducerCapture::Malformed)
    ));
    let mut ceiling = rows[..8].concat();
    ceiling.push_str(&wake_binding_vector(9, 1, [1; 16], 1));
    ceiling.push_str(&wake_phase_vector(10, 0, 1, "attempt_begin", "enter", None));
    for sequence in 11..=255 {
        ceiling.push_str(&wake_phase_vector(
            sequence,
            0,
            1,
            "mark_running",
            if sequence % 2 == 1 { "enter" } else { "err" },
            None,
        ));
    }
    assert!(
        decode_wake(ceiling.as_bytes(), [0; 16], false).is_ok(),
        "legal 255-record pending prefix refused"
    );
    let overflow = ceiling.clone() + &wake_phase_vector(256, 0, 0, "limit", "err", None);
    assert!(matches!(
        decode_wake(overflow.as_bytes(), [0; 16], false),
        Err(ProducerCapture::Overflow)
    ));
    ceiling.push_str(&wake_phase_vector(256, 0, 1, "mark_running", "err", None));
    assert!(
        decode_wake(ceiling.as_bytes(), [0; 16], false).is_err(),
        "256th ordinary record accepted"
    );
    let mut epochs = rows[..8].concat();
    for epoch in 1..=64 {
        let sequence = 7 + u16::from(epoch) * 2;
        epochs.push_str(&wake_binding_vector(
            sequence,
            epoch,
            [1; 16],
            u32::from(epoch),
        ));
        epochs.push_str(&wake_phase_vector(
            sequence + 1,
            0,
            epoch,
            "attempt_begin",
            "enter",
            None,
        ));
    }
    assert!(
        decode_wake(epochs.as_bytes(), [0; 16], false).is_ok(),
        "64 immutable bindings refused"
    );
    epochs.push_str(&wake_binding_vector(137, 65, [1; 16], 65));
    assert!(
        decode_wake(epochs.as_bytes(), [0; 16], false).is_err(),
        "65th binding accepted"
    );
    let run = [
        "locron_wake_phase/v2 ctx=00000000000000000000000000000000 role=run seq=1 tick=0 attempt=0 op=enqueue edge=enter value=None kind=none raw=None t_us=0\n",
        "locron_wake_phase/v2 ctx=00000000000000000000000000000000 role=run seq=2 tick=0 attempt=0 op=enqueue edge=ok value=None kind=none raw=None t_us=1\n",
        "locron_wake_phase/v2 ctx=00000000000000000000000000000000 role=run seq=3 tick=0 attempt=0 op=hint edge=enter value=None kind=none raw=None t_us=2\n",
        "locron_wake_phase/v2 ctx=00000000000000000000000000000000 role=run seq=4 tick=0 attempt=0 op=hint edge=err value=None kind=PermissionDenied raw=Some(-2147483648) t_us=3\n",
        "locron_wake_phase/v2 ctx=00000000000000000000000000000000 role=run seq=5 tick=0 attempt=0 op=run_return edge=ok value=None kind=none raw=None t_us=4\n",
    ].concat();
    let Ok(run_ledger) = decode_wake(run.as_bytes(), [0; 16], true) else {
        panic!("actual hint-result grammar refused");
    };
    assert_eq!(run_ledger.hint_raw, Some(i32::MIN));
    assert_eq!(run_ledger.hint_kind, "PermissionDenied");
    for invalid in [
        run.replacen("raw=Some(-2147483648)", "raw=Some(2147483648)", 1),
        run.replacen("role=run", "role=daemon", 1),
        run.replacen("op=hint edge=err", "op=hint edge=ok", 1),
    ] {
        assert!(
            decode_wake(invalid.as_bytes(), [0; 16], true).is_err(),
            "invalid Run I/O grammar accepted"
        );
    }
    for cancel in [false, true] {
        let lines = ProducerSummary {
            snapshot: ProducerSnapshot::UNOBSERVED,
            cancel,
        }
        .to_string();
        assert_eq!(lines.lines().count(), 2);
        assert!(
            lines
                .lines()
                .all(|line| line.is_ascii() && line.len() + 2 <= 512)
        );
        for private in ["ctx=", "corr=", "ordinal=", "run_uuid=", "00000000-0000-"] {
            assert!(
                !lines.contains(private),
                "private correlation escaped closed summaries"
            );
        }
    }
}
// END selected producer controls.

#[test]
fn native_cli_output_target() {
    let Some(role) = std::env::var_os("WINDOWS_CLI_OUTPUT_ROLE") else {
        return;
    };
    let entered = Instant::now();
    match role.to_str() {
        Some("oversized-v1") => {
            let deadline = entered + Duration::from_secs(30);
            let payload = vec![b'x'; 65_537];
            assert!(
                Instant::now() < deadline,
                "output producer expired before lock"
            );
            let mut stdout = io::stdout().lock();
            assert!(
                Instant::now() < deadline,
                "output producer expired before write"
            );
            let written = stdout.write_all(&payload);
            assert!(
                Instant::now() < deadline,
                "output producer write returned late"
            );
            assert!(written.is_ok(), "output producer write refused");
            let flushed = stdout.flush();
            assert!(
                Instant::now() < deadline,
                "output producer flush returned late"
            );
            assert!(flushed.is_ok(), "output producer flush refused");
        }
        Some("live-v1") => {
            let deadline = entered + Duration::from_secs(10);
            while Instant::now() < deadline {
                thread::sleep(
                    Duration::from_millis(25)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        }
        // BEGIN genuine stderr companions; old target arms stay literal.
        Some("oversized-stderr-v1") => {
            let deadline = entered + Duration::from_secs(30);
            let payload = vec![b'x'; 65_537];
            assert!(
                Instant::now() < deadline,
                "stderr producer expired before lock"
            );
            let mut stderr = io::stderr().lock();
            assert!(
                Instant::now() < deadline,
                "stderr producer expired before write"
            );
            let written = stderr.write_all(&payload);
            assert!(
                Instant::now() < deadline,
                "stderr producer write returned late"
            );
            assert!(written.is_ok(), "stderr producer write refused");
            let flushed = stderr.flush();
            assert!(
                Instant::now() < deadline,
                "stderr producer flush returned late"
            );
            assert!(flushed.is_ok(), "stderr producer flush refused");
        }
        Some("live-stderr-v1") => {
            let deadline = entered + Duration::from_secs(10);
            assert!(
                Instant::now() < deadline,
                "live stderr producer expired before lock"
            );
            let mut stderr = io::stderr().lock();
            assert!(
                Instant::now() < deadline,
                "live stderr producer expired before write"
            );
            let written = stderr.write_all(b"live\n");
            assert!(
                Instant::now() < deadline,
                "live stderr producer write returned late"
            );
            assert!(written.is_ok(), "live stderr producer write refused");
            let flushed = stderr.flush();
            assert!(
                Instant::now() < deadline,
                "live stderr producer flush returned late"
            );
            assert!(flushed.is_ok(), "live stderr producer flush refused");
            while Instant::now() < deadline {
                thread::sleep(
                    Duration::from_millis(25)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        }
        // END genuine stderr companions.
        _ => panic!("invalid fixed output producer role"),
    }
}

#[test]
fn native_cancel_target() {
    let Some(role) = std::env::var_os("WINDOWS_CLI_CANCEL_ROLE") else {
        return;
    };
    assert_eq!(role, "native-cancel-v1", "invalid native target role");
    let entered = Instant::now();
    let deadline = entered + Duration::from_secs(30);
    let result = (|| -> Result<(), Code> {
        let control = Control::new(entered, deadline);
        control.check(deadline)?;
        let path =
            PathBuf::from(std::env::var_os("WINDOWS_CLI_CANCEL_PROGRESS").ok_or(Code::Progress)?);
        let parent = path.parent().ok_or(Code::Progress)?;
        let guarded = DirectoryGuard::existing_private(parent);
        control.check(deadline)?;
        let guard = guarded.map_err(|_| Code::Progress)?;
        control.check(deadline)?;
        let opened = create_private_new(&path);
        control.check(deadline)?;
        let mut writer = opened.map_err(|_| Code::Progress)?;
        for counter in 0_u64..1201 {
            if Instant::now() >= deadline {
                break;
            }
            control.check(deadline)?;
            let written = writer.write_all(format!("{counter:016x}\n").as_bytes());
            control.check(deadline)?;
            written.map_err(|_| Code::Progress)?;
            control.check(deadline)?;
            let flushed = writer.flush();
            control.check(deadline)?;
            flushed.map_err(|_| Code::Progress)?;
            thread::sleep(
                Duration::from_millis(25).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
        drop(writer);
        drop(guard);
        Ok(())
    })();
    assert!(result.is_ok(), "native target refused: {:?}", result.err());
}

#[derive(Clone, Copy)]
enum PeerMode {
    Foreign,
    Malformed,
    Idle,
    Withheld,
    PublicationProof,
}

impl PeerMode {
    fn name(self) -> &'static str {
        match self {
            Self::Foreign => "foreign",
            Self::Malformed => "malformed",
            Self::Idle => "idle",
            Self::Withheld => "withheld",
            Self::PublicationProof => "publication-proof",
        }
    }
}

#[test]
fn native_wake_peer_target() {
    let Some(role) = std::env::var_os("WINDOWS_CLI_WAKE_PEER_ROLE") else {
        return;
    };
    let mode = match role.to_str() {
        Some("foreign") => PeerMode::Foreign,
        Some("malformed") => PeerMode::Malformed,
        Some("idle") => PeerMode::Idle,
        Some("withheld") => PeerMode::Withheld,
        Some("publication-proof") => PeerMode::PublicationProof,
        _ => panic!("invalid native peer role"),
    };
    let entered = Instant::now();
    let control = Control::new(entered, entered + Duration::from_secs(30));
    let result = peer_target(mode, &control);
    assert!(result.is_ok(), "native peer refused: {:?}", result.err());
}

// Every operation stays on the actual auxiliary child, inside its original
// horizon. A synchronous SDK call may block; these gates do not preempt it.
fn peer_operation<T>(control: &Control, operation: impl FnOnce() -> T) -> Result<T, Code> {
    control.check(control.deadline)?;
    let result = operation();
    control.check(control.deadline)?;
    Ok(result)
}

fn close_peer_stage(
    stage: GuardedFile,
    control: &Control,
) -> Result<(tempfile::TempPath, DirectoryGuard), Code> {
    let path = stage.normalized_path().to_path_buf();
    let (file, parent) = stage.into_parts();
    peer_operation(control, || drop(file))?;
    let wrapped = peer_operation(control, || {
        tempfile::TempPath::try_from_path(path).map(|mut stage| {
            // This is only the known CreateNew stage. Pure Drop must not add
            // native deletion after expiry; the existing reaped fixture owns it.
            stage.disable_cleanup(true);
            stage
        })
    })?;
    Ok((wrapped.map_err(|_| Code::Native)?, parent))
}

fn persist_peer_stage(
    stage: tempfile::TempPath,
    destination: &Path,
    control: &Control,
) -> Result<Result<(), tempfile::PathPersistError>, Code> {
    peer_operation(control, || stage.persist_noclobber(destination))
}

fn peer_file_bytes(path: &Path, limit: u64, control: &Control) -> Result<Vec<u8>, Code> {
    let mut file =
        peer_operation(control, || open_read_no_follow(path))?.map_err(|_| Code::Native)?;
    let mut bytes = Vec::new();
    peer_operation(control, || (&mut *file).take(limit).read_to_end(&mut bytes))?
        .map_err(|_| Code::Native)?;
    peer_operation(control, || drop(file))?;
    Ok(bytes)
}

fn peer_final_absent(path: &Path, control: &Control) -> Result<(), Code> {
    let result = peer_operation(control, || open_read_no_follow(path))?;
    if result
        .err()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
    {
        Ok(())
    } else {
        Err(Code::Native)
    }
}

fn peer_publication_proof(root: &Path, control: &Control) -> Result<(), Code> {
    let proof = peer_operation(control, || {
        create_private_new(&root.join("peer-share-proof"))
    })?
    .map_err(|_| Code::Native)?;
    let proof_path = proof.normalized_path().to_path_buf();
    let (file, proof_parent) = proof.into_parts();
    peer_operation(control, || drop(file))?;
    if !peer_file_bytes(&proof_path, 2, control)?.is_empty() {
        return Err(Code::Native);
    }
    // Exact FileSystemRights.FullControl (including DELETE), share READ|WRITE.
    // The parent guards remain live; this is an existing, known private leaf.
    let held = peer_operation(control, || {
        std::fs::OpenOptions::new()
            .access_mode(0x001f_01ff)
            .share_mode(3)
            .custom_flags(0x0020_0000)
            .open(&proof_path)
    })?
    .map_err(|_| Code::Native)?;
    let metadata = peer_operation(control, || held.metadata())?.map_err(|_| Code::Native)?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(Code::Native);
    }
    let refused = peer_operation(control, || open_read_no_follow(&proof_path))?;
    if refused.err().and_then(|error| error.raw_os_error()) != Some(32) {
        return Err(Code::Native);
    }
    peer_operation(control, || drop(held))?;
    if !peer_file_bytes(&proof_path, 2, control)?.is_empty() {
        return Err(Code::Native);
    }
    peer_operation(control, || drop(proof_parent))?;

    let mut collision =
        peer_operation(control, || create_private_new(&root.join("peer-collision")))?
            .map_err(|_| Code::Native)?;
    peer_operation(control, || collision.write_all(b"collision"))?.map_err(|_| Code::Native)?;
    peer_operation(control, || collision.flush())?.map_err(|_| Code::Native)?;
    let collision_path = collision.normalized_path().to_path_buf();
    let original_identity =
        peer_operation(control, || file_identity(&collision))?.map_err(|_| Code::Native)?;
    let private = peer_operation(control, || is_private(&collision_path, false))?
        .map_err(|_| Code::Native)?;
    if !private {
        return Err(Code::Native);
    }
    let (file, collision_parent) = collision.into_parts();
    // No destination leaf handle may mask a replacing implementation.
    peer_operation(control, || drop(file))?;
    let mut candidate = peer_operation(control, || {
        create_private_new(&root.join("peer-collision-pending"))
    })?
    .map_err(|_| Code::Native)?;
    peer_operation(control, || candidate.write_all(b"candidate"))?.map_err(|_| Code::Native)?;
    peer_operation(control, || candidate.flush())?.map_err(|_| Code::Native)?;
    let (candidate, candidate_parent) = close_peer_stage(candidate, control)?;
    let Err(refusal) = persist_peer_stage(candidate, &collision_path, control)? else {
        return Err(Code::Native);
    };
    let reopened = peer_operation(control, || open_read_no_follow(&collision_path))?
        .map_err(|_| Code::Native)?;
    let unchanged_identity =
        peer_operation(control, || file_identity(&reopened))?.map_err(|_| Code::Native)?;
    let private = peer_operation(control, || is_private(&collision_path, false))?
        .map_err(|_| Code::Native)?;
    if unchanged_identity != original_identity || !private {
        return Err(Code::Native);
    }
    peer_operation(control, || drop(reopened))?;
    if peer_file_bytes(&collision_path, 10, control)? != b"collision"
        || peer_file_bytes(refusal.path.as_ref(), 10, control)? != b"candidate"
    {
        return Err(Code::Native);
    }
    // The failed disabled-cleanup stage owner is retained through all readback.
    peer_operation(control, || drop(refusal))?;
    peer_operation(control, || drop((candidate_parent, collision_parent)))?;
    Ok(())
}

fn peer_target(mode: PeerMode, control: &Control) -> Result<(), Code> {
    let deadline = control.deadline;
    control.check(deadline)?;
    let root = PathBuf::from(std::env::var_os("WINDOWS_CLI_WAKE_PEER_ROOT").ok_or(Code::Native)?);
    let guarded = DirectoryGuard::existing_private(&root);
    control.check(deadline)?;
    let guard = guarded.map_err(|_| Code::Native)?;
    control.check(deadline)?;
    let named = endpoint_name_guarded(&guard, "wake", None);
    control.check(deadline)?;
    let endpoint = named.map_err(|_| Code::Native)?;
    control.check(deadline)?;
    let built = Builder::new_current_thread().enable_all().build();
    control.check(deadline)?;
    let runtime = built.map_err(|_| Code::Native)?;
    runtime.block_on(async {
        control.check(deadline)?;
        // Deliberately untrusted negative input. A private directory does not
        // confer pipe privacy; only the real daemon qualifies positive readiness.
        let created = ServerOptions::new()
            .pipe_mode(PipeMode::Byte)
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&endpoint);
        control.check(deadline)?;
        let mut server = created.map_err(|_| Code::Native)?;
        control.check(deadline)?;
        if matches!(mode, PeerMode::PublicationProof) {
            peer_publication_proof(&root, control)?;
        }
        let final_ready = root.join("peer-ready");
        let created = create_private_new(&root.join("peer-ready-pending"));
        control.check(deadline)?;
        let mut ready = created.map_err(|_| Code::Native)?;
        if matches!(mode, PeerMode::PublicationProof) {
            peer_final_absent(&final_ready, control)?;
        }
        control.check(deadline)?;
        let written = ready.write_all(b"1");
        control.check(deadline)?;
        written.map_err(|_| Code::Native)?;
        control.check(deadline)?;
        let flushed = ready.flush();
        control.check(deadline)?;
        flushed.map_err(|_| Code::Native)?;
        let (stage, ready_parent) = close_peer_stage(ready, control)?;
        if matches!(mode, PeerMode::PublicationProof) {
            peer_final_absent(&final_ready, control)?;
        }
        persist_peer_stage(stage, &final_ready, control)?.map_err(|_| Code::Native)?;
        if matches!(mode, PeerMode::PublicationProof)
            && peer_file_bytes(&final_ready, 2, control)? != b"1"
        {
            return Err(Code::Native);
        }
        peer_operation(control, || drop(ready_parent))?;
        gated(control, deadline, server.connect()).await?;
        match mode {
            PeerMode::Malformed | PeerMode::PublicationProof => {
                let mut frame = [0_u8; 16];
                gated(control, deadline, server.read_exact(&mut frame)).await?;
                if frame[0] != 15 || &frame[1..] != WAKE_MESSAGE {
                    return Err(Code::Run);
                }
                gated(control, deadline, server.write_all(b"locron-ack/v0\n")).await?;
            }
            PeerMode::Foreign | PeerMode::Idle | PeerMode::Withheld => {}
        }
        // Remain a real live peer until the exact owner kills/reaps this child.
        gated(control, deadline, async {
            tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
            Ok(())
        })
        .await
    })
}

fn setup_peer(owner: &mut Owner, mode: PeerMode) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.control.step(Phase::Setup, owner.control.deadline)?;
    owner.control.observations.intent(Operation::CaseCurrentExe);
    let executable = std::env::current_exe();
    owner
        .control
        .observations
        .io(Operation::CaseCurrentExe, ChildRole::NoChild, &executable);
    owner.control.check(owner.control.deadline)?;
    let executable = executable.map_err(|_| Code::Native)?;
    owner.spawn_child(
        0,
        Command::new(executable)
            .args(["--exact", PEER_SELECTOR, "--nocapture", "--test-threads=1"])
            .env("WINDOWS_CLI_WAKE_PEER_ROLE", mode.name())
            .env("WINDOWS_CLI_WAKE_PEER_ROOT", owner.root()?)
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        None,
        ChildRole::NegativePeer,
    )?;
    let ready = owner.root()?.join("peer-ready");
    loop {
        owner.live(0, owner.control.deadline)?;
        owner.control.check(owner.control.deadline)?;
        owner.control.observations.intent(Operation::ReadyOpen);
        let file = open_read_no_follow(&ready);
        owner
            .control
            .observations
            .io(Operation::ReadyOpen, ChildRole::NegativePeer, &file);
        owner.control.check(owner.control.deadline)?;
        match file {
            Ok(mut file) => {
                let mut byte = Vec::new();
                owner.control.check(owner.control.deadline)?;
                owner.control.observations.intent(Operation::ReadyRead);
                let read = (&mut *file).take(2).read_to_end(&mut byte);
                owner
                    .control
                    .observations
                    .io(Operation::ReadyRead, ChildRole::NegativePeer, &read);
                if read.is_ok() {
                    owner.control.observations.ready(&byte);
                }
                owner.control.check(owner.control.deadline)?;
                read.map_err(|_| Code::Native)?;
                // Empty is not published. It follows the original NotFound
                // continuation with the same child/liveness/outer clock.
                if !byte.is_empty() {
                    if byte != b"1" {
                        return Err(Code::Native);
                    }
                    break;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(Code::Native),
        }
        pause(
            &owner.control,
            owner.control.deadline,
            Duration::from_millis(5),
        )?;
    }
    Ok(())
}

fn refused_peer(mode: PeerMode, expected: Code) {
    let mut driver = admit(None).expect("resource-free peer admission failed");
    let deadline = driver.control.deadline;
    assert!(
        driver.dispatch(CaseKind::Peer(mode)).is_ok(),
        "peer command refused"
    );
    let refusal = driver.receive_until(deadline);
    assert_eq!(
        refusal.code, expected,
        "first peer refusal was incorrect: {refusal}"
    );
    assert_eq!(
        refusal.flags & (CLIENT | QUERIED),
        CLIENT | QUERIED,
        "refusal did not reach the real same-connection query: {refusal}"
    );
    if matches!(mode, PeerMode::Idle) {
        assert_ne!(driver.control.probe_expiry.load(Ordering::Acquire), 0);
        assert_eq!(driver.control.probe_complete.load(Ordering::Acquire), 0);
        assert!(!driver.control.admitted.load(Ordering::Acquire));
    }
    let result = if refusal.flags & CLEANED == 0 {
        driver.wait_cleanup_until(deadline)
    } else {
        refusal
    };
    driver.finish_if_returned();
    assert_eq!(
        result.code, expected,
        "negative native peer did not refuse as required: {result}"
    );
    assert_ne!(
        result.flags & CLEANED,
        0,
        "actual owned cleanup is unconfirmed: {result}"
    );
    assert_eq!(
        result.flags & ACK,
        0,
        "untrusted peer qualified an ACK: {result}"
    );
    if matches!(mode, PeerMode::Foreign) {
        assert_eq!(
            result.frame_bytes, 0,
            "foreign peer received a dispatched frame: {result}"
        );
    }
    assert!(
        Instant::now() < deadline,
        "negative peer exceeded original helper horizon"
    );
}

#[test]
fn foreign_wake_peer_is_refused_before_frame() {
    refused_peer(PeerMode::Foreign, Code::ForeignPeer);
}

#[test]
fn malformed_wake_ack_is_refused() {
    refused_peer(PeerMode::Malformed, Code::BadAck);
}

#[test]
fn idle_wake_peer_expires_original_probe_deadline() {
    refused_peer(PeerMode::Idle, Code::Expired);
}

#[test]
fn private_peer_publication_preserves_no_clobber_boundary() {
    refused_peer(PeerMode::PublicationProof, Code::BadAck);
}

struct ReturnGate {
    entered: SyncSender<HeldNotice>,
    release: Receiver<()>,
}

struct HeldNotice {
    deadline: Instant,
    root: PathBuf,
}

// Even assertion unwinding releases the controlled return; ownership stays on
// the existing worker rather than resetting/replacing a stalled context.
struct ReleaseGate(Option<SyncSender<()>>);

impl Drop for ReleaseGate {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.try_send(());
        }
    }
}

#[test]
fn withheld_native_return_retains_case_owner_at_deadline() {
    let (entered, notice) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let release = ReleaseGate(Some(release));
    let mut driver = admit(Some(ReturnGate {
        entered,
        release: released,
    }))
    .expect("resource-free controlled admission failed");
    let deadline = driver.control.deadline;
    assert!(
        driver.dispatch(CaseKind::Peer(PeerMode::Withheld)).is_ok(),
        "peer command refused"
    );
    let notice = notice
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .unwrap_or_else(|error| {
            panic!(
                "actual native query did not reach the controlled return gate: {error:?} {}",
                driver.control.observations.snapshot(),
            );
        });
    // The caller supplies only the original outer horizon. The driver must
    // independently discover and refuse the published 200 ms incomplete probe.
    let refused = driver.receive_until(deadline);
    assert!(
        Instant::now() >= notice.deadline,
        "driver did not observe probe expiry"
    );
    assert_eq!(driver.control.probe_complete.load(Ordering::Acquire), 0);
    assert!(!driver.control.admitted.load(Ordering::Acquire));
    assert_eq!(
        refused.code,
        Code::Expired,
        "driver accepted a withheld result: {refused}"
    );
    assert_eq!(
        refused.flags & (GUARD | CLIENT | QUERIED),
        GUARD | CLIENT | QUERIED
    );
    assert_eq!(refused.flags & REAPED, 0);
    assert_eq!(refused.frame_bytes, 0);
    assert!(Instant::now() < deadline, "original helper clock expired");
    // A real incompatible namespace operation proves the retained root guard;
    // the actual query/client/Child remain inside the owner, not a DTO authority.
    let renamed = std::fs::rename(&notice.root, notice.root.with_extension("displaced"));
    assert!(
        Instant::now() < deadline,
        "root probe returned after helper expiry"
    );
    assert_eq!(
        renamed.err().and_then(|error| error.raw_os_error()),
        Some(32)
    );
    drop(release);
    let returned = driver.wait_cleanup_until(deadline);
    assert_eq!(
        returned.code,
        Code::Expired,
        "late query result became success: {returned}"
    );
    assert_ne!(returned.flags & REAPED, 0);
    assert_ne!(returned.flags & CLEANED, 0);
    assert_eq!(returned.frame_bytes, 0);
    assert!(!returned.succeeded());
    assert!(
        Instant::now() < deadline,
        "cleanup exceeded original helper horizon"
    );
    driver.finish_if_returned();
}
