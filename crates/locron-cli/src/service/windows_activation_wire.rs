//! Strict untrusted activation data shared with the internal GUI entry.
//!
//! Decoding any value here cannot authenticate a process or authorize a Scheduler effect.
//! Live transport owners supply kernel peer/child/lease evidence and their original deadlines.

use std::fmt;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const FRAME_LIMIT: usize = 1024;
const HEADER_LEN: usize = 4;
const BODY_LIMIT: usize = FRAME_LIMIT - HEADER_LEN;
const MAX_REMAINING_MICROS: u32 = 30_000_000;

/// Fixed errors intentionally omit input, serde messages and raw capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireError {
    Frame,
    Length,
    Utf8,
    Version,
    Uuid,
    Digest,
    Capability,
    Role,
    Pid,
    Budget,
    Expired,
    Step,
    RunArgument,
}

impl fmt::Display for WireError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Frame => "invalid activation frame schema",
            Self::Length => "invalid activation frame length",
            Self::Utf8 => "invalid activation frame UTF-8",
            Self::Version => "invalid activation protocol version",
            Self::Uuid => "invalid activation UUID",
            Self::Digest => "invalid activation digest",
            Self::Capability => "invalid activation capability",
            Self::Role => "invalid activation role",
            Self::Pid => "invalid activation process ID",
            Self::Budget => "invalid activation remaining budget",
            Self::Expired => "activation original deadline elapsed",
            Self::Step => "unexpected activation frame step",
            Self::RunArgument => "invalid internal activation argument",
        })
    }
}

impl std::error::Error for WireError {}

/// The wire protocol version, separate from the private maintenance record version.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    V1,
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            1 => Ok(Self::V1),
            _ => Err(serde::de::Error::custom(WireError::Version)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Daemon,
    Dashboard,
}

impl Role {
    pub fn parse(value: &str) -> Result<Self, WireError> {
        match value {
            "daemon" => Ok(Self::Daemon),
            "dashboard" => Ok(Self::Dashboard),
            _ => Err(WireError::Role),
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Dashboard => "dashboard",
        }
    }
}

/// Syntax only: a canonical nonnil value does not establish a held lifetime or Run instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalUuid(String);

impl CanonicalUuid {
    pub fn parse(value: &str) -> Result<Self, WireError> {
        let bytes = value.as_bytes();
        if bytes.len() != 36
            || bytes.iter().enumerate().any(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    *byte != b'-'
                } else {
                    !is_lower_hex(*byte)
                }
            })
            || bytes.iter().all(|byte| matches!(byte, b'0' | b'-'))
        {
            return Err(WireError::Uuid);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CanonicalUuid {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CanonicalUuid {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Digest(String);

impl Digest {
    pub fn parse(value: &str) -> Result<Self, WireError> {
        if !valid_hex(value, 64) {
            return Err(WireError::Digest);
        }
        Ok(Self(value.to_owned()))
    }

    #[must_use]
    pub fn from_bytes(value: [u8; 32]) -> Self {
        Self(lower_hex(&value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

/// Live secret data. Only the fixed Run argument/private frame encoders render its bytes.
#[derive(Clone, Eq, PartialEq)]
pub struct Capability([u8; 32]);

impl Capability {
    #[must_use]
    pub const fn from_bytes(value: [u8; 32]) -> Self {
        Self(value)
    }

    pub fn parse(value: &str) -> Result<Self, WireError> {
        if !valid_hex(value, 64) {
            return Err(WireError::Capability);
        }
        let mut bytes = [0; 32];
        for (output, pair) in bytes.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
            *output = hex_digit(pair[0]) * 16 + hex_digit(pair[1]);
        }
        Ok(Self(bytes))
    }

    /// Borrow only inside the owned live digest calculation, never a diagnostic formatter.
    #[must_use]
    pub const fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for Capability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Capability([REDACTED])")
    }
}

impl Serialize for Capability {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&lower_hex(&self.0))
    }
}

impl<'de> Deserialize<'de> for Capability {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(is_lower_hex)
}

const fn is_lower_hex(value: u8) -> bool {
    matches!(value, b'0'..=b'9' | b'a'..=b'f')
}

const fn hex_digit(value: u8) -> u8 {
    if value <= b'9' {
        value - b'0'
    } else {
        value - b'a' + 10
    }
}

fn lower_hex(value: &[u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in value {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    text
}

/// A claimed nonzero PID; a live kernel observation must independently match it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessId(NonZeroU32);

impl ProcessId {
    pub fn new(value: u32) -> Result<Self, WireError> {
        NonZeroU32::new(value).map(Self).ok_or(WireError::Pid)
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl Serialize for ProcessId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.get())
    }
}

impl<'de> Deserialize<'de> for ProcessId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A shortening duration, never permission to restart a phase at receipt time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemainingBudget(u32);

impl RemainingBudget {
    pub fn new(micros: u32) -> Result<Self, WireError> {
        if !(1..=MAX_REMAINING_MICROS).contains(&micros) {
            return Err(WireError::Budget);
        }
        Ok(Self(micros))
    }

    pub fn floor_until(deadline: Instant, observed: Instant) -> Result<Self, WireError> {
        let remaining = deadline
            .checked_duration_since(observed)
            .filter(|remaining| !remaining.is_zero())
            .ok_or(WireError::Expired)?;
        let micros = u32::try_from(remaining.as_micros()).map_err(|_| WireError::Budget)?;
        Self::new(micros)
    }

    #[must_use]
    pub const fn micros(self) -> u32 {
        self.0
    }

    /// Anchor at the captured pre-SID entry, not the later Permit/Bootstrap receipt.
    pub fn shorten(
        self,
        entry: Instant,
        original_deadline: Instant,
        observed: Instant,
    ) -> Result<Instant, WireError> {
        let encoded = entry
            .checked_add(Duration::from_micros(u64::from(self.0)))
            .ok_or(WireError::Budget)?;
        let shortened = encoded.min(original_deadline);
        if shortened <= observed {
            return Err(WireError::Expired);
        }
        Ok(shortened)
    }
}

impl Serialize for RemainingBudget {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.micros())
    }
}

impl<'de> Deserialize<'de> for RemainingBudget {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LauncherHello {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub capability: Capability,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Permit {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub digest: Digest,
    pub scheduler_instance: CanonicalUuid,
    pub producer_pid: ProcessId,
    pub remaining_micros: RemainingBudget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Child {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub digest: Digest,
    pub scheduler_instance: CanonicalUuid,
    pub child_pid: ProcessId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorHello {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub capability: Capability,
    pub digest: Digest,
    pub scheduler_instance: CanonicalUuid,
    pub launcher_pid: ProcessId,
    pub supervisor_lifetime: CanonicalUuid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub digest: Digest,
    pub scheduler_instance: CanonicalUuid,
    pub launcher_pid: ProcessId,
    pub supervisor_pid: ProcessId,
    pub supervisor_lifetime: CanonicalUuid,
}

/// Private stdin expectations only. No decoded bootstrap is an authentication witness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bootstrap {
    pub version: Version,
    pub context: CanonicalUuid,
    pub role: Role,
    pub capability: Capability,
    pub digest: Digest,
    pub scheduler_instance: CanonicalUuid,
    pub producer_pid: ProcessId,
    pub launcher_pid: ProcessId,
    pub remaining_micros: RemainingBudget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Frame {
    LauncherHello(LauncherHello),
    Permit(Permit),
    Child(Child),
    SupervisorHello(SupervisorHello),
    Seal(Seal),
    Bootstrap(Bootstrap),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    LauncherHello,
    Permit,
    Child,
    SupervisorHello,
    Seal,
    Bootstrap,
}

impl Frame {
    #[must_use]
    pub const fn kind(&self) -> FrameKind {
        match self {
            Self::LauncherHello(_) => FrameKind::LauncherHello,
            Self::Permit(_) => FrameKind::Permit,
            Self::Child(_) => FrameKind::Child,
            Self::SupervisorHello(_) => FrameKind::SupervisorHello,
            Self::Seal(_) => FrameKind::Seal,
            Self::Bootstrap(_) => FrameKind::Bootstrap,
        }
    }
}

/// Reject an untrusted body length before the transport allocates or reads that body.
pub fn decode_frame_length(header: [u8; HEADER_LEN]) -> Result<usize, WireError> {
    let length = usize::try_from(u32::from_le_bytes(header)).map_err(|_| WireError::Length)?;
    if !(1..=BODY_LIMIT).contains(&length) {
        return Err(WireError::Length);
    }
    Ok(length)
}

pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, WireError> {
    let body = serde_json::to_vec(frame).map_err(|_| WireError::Frame)?;
    let length = u32::try_from(body.len()).map_err(|_| WireError::Length)?;
    decode_frame_length(length.to_le_bytes())?;
    let mut encoded = Vec::with_capacity(HEADER_LEN + body.len());
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(&body);
    Ok(encoded)
}

pub fn decode_frame(encoded: &[u8]) -> Result<Frame, WireError> {
    let header = encoded
        .get(..HEADER_LEN)
        .ok_or(WireError::Length)?
        .try_into()
        .map_err(|_| WireError::Length)?;
    let length = decode_frame_length(header)?;
    if encoded.len() != HEADER_LEN + length {
        return Err(WireError::Length);
    }
    let body = std::str::from_utf8(&encoded[HEADER_LEN..]).map_err(|_| WireError::Utf8)?;
    serde_json::from_str(body).map_err(|_| WireError::Frame)
}

/// Even a valid frame refuses when it belongs to another protocol step.
pub fn decode_frame_for(encoded: &[u8], step: FrameKind) -> Result<Frame, WireError> {
    let frame = decode_frame(encoded)?;
    if frame.kind() != step {
        return Err(WireError::Step);
    }
    Ok(frame)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunCapability {
    pub context: CanonicalUuid,
    pub capability: Capability,
}

impl RunCapability {
    /// Live Scheduler data only; never log this String or serialize it into durable facts.
    #[must_use]
    pub fn encode_argument(&self) -> String {
        format!(
            "v1:{}:{}",
            self.context.as_str(),
            lower_hex(self.capability.bytes())
        )
    }
}

pub fn parse_run_argument(value: Option<&str>) -> Result<Option<RunCapability>, WireError> {
    match value {
        None | Some("" | "v1:" | "v1:$(Arg0)") => Ok(None),
        Some(value) => {
            if value.len() != 104 {
                return Err(WireError::RunArgument);
            }
            let (context, capability) = value
                .strip_prefix("v1:")
                .and_then(|value| value.split_once(':'))
                .ok_or(WireError::RunArgument)?;
            let context = CanonicalUuid::parse(context).map_err(|_| WireError::RunArgument)?;
            let capability = Capability::parse(capability).map_err(|_| WireError::RunArgument)?;
            Ok(Some(RunCapability {
                context,
                capability,
            }))
        }
    }
}

/// The caller supplies the shared identity of its retained verified root, never a raw nonce.
#[must_use]
pub fn endpoint_name(instance: &Digest, role: Role, context: &CanonicalUuid) -> String {
    format!(
        "locron.activation.v1.{}.{}.{}",
        instance.as_str(),
        role.as_str(),
        context.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BODY_LIMIT, Bootstrap, CanonicalUuid, Capability, Child, Digest, FRAME_LIMIT, Frame,
        FrameKind, LauncherHello, MAX_REMAINING_MICROS, Permit, ProcessId, RemainingBudget, Role,
        RunCapability, Seal, SupervisorHello, Version, WireError, decode_frame, decode_frame_for,
        decode_frame_length, encode_frame, endpoint_name, parse_run_argument,
    };
    use serde_json::{Value, json};
    use std::time::{Duration, Instant};

    const UUID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";
    const SECRET: &str = "b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7b7";

    fn maxima() -> Vec<Frame> {
        let context = CanonicalUuid::parse(UUID).unwrap();
        let digest = Digest::from_bytes([255; 32]);
        let capability = Capability::parse(SECRET).unwrap();
        let pid = ProcessId::new(u32::MAX).unwrap();
        let budget = RemainingBudget::new(MAX_REMAINING_MICROS).unwrap();
        let role = Role::Dashboard;
        vec![
            Frame::LauncherHello(LauncherHello {
                version: Version::V1,
                context: context.clone(),
                role,
                capability: capability.clone(),
            }),
            Frame::Permit(Permit {
                version: Version::V1,
                context: context.clone(),
                role,
                digest: digest.clone(),
                scheduler_instance: context.clone(),
                producer_pid: pid,
                remaining_micros: budget,
            }),
            Frame::Child(Child {
                version: Version::V1,
                context: context.clone(),
                role,
                digest: digest.clone(),
                scheduler_instance: context.clone(),
                child_pid: pid,
            }),
            Frame::SupervisorHello(SupervisorHello {
                version: Version::V1,
                context: context.clone(),
                role,
                capability: capability.clone(),
                digest: digest.clone(),
                scheduler_instance: context.clone(),
                launcher_pid: pid,
                supervisor_lifetime: context.clone(),
            }),
            Frame::Seal(Seal {
                version: Version::V1,
                context: context.clone(),
                role,
                digest: digest.clone(),
                scheduler_instance: context.clone(),
                launcher_pid: pid,
                supervisor_pid: pid,
                supervisor_lifetime: context.clone(),
            }),
            Frame::Bootstrap(Bootstrap {
                version: Version::V1,
                context: context.clone(),
                role,
                capability,
                digest,
                scheduler_instance: context,
                producer_pid: pid,
                launcher_pid: pid,
                remaining_micros: budget,
            }),
        ]
    }

    fn untrusted(body: &[u8]) -> Vec<u8> {
        let mut frame = u32::try_from(body.len()).unwrap().to_le_bytes().to_vec();
        frame.extend_from_slice(body);
        frame
    }

    fn changed(frame: &Frame, change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value = serde_json::to_value(frame).unwrap();
        change(&mut value);
        untrusted(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn every_maximal_complete_frame_round_trips_below_the_fixed_cap() {
        for frame in maxima() {
            let encoded = encode_frame(&frame).unwrap();
            assert!(encoded.len() <= FRAME_LIMIT);
            assert_eq!(decode_frame(&encoded).unwrap(), frame);
            assert_eq!(decode_frame_for(&encoded, frame.kind()).unwrap(), frame);
            assert_eq!(
                decode_frame_length(encoded[..4].try_into().unwrap()).unwrap(),
                encoded.len() - 4
            );
            let wrong_step = if frame.kind() == FrameKind::Permit {
                FrameKind::Seal
            } else {
                FrameKind::Permit
            };
            assert_eq!(
                decode_frame_for(&encoded, wrong_step).unwrap_err(),
                WireError::Step
            );
        }
        assert_eq!(Role::parse("daemon").unwrap().as_str(), "daemon");
        assert_eq!(Role::parse("dashboard").unwrap().as_str(), "dashboard");
        assert_eq!(Role::parse("Daemon").unwrap_err(), WireError::Role);
    }

    #[test]
    fn untrusted_lengths_utf8_truncation_and_trailing_data_refuse() {
        for length in [0, 1021, u32::MAX] {
            assert_eq!(
                decode_frame_length(length.to_le_bytes()).unwrap_err(),
                WireError::Length
            );
            assert_eq!(
                decode_frame(&length.to_le_bytes()).unwrap_err(),
                WireError::Length
            );
        }
        assert_eq!(decode_frame(&[]).unwrap_err(), WireError::Length);
        assert_eq!(
            decode_frame(&untrusted(&[255])).unwrap_err(),
            WireError::Utf8
        );
        let mut valid = encode_frame(&maxima()[0]).unwrap();
        assert_eq!(
            decode_frame(&valid[..valid.len() - 1]).unwrap_err(),
            WireError::Length
        );
        valid.push(b'!');
        assert_eq!(decode_frame(&valid).unwrap_err(), WireError::Length);
        let body = [b' '; BODY_LIMIT];
        assert_eq!(
            decode_frame(&untrusted(&body)).unwrap_err(),
            WireError::Frame
        );
        assert_eq!(
            decode_frame_length(1020_u32.to_le_bytes()).unwrap(),
            BODY_LIMIT
        );
    }

    #[test]
    fn unknown_duplicate_missing_and_wrong_type_fields_refuse_without_rendering_input() {
        for frame in maxima() {
            let object = serde_json::to_value(&frame).unwrap();
            let body = serde_json::to_string(&frame).unwrap();
            for (key, value) in object.as_object().unwrap() {
                let missing = changed(&frame, |value| {
                    value.as_object_mut().unwrap().remove(key);
                });
                assert_eq!(decode_frame(&missing).unwrap_err(), WireError::Frame);
                let duplicate = format!(
                    "{{{}:{},{}",
                    serde_json::to_string(key).unwrap(),
                    value,
                    &body[1..]
                );
                assert_eq!(
                    decode_frame(&untrusted(duplicate.as_bytes())).unwrap_err(),
                    WireError::Frame
                );
            }
            let unknown = changed(&frame, |value| {
                value["extra"] = json!(SECRET);
            });
            assert_eq!(decode_frame(&unknown).unwrap_err(), WireError::Frame);
            for value in [json!(0), json!(2), json!("1"), json!(1.5), json!(null)] {
                assert_eq!(
                    decode_frame(&changed(&frame, |body| {
                        body["version"] = value;
                    }))
                    .unwrap_err(),
                    WireError::Frame
                );
            }
            let invalid_type = changed(&frame, |value| {
                value["type"] = json!("manual_admin");
            });
            assert_eq!(decode_frame(&invalid_type).unwrap_err(), WireError::Frame);
            let error = decode_frame(&unknown).unwrap_err();
            assert!(!format!("{error:?}: {error}").contains(SECRET));
            assert!(!format!("{frame:?}").contains(SECRET));
            for key in ["digest", "capability"] {
                if object.get(key).is_some() {
                    let malformed = changed(&frame, |value| {
                        value[key] = json!(format!("SECRET_SENTINEL_{SECRET}"));
                    });
                    let error = decode_frame(&malformed).unwrap_err();
                    assert_eq!(error, WireError::Frame);
                    assert!(!format!("{error:?}: {error}").contains("SECRET_SENTINEL"));
                    assert!(!format!("{error:?}: {error}").contains(SECRET));
                }
            }
        }
        assert_eq!(
            format!("{:?}", Capability::from_bytes([183; 32])),
            "Capability([REDACTED])"
        );
    }

    #[test]
    fn canonical_uuid_hex_role_pid_and_budget_boundaries_are_strict() {
        let permit = maxima().remove(1);
        for uuid in [
            "00000000-0000-0000-0000-000000000000",
            "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF",
            "ffffffffffffffffffffffffffffffff",
            "ffffffff-ffff-ffff-ffff-fffffffffffg",
        ] {
            assert_eq!(CanonicalUuid::parse(uuid).unwrap_err(), WireError::Uuid);
            for key in ["context", "scheduler_instance"] {
                assert_eq!(
                    decode_frame(&changed(&permit, |value| {
                        value[key] = json!(uuid);
                    }))
                    .unwrap_err(),
                    WireError::Frame
                );
            }
        }
        assert_eq!(CanonicalUuid::parse(UUID).unwrap().as_str(), UUID);
        for digest in ["", "fff", "F".repeat(64).as_str(), "g".repeat(64).as_str()] {
            assert_eq!(Digest::parse(digest).unwrap_err(), WireError::Digest);
            assert_eq!(
                Capability::parse(digest).unwrap_err(),
                WireError::Capability
            );
        }
        assert_eq!(
            Digest::parse(&"f".repeat(64)).unwrap().as_str(),
            "f".repeat(64)
        );
        assert_eq!(Capability::parse(SECRET).unwrap().bytes(), &[183; 32]);
        for value in [json!(0), json!(-1), json!(u64::MAX), json!(1.5), json!("1")] {
            assert_eq!(
                decode_frame(&changed(&permit, |body| {
                    body["producer_pid"] = value;
                }))
                .unwrap_err(),
                WireError::Frame
            );
        }
        assert_eq!(ProcessId::new(0).unwrap_err(), WireError::Pid);
        assert_eq!(ProcessId::new(1).unwrap().get(), 1);
        for value in [
            json!(0),
            json!(30_000_001),
            json!(u64::MAX),
            json!(1.5),
            json!("1"),
        ] {
            assert_eq!(
                decode_frame(&changed(&permit, |body| {
                    body["remaining_micros"] = value;
                }))
                .unwrap_err(),
                WireError::Frame
            );
        }
        for role in ["Daemon", "activation", "", "manual"] {
            assert_eq!(
                decode_frame(&changed(&permit, |value| {
                    value["role"] = json!(role);
                }))
                .unwrap_err(),
                WireError::Frame
            );
        }
    }

    #[test]
    fn run_parser_preserves_unwitnessed_markers_and_redacts_failures() {
        for value in [None, Some(""), Some("v1:"), Some("v1:$(Arg0)")] {
            assert_eq!(parse_run_argument(value).unwrap(), None);
        }
        let expected = RunCapability {
            context: CanonicalUuid::parse(UUID).unwrap(),
            capability: Capability::parse(SECRET).unwrap(),
        };
        assert_eq!(
            parse_run_argument(Some(&expected.encode_argument())).unwrap(),
            Some(expected.clone())
        );
        assert!(!format!("{expected:?}").contains(SECRET));
        for value in ["v1", "v1: ", " v1:", "v1:$(arg0)", "V1:$(Arg0)"] {
            assert_eq!(
                parse_run_argument(Some(value)).unwrap_err(),
                WireError::RunArgument
            );
        }
        for value in [
            format!("v1:{UUID}:{}", SECRET.to_ascii_uppercase()),
            format!("v1:{UUID}:{SECRET}:extra"),
            format!("v1:00000000-0000-0000-0000-000000000000:{SECRET}"),
        ] {
            let error = parse_run_argument(Some(&value)).unwrap_err();
            assert_eq!(error, WireError::RunArgument);
            assert!(!format!("{error:?}: {error}").contains(SECRET));
        }
    }

    #[test]
    fn floor_and_entry_anchor_never_extend_the_original_budget() {
        let entry = Instant::now();
        let original = entry.checked_add(Duration::from_secs(30)).unwrap();
        let observed = entry.checked_add(Duration::from_secs(2)).unwrap();
        let budget = RemainingBudget::new(3_000_000).unwrap();
        let shortened = budget.shorten(entry, original, observed).unwrap();
        assert_eq!(
            shortened,
            entry.checked_add(Duration::from_secs(3)).unwrap()
        );
        assert!(shortened < observed.checked_add(Duration::from_secs(3)).unwrap());
        let shorter_original = entry.checked_add(Duration::from_millis(2500)).unwrap();
        assert_eq!(
            budget.shorten(entry, shorter_original, observed).unwrap(),
            shorter_original
        );
        assert_eq!(
            budget.shorten(entry, original, shortened).unwrap_err(),
            WireError::Expired
        );
        let nanos = entry.checked_add(Duration::from_nanos(1_234_567)).unwrap();
        assert_eq!(
            RemainingBudget::floor_until(nanos, entry).unwrap().micros(),
            1234
        );
        assert_eq!(
            RemainingBudget::floor_until(entry, entry).unwrap_err(),
            WireError::Expired
        );
        assert_eq!(
            RemainingBudget::floor_until(
                entry.checked_add(Duration::from_nanos(999)).unwrap(),
                entry
            )
            .unwrap_err(),
            WireError::Budget
        );
        assert_eq!(
            RemainingBudget::floor_until(
                entry.checked_add(Duration::from_secs(31)).unwrap(),
                entry
            )
            .unwrap_err(),
            WireError::Budget
        );
        assert_eq!(RemainingBudget::new(0).unwrap_err(), WireError::Budget);
        assert_eq!(
            RemainingBudget::new(30_000_001).unwrap_err(),
            WireError::Budget
        );
    }

    #[test]
    fn actual_instant_overflow_refuses_instead_of_resetting_the_clock() {
        let now = Instant::now();
        let (mut low, mut high) = (0_u64, u64::MAX);
        while low < high {
            let middle = low + (high - low).div_ceil(2);
            if now.checked_add(Duration::from_secs(middle)).is_some() {
                low = middle;
            } else {
                high = middle - 1;
            }
        }
        let near_limit = now.checked_add(Duration::from_secs(low)).unwrap();
        assert_eq!(
            RemainingBudget::new(MAX_REMAINING_MICROS)
                .unwrap()
                .shorten(near_limit, near_limit, now)
                .unwrap_err(),
            WireError::Budget
        );
    }

    #[test]
    fn endpoint_names_separate_full_root_role_and_context_without_a_nonce() {
        let instance = Digest::from_bytes([255; 32]);
        let context = CanonicalUuid::parse(UUID).unwrap();
        let endpoint = endpoint_name(&instance, Role::Daemon, &context);
        assert!(endpoint.starts_with("locron.activation.v1."));
        assert!(!endpoint.contains(SECRET));
        assert_ne!(
            endpoint,
            endpoint_name(&instance, Role::Dashboard, &context)
        );
        let mut changed_identity = [255; 32];
        changed_identity[31] ^= 1;
        assert_ne!(
            endpoint,
            endpoint_name(
                &Digest::from_bytes(changed_identity),
                Role::Daemon,
                &context
            )
        );
        let changed_context = CanonicalUuid::parse("fffffffe-ffff-ffff-ffff-ffffffffffff").unwrap();
        assert_ne!(
            endpoint,
            endpoint_name(&instance, Role::Daemon, &changed_context)
        );
    }
}
