//! Command-local, debug-only closed wake observations. This is not an authority.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io::{self, Write};
use std::sync::Mutex;
use std::time::Instant;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::{EnvFilter, FilterExt, filter_fn};
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

const TARGET: &str = "locron::windows_wake_diagnostics";

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum Role {
    Daemon,
    Run,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Run => "run",
        }
    }
}

pub(crate) fn init(verbose: u8, debug: bool, intended: Option<Role>) {
    let level = if debug {
        "trace"
    } else if verbose > 1 {
        "debug"
    } else if verbose > 0 {
        "info"
    } else {
        "warn"
    };
    // A per-layer filter keeps raw attempt input out of fmt even without opt-in.
    // It does not veto the separately filtered private layer.
    let ordinary = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(EnvFilter::new(level).and(filter_fn(|metadata| metadata.target() != TARGET)));
    let private = activation(intended).map(|(role, context)| {
        WakeLayer {
            state: Mutex::new(State {
                role,
                context,
                entered: Instant::now(),
                sequence: 0,
                tick: 0,
                keys: [None; 64],
                count: 0,
                fused: false,
            }),
        }
        .with_filter(filter_fn(|metadata| metadata.target() == TARGET))
    });
    let _ = tracing_subscriber::registry()
        .with(ordinary)
        .with(private)
        .try_init();
}

fn activation(intended: Option<Role>) -> Option<(Role, [u8; 16])> {
    let intended = intended?;
    if std::env::var("LOCRON_WINDOWS_DIAGNOSTIC_VERSION")
        .ok()?
        .as_str()
        != "2"
        || std::env::var("LOCRON_WINDOWS_DIAGNOSTIC_ROLE")
            .ok()?
            .as_str()
            != intended.label()
    {
        return None;
    }
    let text = std::env::var("LOCRON_WINDOWS_DIAGNOSTIC_CONTEXT").ok()?;
    Some((intended, decode_hex::<16>(&text)?))
}

fn decode_hex<const N: usize>(text: &str) -> Option<[u8; N]> {
    if text.len() != N * 2 {
        return None;
    }
    let mut bytes = [0; N];
    for (destination, pair) in bytes
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
    Some(bytes)
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

pub(crate) fn emit(op: &'static str, edge: &'static str, value: Option<u64>) {
    tracing::trace!(target: TARGET, op, edge, value_present = value.is_some(),
        value = value.unwrap_or(0), kind = "none", raw_present = false, raw = 0_i64);
}

pub(crate) fn hint(error: Option<&io::Error>) {
    let raw = error.and_then(io::Error::raw_os_error);
    tracing::trace!(target: TARGET, op = "hint", edge = if error.is_some() { "err" } else { "ok" },
        value_present = false, value = 0_u64,
        kind = error.map_or("none", |error| io_kind(error.kind())),
        raw_present = raw.is_some(), raw = i64::from(raw.unwrap_or(0)));
}

fn io_kind(kind: io::ErrorKind) -> &'static str {
    use io::ErrorKind;
    match kind {
        ErrorKind::NotFound => "NotFound",
        ErrorKind::PermissionDenied => "PermissionDenied",
        ErrorKind::ConnectionRefused => "ConnectionRefused",
        ErrorKind::ConnectionReset => "ConnectionReset",
        ErrorKind::HostUnreachable => "HostUnreachable",
        ErrorKind::NetworkUnreachable => "NetworkUnreachable",
        ErrorKind::ConnectionAborted => "ConnectionAborted",
        ErrorKind::NotConnected => "NotConnected",
        ErrorKind::AddrInUse => "AddrInUse",
        ErrorKind::AddrNotAvailable => "AddrNotAvailable",
        ErrorKind::NetworkDown => "NetworkDown",
        ErrorKind::BrokenPipe => "BrokenPipe",
        ErrorKind::AlreadyExists => "AlreadyExists",
        ErrorKind::WouldBlock => "WouldBlock",
        ErrorKind::NotADirectory => "NotADirectory",
        ErrorKind::IsADirectory => "IsADirectory",
        ErrorKind::DirectoryNotEmpty => "DirectoryNotEmpty",
        ErrorKind::ReadOnlyFilesystem => "ReadOnlyFilesystem",
        ErrorKind::StaleNetworkFileHandle => "StaleNetworkFileHandle",
        ErrorKind::InvalidInput => "InvalidInput",
        ErrorKind::InvalidData => "InvalidData",
        ErrorKind::TimedOut => "TimedOut",
        ErrorKind::WriteZero => "WriteZero",
        ErrorKind::StorageFull => "StorageFull",
        ErrorKind::NotSeekable => "NotSeekable",
        ErrorKind::QuotaExceeded => "QuotaExceeded",
        ErrorKind::FileTooLarge => "FileTooLarge",
        ErrorKind::ResourceBusy => "ResourceBusy",
        ErrorKind::ExecutableFileBusy => "ExecutableFileBusy",
        ErrorKind::Deadlock => "Deadlock",
        ErrorKind::CrossesDevices => "CrossesDevices",
        ErrorKind::TooManyLinks => "TooManyLinks",
        ErrorKind::InvalidFilename => "InvalidFilename",
        ErrorKind::ArgumentListTooLong => "ArgumentListTooLong",
        ErrorKind::Interrupted => "Interrupted",
        ErrorKind::Unsupported => "Unsupported",
        ErrorKind::UnexpectedEof => "UnexpectedEof",
        ErrorKind::OutOfMemory => "OutOfMemory",
        ErrorKind::Other => "Other",
        _ => "unknown",
    }
}

#[derive(Default)]
struct Fields {
    seen: u16,
    refused: bool,
    op: Option<&'static str>,
    edge: Option<&'static str>,
    value_present: bool,
    value: u64,
    kind: Option<&'static str>,
    raw_present: bool,
    raw: i32,
    uuid: Option<[u8; 16]>,
    ordinal: u32,
}

const OPS: [&str; 21] = [
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
];
const EDGES: [&str; 9] = [
    "enter", "ok", "err", "some", "none", "wake", "timer", "external", "signal",
];

impl Fields {
    fn field(&mut self, field: &Field, allowed: &[&str]) -> Option<usize> {
        let index = [
            "op",
            "edge",
            "value_present",
            "value",
            "kind",
            "raw_present",
            "raw",
            "run_uuid",
            "ordinal",
        ]
        .iter()
        .position(|name| *name == field.name());
        let Some(index) = index.filter(|_| allowed.contains(&field.name())) else {
            self.refused = true;
            return None;
        };
        let bit = 1 << index;
        if self.seen & bit != 0 {
            self.refused = true;
            return None;
        }
        self.seen |= bit;
        Some(index)
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match self.field(field, &["op", "edge", "kind", "run_uuid"]) {
            Some(0) => self.op = OPS.iter().copied().find(|op| *op == value),
            Some(1) => self.edge = EDGES.iter().copied().find(|edge| *edge == value),
            Some(4) => {
                self.kind = Some(match value {
                    "none" => "none",
                    "unknown" => "unknown",
                    _ => {
                        // Match only the same closed labels that the safe sender can emit.
                        let kinds = [
                            io::ErrorKind::NotFound,
                            io::ErrorKind::PermissionDenied,
                            io::ErrorKind::ConnectionRefused,
                            io::ErrorKind::ConnectionReset,
                            io::ErrorKind::HostUnreachable,
                            io::ErrorKind::NetworkUnreachable,
                            io::ErrorKind::ConnectionAborted,
                            io::ErrorKind::NotConnected,
                            io::ErrorKind::AddrInUse,
                            io::ErrorKind::AddrNotAvailable,
                            io::ErrorKind::NetworkDown,
                            io::ErrorKind::BrokenPipe,
                            io::ErrorKind::AlreadyExists,
                            io::ErrorKind::WouldBlock,
                            io::ErrorKind::NotADirectory,
                            io::ErrorKind::IsADirectory,
                            io::ErrorKind::DirectoryNotEmpty,
                            io::ErrorKind::ReadOnlyFilesystem,
                            io::ErrorKind::StaleNetworkFileHandle,
                            io::ErrorKind::InvalidInput,
                            io::ErrorKind::InvalidData,
                            io::ErrorKind::TimedOut,
                            io::ErrorKind::WriteZero,
                            io::ErrorKind::StorageFull,
                            io::ErrorKind::NotSeekable,
                            io::ErrorKind::QuotaExceeded,
                            io::ErrorKind::FileTooLarge,
                            io::ErrorKind::ResourceBusy,
                            io::ErrorKind::ExecutableFileBusy,
                            io::ErrorKind::Deadlock,
                            io::ErrorKind::CrossesDevices,
                            io::ErrorKind::TooManyLinks,
                            io::ErrorKind::InvalidFilename,
                            io::ErrorKind::ArgumentListTooLong,
                            io::ErrorKind::Interrupted,
                            io::ErrorKind::Unsupported,
                            io::ErrorKind::UnexpectedEof,
                            io::ErrorKind::OutOfMemory,
                            io::ErrorKind::Other,
                        ];
                        let Some(kind) = kinds.into_iter().map(io_kind).find(|kind| *kind == value)
                        else {
                            self.refused = true;
                            return;
                        };
                        kind
                    }
                })
            }
            Some(7) => {
                self.uuid = uuid::Uuid::parse_str(value)
                    .ok()
                    .filter(|uuid| {
                        !uuid.is_nil()
                            && value.len() == 36
                            && uuid.hyphenated().to_string() == value
                    })
                    .map(uuid::Uuid::into_bytes);
                if self.uuid.is_none() {
                    self.refused = true;
                }
            }
            _ => {}
        }
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        match self.field(field, &["value", "ordinal"]) {
            Some(3) => self.value = value,
            Some(8) => match u32::try_from(value).ok().filter(|ordinal| *ordinal != 0) {
                Some(ordinal) => self.ordinal = ordinal,
                None => self.refused = true,
            },
            _ => {}
        }
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        if self.field(field, &["raw"]).is_some() {
            match i32::try_from(value) {
                Ok(raw) => self.raw = raw,
                Err(_) => self.refused = true,
            }
        }
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        match self.field(field, &["value_present", "raw_present"]) {
            Some(2) => self.value_present = value,
            Some(5) => self.raw_present = value,
            _ => {}
        }
    }
    fn record_f64(&mut self, _: &Field, _: f64) {
        self.refused = true;
    }
    fn record_i128(&mut self, _: &Field, _: i128) {
        self.refused = true;
    }
    fn record_u128(&mut self, _: &Field, _: u128) {
        self.refused = true;
    }
    fn record_bytes(&mut self, _: &Field, _: &[u8]) {
        self.refused = true;
    }
    fn record_error(&mut self, _: &Field, _: &(dyn std::error::Error + 'static)) {
        self.refused = true;
    }
    fn record_debug(&mut self, _: &Field, _: &dyn std::fmt::Debug) {
        self.refused = true;
    }
}

struct WakeLayer {
    state: Mutex<State>,
}

impl<S: Subscriber> Layer<S> for WakeLayer {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.fused {
            return;
        }
        if !state.emit(&fields) {
            state.fused = true;
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct Key {
    uuid: [u8; 16],
    ordinal: u32,
}

struct State {
    role: Role,
    context: [u8; 16],
    entered: Instant,
    sequence: u16,
    tick: u8,
    keys: [Option<Key>; 64],
    count: u8,
    fused: bool,
}

impl State {
    fn emit(&mut self, fields: &Fields) -> bool {
        let (Some(op), Some(edge), Some(kind)) = (fields.op, fields.edge, fields.kind) else {
            return false;
        };
        let attempt_op = OPS[14..].contains(&op);
        let expected = if attempt_op { 511 } else { 127 };
        if fields.refused
            || fields.seen != expected
            || (!fields.value_present && fields.value != 0)
            || (!fields.raw_present && fields.raw != 0)
            || ((op != "hint" || edge != "err") && (kind != "none" || fields.raw_present))
            || (self.role == Role::Run && !["enqueue", "hint", "run_return"].contains(&op))
            || (self.role == Role::Daemon && ["enqueue", "hint", "run_return"].contains(&op))
        {
            return false;
        }
        let value = fields.value_present.then_some(fields.value);
        if !shape(op, edge, value) {
            return false;
        }
        if self.sequence >= 255 {
            return self.limit();
        }
        let epoch = if attempt_op {
            let Some(uuid) = fields.uuid else {
                return false;
            };
            let key = Key {
                uuid,
                ordinal: fields.ordinal,
            };
            let existing = self.keys.iter().position(|entry| *entry == Some(key));
            if op == "attempt_begin" {
                if existing.is_some() {
                    return false;
                }
                if self.count == 64 || self.sequence >= 254 {
                    return self.limit();
                }
                self.keys[usize::from(self.count)] = Some(key);
                self.count += 1;
                let mut hash = Sha256::new();
                hash.update(b"locron-wake-attempt/v1\0");
                hash.update(self.context);
                hash.update(uuid);
                hash.update(fields.ordinal.to_be_bytes());
                let digest = hash.finalize();
                self.sequence += 1;
                let row = format!(
                    "locron_wake_bind/v1 ctx={} role=daemon seq={} attempt={} ordinal={} corr={} t_us={}\n",
                    hex(&self.context),
                    self.sequence,
                    self.count,
                    fields.ordinal,
                    hex(&digest),
                    self.time()
                );
                if row.len() > 203 || io::stderr().lock().write_all(row.as_bytes()).is_err() {
                    return false;
                }
                u16::from(self.count)
            } else {
                let Some(index) = existing else {
                    return false;
                };
                let Ok(epoch) = u16::try_from(index + 1) else {
                    return false;
                };
                epoch
            }
        } else {
            0
        };
        if op == "tick" && edge == "enter" {
            let Some(tick) = self.tick.checked_add(1) else {
                return self.limit();
            };
            self.tick = tick;
        }
        self.sequence += 1;
        let row = format!(
            "locron_wake_phase/v2 ctx={} role={} seq={} tick={} attempt={} op={op} edge={edge} value={} kind={kind} raw={} t_us={}\n",
            hex(&self.context),
            self.role.label(),
            self.sequence,
            if attempt_op { 0 } else { self.tick },
            epoch,
            option_unsigned(value),
            option_signed(fields.raw_present.then_some(fields.raw)),
            self.time()
        );
        row.len() <= 241 && io::stderr().lock().write_all(row.as_bytes()).is_ok()
    }
    fn time(&self) -> u64 {
        u64::try_from(self.entered.elapsed().as_micros()).unwrap_or(u64::MAX)
    }
    fn limit(&mut self) -> bool {
        self.fused = true;
        if self.sequence < 256 {
            self.sequence += 1;
            let row = format!(
                "locron_wake_phase/v2 ctx={} role={} seq={} tick=0 attempt=0 op=limit edge=err value=None kind=none raw=None t_us={}\n",
                hex(&self.context),
                self.role.label(),
                self.sequence,
                self.time()
            );
            let _ = io::stderr().lock().write_all(row.as_bytes());
        }
        true
    }
}

fn option_unsigned(value: Option<u64>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| format!("Some({value})"))
}
fn option_signed(value: Option<i32>) -> String {
    value.map_or_else(|| "None".to_owned(), |value| format!("Some({value})"))
}

fn shape(op: &str, edge: &str, value: Option<u64>) -> bool {
    match op {
        "attempt_begin" => edge == "enter" && value.is_none(),
        "cancel_signal" => edge == "ok" && value.is_none(),
        "mark_running" | "cancel_poll" => match edge {
            "enter" | "err" => value.is_none(),
            "ok" => matches!(value, Some(0 | 1)),
            _ => false,
        },
        "execution" => match edge {
            "enter" | "err" => value.is_none(),
            "ok" => value.is_some_and(|value| (1..=6).contains(&value)),
            _ => false,
        },
        "completion" | "failure_complete" => match edge {
            "enter" if op == "failure_complete" => matches!(value, Some(1 | 2)),
            "enter" | "ok" => value.is_none(),
            "err" => matches!(value, Some(1 | 2)),
            _ => false,
        },
        "capacity" => edge == "ok" && value.is_some(),
        "reconcile" | "admit" if edge == "ok" => value.is_some(),
        "delay" => match edge {
            "enter" => value.is_none(),
            "some" | "none" | "err" => value.is_some_and(|value| value <= 30_000_000),
            _ => false,
        },
        "wait" => match edge {
            "enter" => value.is_some_and(|value| value <= 30_000_000),
            "wake" | "timer" | "external" | "signal" => value.is_none(),
            _ => false,
        },
        "run_return" => matches!(edge, "ok" | "err") && value.is_none(),
        _ => matches!(edge, "enter" | "ok" | "err") && value.is_none(),
    }
}
