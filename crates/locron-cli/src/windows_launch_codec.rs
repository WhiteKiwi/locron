//! Bounded metadata codec for read-only helper launch qualification.
//!
//! A valid transcript is not a live launch proof or operation authority. The
//! launch owner must separately prove exact Child/guard overlap and terminal
//! closure under its original deadline. This module performs no transport I/O.

use anyhow::{Result, ensure};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use super::windows_receipt::{required_nullable, valid_hash};

const SCHEMA: &str = "locron.windows-helper-launch/v1";
pub(super) const FRAME_BYTES: usize = 4096;
const FRAMES: usize = 4;
const TOTAL_BYTES: usize = FRAME_BYTES * FRAMES;
const PREFIX_BYTES: usize = 4;
const DEADLINE_MS: u64 = 30_000;

fn canonical_uuid<'de, D>(deserializer: D) -> std::result::Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    let spelling = String::deserialize(deserializer)?;
    let id = Uuid::parse_str(&spelling).map_err(serde::de::Error::custom)?;
    if id.is_nil() || spelling != id.hyphenated().to_string() {
        return Err(serde::de::Error::custom("noncanonical or zero launch UUID"));
    }
    Ok(id)
}

fn nullable_uuid<'de, D>(deserializer: D) -> std::result::Result<Option<Uuid>, D::Error>
where
    D: Deserializer<'de>,
{
    let Some(spelling) = Option::<String>::deserialize(deserializer)? else {
        return Ok(None);
    };
    let id = Uuid::parse_str(&spelling).map_err(serde::de::Error::custom)?;
    if id.is_nil() || spelling != id.hyphenated().to_string() {
        return Err(serde::de::Error::custom(
            "noncanonical or zero launch nonce",
        ));
    }
    Ok(Some(id))
}

fn fixed_hex(value: &str, digits: usize) -> bool {
    value.len() == digits
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Metadata only: constructors do not establish live file or image ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HelperIdentity {
    volume: String,
    file: String,
    sha256: String,
}

impl HelperIdentity {
    pub(super) fn new(volume: u64, file: u128, sha256: String) -> Result<Self> {
        let identity = Self {
            volume: format!("{volume:016x}"),
            file: format!("{file:032x}"),
            sha256,
        };
        identity.validate()?;
        Ok(identity)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            fixed_hex(&self.volume, 16) && fixed_hex(&self.file, 32) && valid_hash(&self.sha256),
            "invalid complete helper identity/digest"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Bindings {
    #[serde(deserialize_with = "canonical_uuid")]
    operation: Uuid,
    #[serde(deserialize_with = "canonical_uuid")]
    session: Uuid,
    original_request_sha256: String,
    current_request_sha256: String,
    helper: HelperIdentity,
}

impl Bindings {
    pub(super) fn new(
        operation: Uuid,
        session: Uuid,
        original_request_sha256: String,
        current_request_sha256: String,
        helper: HelperIdentity,
    ) -> Result<Self> {
        let bindings = Self {
            operation,
            session,
            original_request_sha256,
            current_request_sha256,
            helper,
        };
        bindings.validate()?;
        Ok(bindings)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.operation.is_nil()
                && !self.session.is_nil()
                && valid_hash(&self.original_request_sha256)
                && valid_hash(&self.current_request_sha256),
            "invalid launch request/session binding"
        );
        self.helper.validate()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Phase {
    Challenge,
    Ready,
    Permit,
    Qualified,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Frame {
    schema: String,
    phase: Phase,
    bindings: Bindings,
    parent_pid: u32,
    #[serde(deserialize_with = "required_nullable")]
    child_pid: Option<u32>,
    #[serde(deserialize_with = "nullable_uuid")]
    child_nonce: Option<Uuid>,
    #[serde(deserialize_with = "required_nullable")]
    remaining_ms: Option<u64>,
}

impl Frame {
    pub(super) fn challenge(
        bindings: Bindings,
        parent_pid: u32,
        remaining_ms: u64,
    ) -> Result<Self> {
        let frame = Self {
            schema: SCHEMA.into(),
            phase: Phase::Challenge,
            bindings,
            parent_pid,
            child_pid: None,
            child_nonce: None,
            remaining_ms: Some(remaining_ms),
        };
        frame.validate()?;
        Ok(frame)
    }

    pub(super) fn next(&self, phase: Phase, child_pid: u32, child_nonce: Uuid) -> Result<Self> {
        let frame = Self {
            schema: SCHEMA.into(),
            phase,
            bindings: self.bindings.clone(),
            parent_pid: self.parent_pid,
            child_pid: Some(child_pid),
            child_nonce: Some(child_nonce),
            remaining_ms: None,
        };
        frame.validate()?;
        Ok(frame)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.parent_pid != 0,
            "wrong launch schema or parent PID"
        );
        self.bindings.validate()?;
        match self.phase {
            Phase::Challenge => ensure!(
                self.child_pid.is_none()
                    && self.child_nonce.is_none()
                    && self
                        .remaining_ms
                        .is_some_and(|ms| (1..=DEADLINE_MS).contains(&ms)),
                "invalid Challenge shape/budget"
            ),
            Phase::Ready | Phase::Permit | Phase::Qualified => ensure!(
                self.child_pid.is_some_and(|pid| pid != 0)
                    && self.child_nonce.is_some_and(|id| !id.is_nil())
                    && self.remaining_ms.is_none(),
                "invalid child phase shape"
            ),
        }
        Ok(())
    }
}

/// This budget counts both locally sent and received frames; it does not time I/O.
#[derive(Default)]
pub(super) struct Codec {
    frames: usize,
    bytes: usize,
}

impl Codec {
    /// Inspect the untrusted prefix before allocating a bounded payload.
    pub(super) fn body_length(prefix: [u8; PREFIX_BYTES]) -> Result<usize> {
        let bytes = usize::try_from(u32::from_le_bytes(prefix))?;
        ensure!(
            (1..=FRAME_BYTES - PREFIX_BYTES).contains(&bytes),
            "launch frame length exceeds its wire bound"
        );
        Ok(bytes)
    }

    fn reserve(&mut self, bytes: usize) -> Result<()> {
        let total = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| anyhow::anyhow!("launch byte count overflow"))?;
        ensure!(
            self.frames < FRAMES && bytes <= FRAME_BYTES && total <= TOTAL_BYTES,
            "launch frame/count/total bound exceeded"
        );
        self.frames += 1;
        self.bytes = total;
        Ok(())
    }

    pub(super) fn encode(&mut self, frame: &Frame) -> Result<Vec<u8>> {
        frame.validate()?;
        let body = serde_json::to_vec(frame)?;
        let prefix = u32::try_from(body.len())?.to_le_bytes();
        Self::body_length(prefix)?;
        self.reserve(body.len() + PREFIX_BYTES)?;
        let mut wire = Vec::with_capacity(body.len() + PREFIX_BYTES);
        wire.extend(prefix);
        wire.extend(body);
        Ok(wire)
    }

    pub(super) fn decode(&mut self, prefix: [u8; PREFIX_BYTES], body: &[u8]) -> Result<Frame> {
        ensure!(
            Self::body_length(prefix)? == body.len(),
            "launch frame body differs from its declared length"
        );
        self.reserve(body.len() + PREFIX_BYTES)?;
        // from_slice requires one complete value and rejects trailing JSON.
        let frame: Frame = serde_json::from_slice(body)?;
        frame.validate()?;
        Ok(frame)
    }
}

/// Pure order/binding checks. Completion here is deliberately not qualification.
pub(super) struct Transcript {
    bindings: Bindings,
    parent_pid: u32,
    actual_child_pid: u32,
    child_nonce: Option<Uuid>,
    next: usize,
}

impl Transcript {
    pub(super) fn new(bindings: Bindings, parent_pid: u32, actual_child_pid: u32) -> Result<Self> {
        bindings.validate()?;
        ensure!(
            parent_pid != 0 && actual_child_pid != 0 && parent_pid != actual_child_pid,
            "invalid original parent/Child PID binding"
        );
        Ok(Self {
            bindings,
            parent_pid,
            actual_child_pid,
            child_nonce: None,
            next: 0,
        })
    }

    pub(super) fn observe(&mut self, frame: &Frame) -> Result<()> {
        frame.validate()?;
        let expected = [
            Phase::Challenge,
            Phase::Ready,
            Phase::Permit,
            Phase::Qualified,
        ]
        .get(self.next)
        .ok_or_else(|| anyhow::anyhow!("extra launch phase"))?;
        ensure!(
            frame.phase == *expected
                && frame.bindings == self.bindings
                && frame.parent_pid == self.parent_pid,
            "wrong launch phase or frozen binding"
        );
        if frame.phase != Phase::Challenge {
            ensure!(
                frame.child_pid == Some(self.actual_child_pid),
                "frame PID differs from original Child"
            );
            if frame.phase == Phase::Ready {
                self.child_nonce = frame.child_nonce;
            } else {
                ensure!(
                    frame.child_nonce == self.child_nonce,
                    "stale or changed child nonce"
                );
            }
        }
        self.next += 1;
        Ok(())
    }

    pub(super) fn finished(&self) -> bool {
        self.next == FRAMES
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn challenge() -> Frame {
        Frame::challenge(
            Bindings::new(
                Uuid::now_v7(),
                Uuid::now_v7(),
                "ab".repeat(32),
                "cd".repeat(32),
                HelperIdentity::new(u64::MAX, u128::MAX, "ef".repeat(32)).unwrap(),
            )
            .unwrap(),
            100,
            30_000,
        )
        .unwrap()
    }

    fn decode_value(value: &Value) -> Result<Frame> {
        let body = serde_json::to_vec(value)?;
        Codec::default().decode(u32::try_from(body.len())?.to_le_bytes(), &body)
    }

    #[test]
    fn all_four_frames_bind_full_id_and_actual_child_with_one_budget() {
        let challenge = challenge();
        let nonce = Uuid::now_v7();
        let mut codec = Codec::default();
        let mut transcript = Transcript::new(challenge.bindings.clone(), 100, 200).unwrap();
        for frame in [
            challenge,
            Frame::challenge(transcript.bindings.clone(), 100, 1)
                .unwrap()
                .next(Phase::Ready, 200, nonce)
                .unwrap(),
            Frame::challenge(transcript.bindings.clone(), 100, 1)
                .unwrap()
                .next(Phase::Permit, 200, nonce)
                .unwrap(),
            Frame::challenge(transcript.bindings.clone(), 100, 1)
                .unwrap()
                .next(Phase::Qualified, 200, nonce)
                .unwrap(),
        ] {
            let decoded = if matches!(frame.phase, Phase::Challenge | Phase::Permit) {
                let wire = codec.encode(&frame).unwrap();
                Codec::default()
                    .decode(wire[..4].try_into().unwrap(), &wire[4..])
                    .unwrap()
            } else {
                let wire = Codec::default().encode(&frame).unwrap();
                codec
                    .decode(wire[..4].try_into().unwrap(), &wire[4..])
                    .unwrap()
            };
            assert_eq!(decoded, frame);
            transcript.observe(&decoded).unwrap();
        }
        assert!(transcript.finished());
        assert_eq!(transcript.bindings.helper.volume, "ffffffffffffffff");
        assert_eq!(transcript.bindings.helper.file, "f".repeat(32));
        let extra = Frame::challenge(transcript.bindings.clone(), 100, 1).unwrap();
        assert!(codec.encode(&extra).is_err());
        assert!(transcript.observe(&extra).is_err());
    }

    #[test]
    fn malformed_prefix_shape_unknown_fields_and_trailing_json_refuse() {
        assert!(Codec::body_length(0u32.to_le_bytes()).is_err());
        assert!(Codec::body_length(u32::MAX.to_le_bytes()).is_err());
        assert!(Codec::body_length(4093u32.to_le_bytes()).is_err());
        assert_eq!(Codec::body_length(4092u32.to_le_bytes()).unwrap(), 4092);
        let original = serde_json::to_value(challenge()).unwrap();
        for (field, replacement) in [
            ("schema", json!("locron.windows-helper-launch/v2")),
            ("parent_pid", json!(0)),
            ("phase", json!("completed")),
            ("child_pid", json!(200)),
            ("child_nonce", json!(Uuid::now_v7())),
            ("remaining_ms", json!(0)),
            ("remaining_ms", json!(30_001)),
            ("effect", json!("replace")),
        ] {
            let mut value = original.clone();
            value[field] = replacement;
            assert!(decode_value(&value).is_err(), "accepted invalid {field}");
        }
        // Missing nullable keys are errors rather than defaulted hints.
        let mut missing = original.clone();
        missing.as_object_mut().unwrap().remove("child_pid");
        assert!(decode_value(&missing).is_err());
        let mut body = serde_json::to_vec(&original).unwrap();
        body.extend(b" {}");
        assert!(
            Codec::default()
                .decode(u32::try_from(body.len()).unwrap().to_le_bytes(), &body)
                .is_err()
        );
        assert!(Codec::default().decode(1u32.to_le_bytes(), b"{}").is_err());
        let mut duplicate = serde_json::to_vec(&original).unwrap();
        duplicate.pop();
        duplicate.extend(br#", "phase":"challenge"}"#);
        assert!(
            Codec::default()
                .decode(
                    u32::try_from(duplicate.len()).unwrap().to_le_bytes(),
                    &duplicate
                )
                .is_err()
        );
        let mut nested = original.clone();
        nested["bindings"]["helper"]["extra"] = json!(true);
        assert!(decode_value(&nested).is_err());
        nested = original.clone();
        nested["bindings"]["session"] = json!(Uuid::nil().to_string());
        assert!(decode_value(&nested).is_err());
    }

    #[test]
    fn prefix_is_included_in_four_frame_total_without_over_limit_allocation() {
        let challenge = challenge();
        let mut body = serde_json::to_vec(&challenge).unwrap();
        body.resize(FRAME_BYTES - PREFIX_BYTES, b' ');
        let prefix = u32::try_from(body.len()).unwrap().to_le_bytes();
        let mut codec = Codec::default();
        for _ in 0..FRAMES {
            assert_eq!(codec.decode(prefix, &body).unwrap(), challenge);
        }
        assert_eq!(codec.bytes, TOTAL_BYTES);
        assert!(codec.decode(prefix, &body).is_err());
    }

    #[test]
    fn stale_session_wrong_full_id_and_out_of_order_phase_refuse() {
        let challenge = challenge();
        let ready = challenge.next(Phase::Ready, 200, Uuid::now_v7()).unwrap();
        let original = serde_json::to_value(&ready).unwrap();
        let mut transcript = Transcript::new(challenge.bindings.clone(), 100, 200).unwrap();
        assert!(transcript.observe(&ready).is_err());
        transcript.observe(&challenge).unwrap();
        for (field, value) in [
            ("session", json!(Uuid::now_v7())),
            ("operation", json!(Uuid::now_v7())),
            ("original_request_sha256", json!("12".repeat(32))),
            ("current_request_sha256", json!("34".repeat(32))),
        ] {
            let mut changed = original.clone();
            changed["bindings"][field] = value;
            assert!(
                transcript
                    .observe(&decode_value(&changed).unwrap())
                    .is_err()
            );
        }
        for (field, value) in [
            ("volume", json!("7fffffffffffffff")),
            ("volume", json!("fffffffffffffffe")),
            ("file", json!("7fffffffffffffffffffffffffffffff")),
            ("file", json!("fffffffffffffffffffffffffffffffe")),
            ("sha256", json!("56".repeat(32))),
        ] {
            let mut changed = original.clone();
            changed["bindings"]["helper"][field] = value;
            assert!(
                transcript
                    .observe(&decode_value(&changed).unwrap())
                    .is_err()
            );
        }
        let wrong_pid = challenge.next(Phase::Ready, 201, Uuid::now_v7()).unwrap();
        assert!(transcript.observe(&wrong_pid).is_err());
        transcript.observe(&ready).unwrap();
        let stale = ready.next(Phase::Permit, 200, Uuid::now_v7()).unwrap();
        assert!(transcript.observe(&stale).is_err());
        let duplicate = ready
            .next(Phase::Ready, 200, ready.child_nonce.unwrap())
            .unwrap();
        assert!(transcript.observe(&duplicate).is_err());
    }

    #[test]
    fn canonical_uuid_complete_hex_and_ready_nullability_are_strict() {
        let challenge = challenge();
        let ready = challenge.next(Phase::Ready, 200, Uuid::now_v7()).unwrap();
        let original = serde_json::to_value(&ready).unwrap();
        for (field, replacement) in [
            ("child_pid", json!(null)),
            ("child_pid", json!(0)),
            ("child_pid", json!("200")),
            ("child_nonce", json!(null)),
            ("child_nonce", json!(Uuid::nil().to_string())),
            (
                "child_nonce",
                json!(ready.child_nonce.unwrap().simple().to_string()),
            ),
            ("remaining_ms", json!(1)),
        ] {
            let mut value = original.clone();
            value[field] = replacement;
            assert!(
                decode_value(&value).is_err(),
                "accepted invalid Ready {field}"
            );
        }
        for (field, replacement) in [
            ("volume", json!("ffffffffffffffff0")),
            ("file", json!("f".repeat(31))),
            ("file", json!(u128::MAX.to_string())),
            ("sha256", json!("EF".repeat(32))),
        ] {
            let mut value = original.clone();
            value["bindings"]["helper"][field] = replacement;
            assert!(
                decode_value(&value).is_err(),
                "accepted invalid helper {field}"
            );
        }
        let mut value = original;
        value["bindings"]["operation"] = json!(challenge.bindings.operation.simple().to_string());
        assert!(decode_value(&value).is_err());
    }
}
