//! Native framed heartbeat observation controls in their own test process.
#![cfg(windows)]

#[path = "support/windows_heartbeat_failure.rs"]
mod windows_heartbeat_failure;

use locron_store::{FrameChannel, StatePaths};
use std::io::Write;
use std::path::PathBuf;
use windows_heartbeat_failure::{
    HEARTBEAT_FAILURE_PAYLOAD_CAP, HEARTBEAT_FAILURE_PHYSICAL_CAP, HeartbeatFailure,
    HeartbeatFailurePhase, read_finalized_heartbeat_failure,
};

mod heartbeat_failure_observation {
    use super::*;
    use locron_store::FrameWriter;

    const RECORD: &[u8] =
        b"native-heartbeat-failure phase=persist counter=8 kind=PermissionDenied raw_os=Some(5)\n";

    fn required<T, E>(result: Result<T, E>) -> T {
        result.unwrap_or_else(|_| panic!("heartbeat observation control setup refused"))
    }

    struct FramedFixture {
        _temporary: tempfile::TempDir,
        paths: StatePaths,
        run_id: String,
        physical: u64,
        payload: u64,
    }

    impl FramedFixture {
        fn new(frames: &[(FrameChannel, &[u8])]) -> Self {
            let temporary = required(tempfile::tempdir());
            let paths = StatePaths::new(temporary.path().join("private observation"));
            let run_id = uuid::Uuid::from_u128(901).to_string();
            let path = required(paths.final_output(&run_id, 1));
            let mut writer = required(FrameWriter::create(&path));
            let mut payload = 0;
            for (channel, bytes) in frames {
                required(writer.write(*channel, 1, bytes));
                payload += bytes.len() as u64;
            }
            let physical = required(writer.sync());
            drop(writer);
            Self {
                _temporary: temporary,
                paths,
                run_id,
                physical,
                payload,
            }
        }

        fn path(&self) -> PathBuf {
            required(self.paths.final_output(&self.run_id, 1))
        }

        fn observe(&self) -> Option<HeartbeatFailure> {
            read_finalized_heartbeat_failure(
                &self.paths,
                &self.run_id,
                1,
                self.physical,
                self.payload,
            )
        }

        fn replace_bytes(&mut self, bytes: &[u8]) {
            let mut file = required(locron_core::filesystem::open_private(
                &self.path(),
                std::fs::OpenOptions::new().write(true).truncate(true),
            ));
            required(file.write_all(bytes));
            required(file.sync_all());
            self.physical = bytes.len() as u64;
        }
    }

    #[test]
    fn observes_one_fragmented_stderr_record_as_typed_fields() {
        let fixture = FramedFixture::new(&[
            (FrameChannel::Stdout, b"unrelated stdout"),
            (FrameChannel::Stderr, b"unrelated panic prefix\n"),
            (FrameChannel::Stderr, &RECORD[..19]),
            (FrameChannel::Stdout, b"interleaved stdout"),
            (FrameChannel::Stderr, &RECORD[19..]),
            (FrameChannel::Stderr, b"unrelated panic suffix\n"),
        ]);
        let expected = HeartbeatFailure {
            phase: HeartbeatFailurePhase::Persist,
            counter: 8,
            kind: std::io::ErrorKind::PermissionDenied,
            raw_os: Some(5),
        };
        assert_eq!(fixture.observe(), Some(expected));
        assert_eq!(
            expected.to_string(),
            "phase=persist counter=8 kind=PermissionDenied raw_os=Some(5)"
        );
        for (phase, label) in [
            (HeartbeatFailurePhase::Create, "create"),
            (HeartbeatFailurePhase::Write, "write"),
            (HeartbeatFailurePhase::Flush, "flush"),
            (HeartbeatFailurePhase::Persist, "persist"),
        ] {
            for (raw, expected_raw) in [
                ("None", None),
                ("Some(-2147483648)", Some(i32::MIN)),
                ("Some(2147483647)", Some(i32::MAX)),
            ] {
                let record = format!(
                    "native-heartbeat-failure phase={label} counter=18446744073709551615 kind=Other raw_os={raw}\r\n"
                );
                let fixture = FramedFixture::new(&[(FrameChannel::Stderr, record.as_bytes())]);
                let observed = fixture.observe().unwrap();
                assert_eq!(observed.phase, phase);
                assert_eq!(observed.counter, u64::MAX);
                assert_eq!(observed.kind, std::io::ErrorKind::Other);
                assert_eq!(observed.raw_os, expected_raw);
            }
        }
    }

    #[test]
    fn malformed_duplicate_partial_and_wrong_channel_records_stay_unobserved() {
        for channel in [FrameChannel::Stdout, FrameChannel::Body] {
            assert!(FramedFixture::new(&[(channel, RECORD)]).observe().is_none());
        }
        let duplicate = FramedFixture::new(&[
            (FrameChannel::Stderr, RECORD),
            (FrameChannel::Stderr, RECORD),
        ]);
        assert!(duplicate.observe().is_none());
        for record in [
            "native-heartbeat-failure phase=unknown counter=8 kind=Other raw_os=None\n",
            "native-heartbeat-failure phase=create counter=8 kind=unknown raw_os=None\n",
            "native-heartbeat-failure phase=create counter=8 kind=Other raw_os=Some(2147483648)\n",
            "native-heartbeat-failure phase=create counter=8 kind=Other raw_os=Some(-2147483649)\n",
            "native-heartbeat-failure phase=create counter=18446744073709551616 kind=Other raw_os=None\n",
            "native-heartbeat-failure phase=create counter=-1 kind=Other raw_os=None\n",
            "native-heartbeat-failure phase=create counter=08 kind=Other raw_os=None\n",
            "native-heartbeat-failure phase=create counter=8 kind=Other raw_os=Some(+5)\n",
            "native-heartbeat-failure phase=create counter=8 kind=Other raw_os=None extra=private\n",
            "prefix native-heartbeat-failure phase=create counter=8 kind=Other raw_os=None\n",
            "native-heartbeat-failure phase=create counter=8 kind=Other raw_os=None",
        ] {
            let fixture = FramedFixture::new(&[(FrameChannel::Stderr, record.as_bytes())]);
            assert!(fixture.observe().is_none());
        }
    }

    #[test]
    fn corrupt_truncated_oversized_missing_and_refused_frames_stay_unobserved() {
        for damage in 0..4 {
            let mut fixture = FramedFixture::new(&[(FrameChannel::Stderr, RECORD)]);
            let mut bytes = required(std::fs::read(fixture.path()));
            match damage {
                0 => bytes[0] ^= 1,
                1 => *bytes.last_mut().unwrap() ^= 1,
                2 => {
                    bytes.pop();
                }
                _ => bytes.push(0),
            }
            fixture.replace_bytes(&bytes);
            assert!(fixture.observe().is_none());
        }
        for length in [
            HEARTBEAT_FAILURE_PAYLOAD_CAP as usize + 1,
            HEARTBEAT_FAILURE_PHYSICAL_CAP as usize + 1,
        ] {
            let bytes = vec![b'x'; length];
            let fixture = FramedFixture::new(&[(FrameChannel::Stderr, &bytes)]);
            assert!(fixture.observe().is_none());
        }
        let mut frames = vec![(FrameChannel::Stdout, b"x".as_slice()); 31];
        frames.push((FrameChannel::Stderr, RECORD));
        assert!(FramedFixture::new(&frames).observe().is_some());
        frames.insert(0, (FrameChannel::Stdout, b"x"));
        assert!(FramedFixture::new(&frames).observe().is_none());
        let fixture = FramedFixture::new(&[(FrameChannel::Stderr, RECORD)]);
        let held = required(locron_core::filesystem::open_private(
            &fixture.path(),
            std::fs::OpenOptions::new().read(true).write(true),
        ));
        assert!(fixture.observe().is_none());
        drop(held);
        assert!(fixture.observe().is_some());
        required(std::fs::remove_file(fixture.path()));
        assert!(fixture.observe().is_none());
    }
}
