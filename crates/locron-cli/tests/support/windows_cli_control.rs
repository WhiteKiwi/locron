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
use std::io::{self, Read, Write};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
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

fn decode_signed(payload: u64) -> Option<Option<i32>> {
    let bits = u32::try_from(payload & RAW_MASK).ok()?;
    if payload & PRESENT == 0 {
        return (bits == 0).then_some(None);
    }
    Some(Some(i32::from_ne_bytes(bits.to_ne_bytes())))
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

    fn status(&self, operation: Operation, role: ChildRole, status: &std::process::ExitStatus) {
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

    fn work(&self, work: &Result<(), Code>, phase: u8) {
        self.intent(Operation::WorkOutcome);
        let code = match work {
            Ok(()) => Code::Success,
            Err(code) => *code,
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
                let Some(raw) = decode_signed(payload) else {
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
                let Some(code) = decode_signed(payload) else {
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
            (Some(c), Some(w), Some(r), Some(b), Some(e), Some(n))
                if c[0] == epoch && c[1] == 0 && c[2] == 14
                    && w[2] == epoch && w[4] == 2 && w[1] == 1 && w[0] == 0
                    && r[1] == epoch && r[2] == 0 && r[0] == u64::from(code.slot())
                    && b[1] == epoch && b[0] == bytes
                    && e[1] == epoch && n[1] == epoch && e[2] == n[2]
                    && b[2] == n[2] && n[3] == 58 && n[6] == 1)
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
                (Some(c), Some(w), Some(r), Some(e), Some(n))
                    if c[0] == 1 && c[1] == 0 && c[2] == 14
                        && w[2] == 1 && w[4] == 1 && w[1] == 0 && w[0] == 0
                        && r[1] == 1 && r[0] == 1 && r[2] == 0
                        && e[1] == 1 && n[1] == 1 && e[2] == n[2] && w[3] == n[2]
                        && n[3] == 59 && n[6] == 3)
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
}

impl CaseResult {
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
        {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
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
        let result = self.captured_output_body(phase, command, hold_duplicate, poll_span);
        self.call_context.returned(&self.control, &result);
        result
    }

    fn captured_output_body(
        &mut self,
        phase: Phase,
        command: &mut Command,
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
        let child = command
            .stdout(Stdio::from(duplicate))
            .stderr(Stdio::null())
            .spawn();
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
                        .status(wait, ChildRole::ControlCli, status)
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
                    .status(Operation::CleanupTryWait, role, status);
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
                        .status(Operation::CleanupWait, role, status);
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
        self.control.observations.work(&work, previous_phase);
        self.control
            .phase
            .store(Phase::Cleanup as u8, Ordering::Release);
        if self.reap().is_err() {
            self.uncertain_cleanup = true;
            // No result/field destruction while exact root exit is unknown.
            self.quarantine();
        }
        let cleanup = self.release();
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
        if self.state.is_some()
            || self.guard.is_some()
            || self.runtime.is_some()
            || self.client.is_some()
            || self.metadata.is_some()
            || self.query_handle.is_some()
            || self.capture_reader.is_some()
            || self.capture_duplicate.is_some()
            || !self.captures.is_empty()
        {
            if self.release().is_err() {
                self.quarantine();
            }
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
                .status(Operation::ChildLiveness, role, status);
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
enum CaseKind {
    Wake,
    Cancel,
    Peer(PeerMode),
    Capture(CaptureCase),
}

fn admit(hook: Option<ReturnGate>) -> Result<CaseAdmission, CaseResult> {
    // The only origin/outer horizon is born before even empty OS admission.
    let entered = Instant::now();
    let control = Control::new(entered, entered + Duration::from_secs(30));
    admit_control(control, hook)
}

fn admit_control(
    control: Arc<Control>,
    hook: Option<ReturnGate>,
) -> Result<CaseAdmission, CaseResult> {
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
                        owner.daemon(0, Some(Duration::from_secs(5)))?;
                        wake_work(&mut owner)
                    }
                    CaseKind::Cancel => {
                        owner.daemon(0, Some(Duration::from_secs(8)))?;
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
                }
            }))
            .unwrap_or(Err(Code::Panicked));
            // Owner stays outside the caught setup/work closure on this one worker.
            let completed = owner.complete(work);
            let _ = sender.try_send(completed);
        });
    let thread = match admitted {
        Ok(thread) => thread,
        Err(_) => return Err(control.snapshot(control.refuse(Code::Native))),
    };
    Ok(CaseAdmission {
        control,
        command: Some(command),
        result,
        thread: Some(thread),
    })
}

fn drive(kind: CaseKind) -> CaseResult {
    let mut driver = match admit(None) {
        Ok(driver) => driver,
        Err(result) => return result,
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

/// Tests secured actual-daemon delivery and manual admission within original5s.
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
    let run_id = owner.submit("wake")?;
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
pub fn run_cancel_case() -> CaseResult {
    drive(CaseKind::Cancel)
}

fn cancel_work(owner: &mut Owner) -> Result<(), Code> {
    owner.retain_guard()?;
    owner.live(0, owner.control.deadline)?;
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
    let run_id = owner.submit("cancel")?;
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
    owner.output(Phase::Cancel, owner.command()?.args(["cancel", &run_id]))?;
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
    for record in bytes.chunks_exact(17) {
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
        let actual_code = decode_signed(payload).flatten();
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
}

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
