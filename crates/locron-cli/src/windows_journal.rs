//! Bounded durable framing. Only a caller-selected validated Rust record can be decoded.
//!
//! The operation engine supplies its concrete ServiceRestoreRecord-containing
//! record and live identity checks; this codec never authorizes or replays effects.

use std::io::{self, Read, Seek, SeekFrom, Write};
use std::marker::PhantomData;
use std::path::Path;

use anyhow::{Result, ensure};
use locron_core::filesystem::{GuardedFile, create_private_new_exclusive, open_private_exclusive};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

const FRAME_LIMIT: usize = 128 * 1024;
const JOURNAL_LIMIT: usize = 16 * 1024 * 1024;
const FRAME_COUNT: usize = 128;
const HEADER: usize = 4 + 32;
const OVERHEAD: usize = HEADER + 32;

#[derive(Default)]
struct BoundedPayload(Vec<u8>);

impl Write for BoundedPayload {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > FRAME_LIMIT - OVERHEAD)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "journal payload exceeds its finite bound",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Chain<R> {
    records: Vec<R>,
    last_digest: [u8; 32],
    byte_len: usize,
}

/// A pure, typed reservation made before any journal/backup/staging creation.
/// The engine supplies a complete worst-case record and its total forward plus
/// rollback callback count. Encoding bounds include every repeated field.
pub(super) struct Reservation<R> {
    frames: usize,
    maximum_frame_bytes: usize,
    _record: PhantomData<fn() -> R>,
}

pub(super) fn preflight<R: Serialize>(
    worst_case_record: &R,
    frames: usize,
) -> Result<Reservation<R>> {
    preflight_with_growth(worst_case_record, frames, 0)
}

/// Checked growth for complete serde object slots, including fixed-key null slots.
/// The engine supplies the actual ServiceRestoreRecord and its validated provider
/// maximum. This size helper neither constructs nor authorizes a service record.
pub(super) fn object_growth<T: Serialize>(
    slot: Option<&T>,
    maximum_object_bytes: usize,
    repetitions: usize,
) -> Result<usize> {
    ensure!(
        repetitions > 0 && maximum_object_bytes <= FRAME_LIMIT - OVERHEAD,
        "invalid complete object-slot reservation"
    );
    let baseline = if let Some(object) = slot {
        let mut bounded = BoundedPayload::default();
        serde_json::to_writer(&mut bounded, object)?;
        ensure!(
            bounded.0.first() == Some(&b'{') && bounded.0.last() == Some(&b'}'),
            "future object slots must contain serde objects, not escaped strings"
        );
        bounded.0.len()
    } else {
        // The concrete outer schema always emits the optional key and null.
        // Field names, punctuation and all other future outer values belong in
        // the already bounded outer representation, not in this slot delta.
        4
    };
    let growth = maximum_object_bytes
        .checked_sub(baseline)
        .and_then(|growth| growth.checked_mul(repetitions))
        .ok_or_else(|| anyhow::anyhow!("unproven/overflowing complete object-slot growth"))?;
    Ok(growth)
}

/// Every repeated slot and future outer-field maximum must already be proven by
/// the concrete typed caller. Additional growth covers its complete remaining
/// forward and rollback path; actual appends retain the same strict reservation.
pub(super) fn preflight_with_growth<R: Serialize>(
    worst_case_record: &R,
    frames: usize,
    additional_payload_bytes: usize,
) -> Result<Reservation<R>> {
    let (bytes, _) = frame(worst_case_record, [0; 32])?;
    let maximum_frame_bytes = bytes
        .len()
        .checked_add(additional_payload_bytes)
        .ok_or_else(|| anyhow::anyhow!("complete outer journal representation overflows"))?;
    check_budget(0, 0, frames, maximum_frame_bytes)?;
    Ok(Reservation {
        frames,
        maximum_frame_bytes,
        _record: PhantomData,
    })
}

fn check_budget(
    existing_frames: usize,
    existing_bytes: usize,
    frames: usize,
    maximum_frame_bytes: usize,
) -> Result<()> {
    ensure!(
        frames > 0 && (OVERHEAD + 1..=FRAME_LIMIT).contains(&maximum_frame_bytes),
        "invalid complete-operation reservation"
    );
    ensure!(
        existing_frames
            .checked_add(frames)
            .is_some_and(|count| count <= FRAME_COUNT),
        "complete operation exceeds the journal frame budget"
    );
    let size = frames
        .checked_mul(maximum_frame_bytes)
        .and_then(|size| existing_bytes.checked_add(size));
    ensure!(
        size.is_some_and(|size| size <= JOURNAL_LIMIT),
        "complete operation exceeds the journal byte budget"
    );
    Ok(())
}

impl<R> Default for Chain<R> {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            last_digest: [0; 32],
            byte_len: 0,
        }
    }
}

fn frame<R: Serialize>(record: &R, previous: [u8; 32]) -> Result<(Vec<u8>, [u8; 32])> {
    let mut bounded = BoundedPayload::default();
    serde_json::to_writer(&mut bounded, record)?;
    let payload = bounded.0;
    let length = payload
        .len()
        .checked_add(OVERHEAD)
        .ok_or_else(|| anyhow::anyhow!("journal frame size overflow"))?;
    ensure!(
        length <= FRAME_LIMIT,
        "journal frame exceeds its finite bound"
    );
    let mut bytes = Vec::with_capacity(length);
    bytes.extend_from_slice(&(u32::try_from(length)?).to_le_bytes());
    bytes.extend_from_slice(&previous);
    bytes.extend_from_slice(&payload);
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&digest);
    Ok((bytes, digest))
}

fn decode<R: DeserializeOwned>(
    bytes: &[u8],
    mut validate: impl FnMut(&R, Option<&R>) -> Result<()>,
) -> Result<Chain<R>> {
    ensure!(
        bytes.len() <= JOURNAL_LIMIT,
        "journal exceeds its finite byte bound"
    );
    let mut chain = Chain::default();
    let mut offset = 0;
    while offset < bytes.len() {
        ensure!(
            chain.records.len() < FRAME_COUNT,
            "journal exceeds its finite frame count"
        );
        let length = bytes
            .get(offset..offset + 4)
            .ok_or_else(|| anyhow::anyhow!("journal has a partial frame length"))?;
        let length = u32::from_le_bytes(length.try_into()?) as usize;
        ensure!(
            (OVERHEAD + 1..=FRAME_LIMIT).contains(&length),
            "invalid journal frame length"
        );
        let end = offset
            .checked_add(length)
            .ok_or_else(|| anyhow::anyhow!("journal frame offset overflow"))?;
        let bytes = bytes
            .get(offset..end)
            .ok_or_else(|| anyhow::anyhow!("journal has a partial final frame"))?;
        ensure!(
            bytes[4..HEADER] == chain.last_digest,
            "journal digest chain is broken"
        );
        let digest_start = length - 32;
        let digest: [u8; 32] = Sha256::digest(&bytes[..digest_start]).into();
        ensure!(
            bytes[digest_start..] == digest,
            "journal frame checksum differs"
        );
        let record: R = serde_json::from_slice(&bytes[HEADER..digest_start])?;
        validate(&record, chain.records.last())?;
        chain.records.push(record);
        chain.last_digest = digest;
        offset = end;
    }
    chain.byte_len = offset;
    Ok(chain)
}

/// Owns the exact private exclusive journal handle throughout one operation.
pub(super) struct Journal<R> {
    file: GuardedFile,
    chain: Chain<R>,
    poisoned: bool,
    reservation: Option<Reservation<R>>,
}

impl<R: Serialize + DeserializeOwned> Journal<R> {
    pub(super) fn create(path: &Path, reservation: Reservation<R>) -> Result<Self> {
        Ok(Self {
            file: create_private_new_exclusive(path)?,
            chain: Chain::default(),
            poisoned: false,
            reservation: Some(reservation),
        })
    }

    pub(super) fn open(
        path: &Path,
        validate: impl FnMut(&R, Option<&R>) -> Result<()>,
    ) -> Result<Self> {
        let mut file = open_private_exclusive(path)?;
        let size = file.metadata()?.len();
        ensure!(
            size > 0 && size <= JOURNAL_LIMIT as u64,
            "empty/oversized existing journal refuses recovery"
        );
        let mut bytes = Vec::new();
        Read::by_ref(&mut *file)
            .take(JOURNAL_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == size,
            "journal changed while it was read"
        );
        let chain = decode(&bytes, validate)?;
        Ok(Self {
            file,
            chain,
            poisoned: false,
            // A recovered operation must preflight its complete remaining path.
            reservation: None,
        })
    }

    pub(super) fn last_record(&self) -> Option<&R> {
        self.chain.records.last()
    }

    /// Reserve the complete forward and rollback write budget before the first effect.
    /// A record/binding set that cannot fit refuses before disabling a task.
    pub(super) fn reserve(&mut self, reservation: Reservation<R>) -> Result<()> {
        ensure!(!self.poisoned, "journal write outcome is uncertain");
        check_budget(
            self.chain.records.len(),
            self.chain.byte_len,
            reservation.frames,
            reservation.maximum_frame_bytes,
        )?;
        self.reservation = Some(reservation);
        Ok(())
    }

    pub(super) fn append(
        &mut self,
        record: R,
        mut validate: impl FnMut(&R, Option<&R>) -> Result<()>,
    ) -> Result<()> {
        ensure!(!self.poisoned, "journal write outcome is uncertain");
        validate(&record, self.chain.records.last())?;
        let (bytes, digest) = frame(&record, self.chain.last_digest)?;
        let reservation = self
            .reservation
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("complete remaining operation is not reserved"))?;
        ensure!(
            reservation.frames > 0 && bytes.len() <= reservation.maximum_frame_bytes,
            "journal append exceeds its preflight reservation"
        );
        let remaining_frames = reservation.frames - 1;
        let maximum_frame_bytes = reservation.maximum_frame_bytes;
        // No external object may be adopted as a journal merely by its filename.
        // The retained share0 handle prevents competing writes/deletes here.
        let result = (|| -> Result<()> {
            ensure!(
                self.file.metadata()?.len() == self.chain.byte_len as u64,
                "journal length differs from its known durable chain"
            );
            ensure!(
                self.file.seek(SeekFrom::End(0))? == self.chain.byte_len as u64,
                "journal append position differs"
            );
            self.file.write_all(&bytes)?;
            self.file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            self.poisoned = true;
            return Err(error.context(
                "journal write/flush is uncertain; retain backups and refuse further effects",
            ));
        }
        self.chain.byte_len += bytes.len();
        self.chain.last_digest = digest;
        self.chain.records.push(record);
        self.reservation = Some(Reservation {
            frames: remaining_frames,
            maximum_frame_bytes,
            _record: PhantomData,
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::super::windows_protocol::Phase;
    use super::*;

    #[derive(Clone, Serialize)]
    struct ObjectSlot {
        future_path: String,
        fingerprint: String,
    }

    #[derive(Serialize)]
    struct ObjectEnvelope {
        original: ObjectSlot,
        current: Option<ObjectSlot>,
        outer_maximum: String,
    }

    #[test]
    fn complete_object_growth_counts_each_slot_null_and_json_encoding_before_effects() {
        let baseline = ObjectSlot {
            future_path: r"\\?\C:\owned\locron.exe".into(),
            fingerprint: "f".repeat(64),
        };
        let future = ObjectSlot {
            future_path: format!(r"\\?\C:\{}", "界".repeat(4096 - 7)),
            fingerprint: "f".repeat(64),
        };
        let maximum = serde_json::to_vec(&future).unwrap().len();
        let outer = ObjectEnvelope {
            original: baseline.clone(),
            current: None,
            outer_maximum: "all separately bounded future outer values".into(),
        };
        let growth = object_growth(Some(&baseline), maximum, 1)
            .unwrap()
            .checked_add(object_growth::<ObjectSlot>(None, maximum, 1).unwrap())
            .unwrap();
        let reservation = preflight_with_growth(&outer, 12, growth).unwrap();
        let future_outer = ObjectEnvelope {
            original: future.clone(),
            current: Some(future),
            outer_maximum: outer.outer_maximum,
        };
        let (actual, _) = frame(&future_outer, [0; 32]).unwrap();
        assert_eq!(actual.len(), reservation.maximum_frame_bytes);
        assert_eq!(reservation.frames, 12);
        assert_eq!(
            object_growth(Some(&baseline), maximum, 2).unwrap(),
            object_growth(Some(&baseline), maximum, 1).unwrap() * 2
        );
        let escaped = serde_json::to_string(&baseline).unwrap();
        assert!(object_growth(Some(&escaped), maximum, 1).is_err());
    }

    #[test]
    fn missing_slot_bound_overflow_and_complete_frame_budget_refuse_without_creation() {
        let record = record(0);
        let (actual, _) = frame(&record, [0; 32]).unwrap();
        let maximum_growth = FRAME_LIMIT - actual.len();
        let exact = preflight_with_growth(&record, FRAME_COUNT, maximum_growth).unwrap();
        assert_eq!(exact.maximum_frame_bytes, FRAME_LIMIT);
        assert!(preflight_with_growth(&record, 1, maximum_growth + 1).is_err());
        assert!(preflight_with_growth(&record, 1, usize::MAX).is_err());
        assert!(preflight_with_growth(&record, FRAME_COUNT + 1, 0).is_err());
        assert!(object_growth::<ObjectSlot>(None, 3, 1).is_err());
        assert!(object_growth::<ObjectSlot>(None, 10, usize::MAX).is_err());
        assert!(object_growth::<ObjectSlot>(None, FRAME_LIMIT, 1).is_err());
        assert!(object_growth::<ObjectSlot>(None, 10, 0).is_err());
        let object = ObjectSlot {
            future_path: "complete baseline".into(),
            fingerprint: "f".repeat(64),
        };
        assert!(object_growth(Some(&object), 4, 1).is_err());
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Record {
        schema: String,
        operation_id: String,
        sid: String,
        request_sha256: String,
        sequence: u32,
        phase: Phase,
        padding: String,
    }

    fn record(sequence: u32) -> Record {
        Record {
            schema: "locron.windows-journal/v1".to_owned(),
            operation_id: "e41c210d-c98d-47fb-9975-a5af66d01346".to_owned(),
            sid: "S-1-5-21-1-2-3-1001".to_owned(),
            request_sha256: "ab".repeat(32),
            sequence,
            phase: if sequence == 0 {
                Phase::Accepted
            } else {
                Phase::Quiescing
            },
            padding: String::new(),
        }
    }

    fn validate(record: &Record, previous: Option<&Record>) -> Result<()> {
        ensure!(
            record.schema == "locron.windows-journal/v1"
                && record.sid == "S-1-5-21-1-2-3-1001"
                && record.operation_id == "e41c210d-c98d-47fb-9975-a5af66d01346"
                && record.request_sha256 == "ab".repeat(32),
            "foreign journal authority"
        );
        ensure!(
            record.sequence == previous.map_or(0, |previous| previous.sequence + 1),
            "journal sequence differs"
        );
        Ok(())
    }

    #[test]
    fn complete_typed_chain_refuses_corrupt_or_partial_frames() {
        let (first, hash) = frame(&record(0), [0; 32]).unwrap();
        let (second, _) = frame(&record(1), hash).unwrap();
        let bytes = [first.as_slice(), second.as_slice()].concat();
        let decoded = decode(&bytes, validate).unwrap();
        assert_eq!(decoded.records.len(), 2);
        assert_eq!(decoded.records[1].sequence, 1);
        for cut in first.len() + 1..bytes.len() {
            assert!(
                decode::<Record>(&bytes[..cut], validate).is_err(),
                "partial tail at {cut}"
            );
        }
        for offset in [
            first.len(),
            first.len() + 4,
            first.len() + HEADER + 20,
            bytes.len() - 1,
        ] {
            let mut changed = bytes.clone();
            changed[offset] ^= 1;
            assert!(
                decode::<Record>(&changed, validate).is_err(),
                "corruption at {offset}"
            );
        }
        let mut other = record(1);
        other.operation_id = "other-operation".to_owned();
        let (graft, _) = frame(&other, hash).unwrap();
        assert!(
            decode::<Record>(&[first.as_slice(), graft.as_slice()].concat(), validate).is_err()
        );
    }

    #[test]
    fn typed_records_and_all_finite_bounds_are_required() {
        let value = serde_json::to_string(&record(0)).unwrap();
        for changed in [
            value.replacen("\"schema\":", "\"unknown\":1,\"schema\":", 1),
            value.replacen("\"schema\":", "\"schema\":\"duplicate\",\"schema\":", 1),
            value.replace("\"accepted\"", "\"invented-phase\""),
        ] {
            // Serialize a raw malformed record only to exercise the typed decoder.
            let payload = changed.as_bytes();
            let size = payload.len() + OVERHEAD;
            let mut bytes = (size as u32).to_le_bytes().to_vec();
            bytes.extend_from_slice(&[0; 32]);
            bytes.extend_from_slice(payload);
            let digest: [u8; 32] = Sha256::digest(&bytes).into();
            bytes.extend_from_slice(&digest);
            assert!(decode::<Record>(&bytes, validate).is_err());
        }
        let mut oversized = record(0);
        oversized.padding = "x".repeat(FRAME_LIMIT);
        assert!(frame(&oversized, [0; 32]).is_err());
        assert!(decode::<Record>(&vec![0; JOURNAL_LIMIT + 1], validate).is_err());
        let mut bytes = Vec::new();
        let mut hash = [0; 32];
        for sequence in 0..=FRAME_COUNT {
            let (entry, next) = frame(&record(sequence as u32), hash).unwrap();
            bytes.extend_from_slice(&entry);
            hash = next;
        }
        assert!(decode::<Record>(&bytes, validate).is_err());
    }

    #[test]
    fn guarded_flush_roundtrip_reserves_before_effect_and_refuses_unknown_existing_journal() {
        let root = tempfile::Builder::new()
            .prefix("locron-journal-fixture-")
            .tempdir()
            .unwrap();
        locron_core::filesystem::restrict_owned(root.path(), true).unwrap();
        let path = root.path().join("journal.bin");
        let worst_case = record(999);
        assert!(preflight(&worst_case, FRAME_COUNT + 1).is_err());
        let mut oversized = record(0);
        oversized.padding = "x".repeat(FRAME_LIMIT);
        assert!(preflight(&oversized, 1).is_err());
        assert!(!path.exists());
        let budget = preflight(&worst_case, 4).unwrap();
        let mut journal: Journal<Record> = Journal::create(&path, budget).unwrap();
        assert!(journal.last_record().is_none());
        journal.append(record(0), validate).unwrap();
        journal.append(record(1), validate).unwrap();
        assert_eq!(journal.last_record().unwrap().sequence, 1);
        assert!(journal.append(record(3), validate).is_err());
        drop(journal);
        let mut journal = Journal::<Record>::open(&path, validate).unwrap();
        assert_eq!(journal.last_record().unwrap().sequence, 1);
        let length = journal.file.metadata().unwrap().len();
        assert!(journal.append(record(2), validate).is_err());
        assert_eq!(journal.file.metadata().unwrap().len(), length);
        assert!(!journal.poisoned);
        assert!(
            journal
                .reserve(preflight(&worst_case, FRAME_COUNT).unwrap())
                .is_err()
        );
        journal.reserve(preflight(&worst_case, 4).unwrap()).unwrap();
        let length = journal.file.metadata().unwrap().len();
        journal.file.set_len(length - 1).unwrap();
        journal.file.sync_all().unwrap();
        assert!(journal.append(record(2), validate).is_err());
        assert!(journal.poisoned);
        assert!(journal.reserve(preflight(&worst_case, 1).unwrap()).is_err());
        assert!(journal.append(record(2), validate).is_err());
        drop(journal);
        assert!(Journal::<Record>::open(&path, validate).is_err());
        let empty = root.path().join("empty.bin");
        drop(Journal::<Record>::create(&empty, preflight(&worst_case, 1).unwrap()).unwrap());
        assert!(Journal::<Record>::open(&empty, validate).is_err());
        assert!(Journal::<Record>::create(&path, preflight(&worst_case, 1).unwrap()).is_err());
    }

    #[test]
    fn preflight_counts_encoded_full_records_and_append_consumes_its_budget() {
        let mut worst_case = record(999);
        // Encoding the complete future record counts escaped bytes too.
        worst_case.padding = "\\\n".repeat(FRAME_LIMIT / 4);
        assert!(preflight(&worst_case, 1).is_err());
        worst_case.padding.clear();
        assert!(preflight(&worst_case, 0).is_err());
        assert!(preflight(&worst_case, usize::MAX).is_err());
        let root = tempfile::Builder::new()
            .prefix("locron-reservation-fixture-")
            .tempdir()
            .unwrap();
        locron_core::filesystem::restrict_owned(root.path(), true).unwrap();
        let path = root.path().join("journal.bin");
        let mut journal = Journal::create(&path, preflight(&worst_case, 1).unwrap()).unwrap();
        journal.append(record(0), validate).unwrap();
        let length = journal.file.metadata().unwrap().len();
        assert!(journal.append(record(1), validate).is_err());
        assert_eq!(journal.file.metadata().unwrap().len(), length);
        assert_eq!(journal.last_record().unwrap().sequence, 0);
        assert!(!journal.poisoned);
        journal.reserve(preflight(&worst_case, 2).unwrap()).unwrap();
        let mut larger = record(1);
        larger.padding = "a field absent from the worst-case record".to_owned();
        assert!(journal.append(larger, validate).is_err());
        assert_eq!(journal.file.metadata().unwrap().len(), length);
        assert_eq!(journal.last_record().unwrap().sequence, 0);
    }
}
