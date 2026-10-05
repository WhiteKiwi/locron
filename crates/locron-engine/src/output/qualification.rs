//! Fixed, real-file controls for the private production parser and repair helper.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use locron_core::filesystem::{self, GuardedFile};

use super::*;

const FIXTURE_CAP: usize = 1_048_576 + 256;

#[derive(Clone, Copy)]
struct Horizon(Instant);

impl Horizon {
    fn new() -> Self {
        Self(Instant::now() + Duration::from_secs(30))
    }

    fn check(self, key: &str, phase: &str) {
        assert!(
            Instant::now() < self.0,
            "output-qualification key={key} phase={phase} deadline"
        );
    }

    fn require(self, key: &str, phase: &str, condition: bool) {
        self.check(key, phase);
        assert!(condition, "output-qualification key={key} phase={phase}");
    }

    fn io<T>(
        self,
        key: &str,
        phase: &str,
        operation: impl FnOnce() -> io::Result<T>,
    ) -> io::Result<T> {
        self.check(key, phase);
        let result = operation();
        self.check(key, phase);
        result
    }

    fn need<T>(self, key: &str, phase: &str, result: io::Result<T>) -> T {
        self.check(key, phase);
        result.unwrap_or_else(|error| {
            panic!(
                "output-qualification key={key} phase={phase} kind={:?} raw={:?}",
                error.kind(),
                error.raw_os_error()
            )
        })
    }
}

struct Rows {
    horizon: Horizon,
    completed: BTreeSet<&'static str>,
}

impl Rows {
    fn new() -> Self {
        Self {
            horizon: Horizon::new(),
            completed: BTreeSet::new(),
        }
    }

    fn complete(&mut self, key: &'static str) {
        self.horizon
            .require(key, "row-complete", self.completed.insert(key));
        println!("output-qualification PASS {key}");
    }

    fn finish(&self, count: usize) {
        self.horizon
            .require("engine:group", "row-count", self.completed.len() == count);
    }
}

struct Fixture {
    root_guard: filesystem::DirectoryGuard,
    temporary: tempfile::TempDir,
    path: PathBuf,
    horizon: Horizon,
    key: &'static str,
    owned: Rc<RefCell<BTreeMap<PathBuf, ObjectIdentity>>>,
    #[cfg(unix)]
    root_identity: (u64, u64),
    #[cfg(unix)]
    temporary_identity: (u64, u64),
}

#[cfg(unix)]
type ObjectIdentity = (u64, u64);
#[cfg(windows)]
type ObjectIdentity = filesystem::FileIdentity;

fn object_identity(file: &GuardedFile) -> io::Result<ObjectIdentity> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        file.metadata()
            .map(|metadata| (metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        filesystem::file_identity(file)
    }
}

#[cfg(unix)]
fn directory_identity(path: &Path) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt as _;
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    Ok((metadata.dev(), metadata.ino()))
}

impl Fixture {
    fn new(horizon: Horizon, key: &'static str, bytes: &[u8]) -> Self {
        horizon.require(key, "fixture-cap", bytes.len() <= FIXTURE_CAP);
        let mut temporary = horizon.need(
            key,
            "temporary",
            horizon.io(key, "temporary", tempfile::tempdir),
        );
        temporary.disable_cleanup(true);
        let root = temporary.path().join("private");
        let guard = horizon.need(
            key,
            "private-root",
            horizon.io(key, "private-root", || {
                filesystem::DirectoryGuard::private(&root)
            }),
        );
        let root = guard.normalized_path().to_path_buf();
        #[cfg(unix)]
        let root_identity = horizon.need(
            key,
            "root-identity",
            horizon.io(key, "root-identity", || directory_identity(&root)),
        );
        #[cfg(unix)]
        let temporary_identity = horizon.need(
            key,
            "temporary-identity",
            horizon.io(key, "temporary-identity", || {
                directory_identity(temporary.path())
            }),
        );
        let path = root.join("output.partial");
        let mut writer = horizon.need(
            key,
            "create",
            horizon.io(key, "create", || filesystem::create_private_new(&path)),
        );
        horizon.need(
            key,
            "fixture-write",
            horizon.io(key, "fixture-write", || writer.write_all(bytes)),
        );
        let identity = horizon.need(
            key,
            "created-identity",
            horizon.io(key, "created-identity", || object_identity(&writer)),
        );
        horizon.need(
            key,
            "fixture-sync",
            horizon.io(key, "fixture-sync", || writer.sync_all()),
        );
        drop(writer);
        horizon.check(key, "writer-released");
        let owned = Rc::new(RefCell::new(BTreeMap::from([(path.clone(), identity)])));
        Self {
            root_guard: guard,
            temporary,
            path,
            horizon,
            key,
            owned,
            #[cfg(unix)]
            root_identity,
            #[cfg(unix)]
            temporary_identity,
        }
    }

    fn admit(&self) -> Adapter {
        let file = self.horizon.need(
            self.key,
            "admit",
            self.horizon.io(self.key, "admit", || {
                filesystem::open_private(&self.path, OpenOptions::new().read(true).write(true))
            }),
        );
        Adapter {
            file,
            horizon: self.horizon,
            key: self.key,
            fault: None,
            witness: Arc::new(()),
            injected: 0,
            counts: Counts::default(),
            control: Control::None,
            control_ran: false,
            #[cfg(unix)]
            owned: Rc::clone(&self.owned),
            #[cfg(not(unix))]
            _owned: Rc::clone(&self.owned),
        }
    }

    fn close(self) {
        let h = self.horizon;
        let key = self.key;
        let root = self.root_guard.normalized_path().to_path_buf();
        #[cfg(unix)]
        {
            let root_after = h.need(
                key,
                "cleanup-root-id",
                h.io(key, "cleanup-root-id", || directory_identity(&root)),
            );
            let temporary_after = h.need(
                key,
                "cleanup-temp-id",
                h.io(key, "cleanup-temp-id", || {
                    directory_identity(self.temporary.path())
                }),
            );
            h.require(
                key,
                "cleanup-directory-ids",
                root_after == self.root_identity && temporary_after == self.temporary_identity,
            );
        }
        for (path, expected) in self.owned.borrow().iter() {
            h.require(key, "owned-parent", path.parent() == Some(root.as_path()));
            let file = h.need(
                key,
                "cleanup-file-open",
                h.io(key, "cleanup-file-open", || {
                    filesystem::open_private(path, OpenOptions::new().read(true))
                }),
            );
            let actual = h.need(
                key,
                "cleanup-file-id",
                h.io(key, "cleanup-file-id", || object_identity(&file)),
            );
            h.require(key, "cleanup-owned-id", actual == *expected);
            drop(file);
            h.check(key, "cleanup-file-released");
            h.need(
                key,
                "cleanup-file",
                h.io(key, "cleanup-file", || std::fs::remove_file(path)),
            );
        }
        drop(self.root_guard);
        self.horizon.check(self.key, "root-released");
        h.need(
            key,
            "cleanup-empty-root",
            h.io(key, "cleanup-empty-root", || std::fs::remove_dir(&root)),
        );
        h.need(
            key,
            "cleanup-empty-temp",
            h.io(key, "cleanup-empty-temp", || {
                std::fs::remove_dir(self.temporary.path())
            }),
        );
    }

    #[cfg(windows)]
    fn moved(&self, destination: &Path) {
        let identity = self
            .owned
            .borrow_mut()
            .remove(&self.path)
            .expect("owned source leaf");
        self.owned
            .borrow_mut()
            .insert(destination.to_path_buf(), identity);
        self.horizon.check(self.key, "owned-move-recorded");
    }
}

fn exact_bytes(horizon: Horizon, key: &str, path: &Path, expected: &[u8]) {
    let mut file = horizon.need(
        key,
        "oracle-open",
        horizon.io(key, "oracle-open", || {
            filesystem::open_private(path, OpenOptions::new().read(true))
        }),
    );
    exact_reader(horizon, key, &mut *file, expected);
    drop(file);
    horizon.check(key, "oracle-released");
}

fn exact_reader(horizon: Horizon, key: &str, file: &mut impl Read, expected: &[u8]) {
    horizon.require(key, "oracle-cap", expected.len() <= FIXTURE_CAP);
    let mut capped = file.take(expected.len() as u64 + 1);
    let mut buffer = [0; 8192];
    let mut offset = 0;
    loop {
        let count = horizon.need(
            key,
            "oracle-read",
            horizon.io(key, "oracle-read", || capped.read(&mut buffer)),
        );
        if count == 0 {
            break;
        }
        let end = offset + count;
        horizon.require(key, "oracle-length", end <= expected.len());
        horizon.require(
            key,
            "oracle-bytes",
            buffer[..count] == expected[offset..end],
        );
        offset = end;
    }
    horizon.require(key, "oracle-eof", offset == expected.len());
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Point {
    ReadAt(u64),
    Metadata(u32),
    SetLen,
    Sync,
    Seek,
}

#[derive(Clone, Copy)]
struct Fault {
    point: Point,
    kind: io::ErrorKind,
}

struct Marker {
    key: &'static str,
    witness: Arc<()>,
}

impl fmt::Debug for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("OutputFaultMarker")
    }
}
impl fmt::Display for Marker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("controlled output I/O")
    }
}
impl std::error::Error for Marker {}

#[derive(Default)]
struct Counts {
    read_attempts: u64,
    bytes_read: u64,
    largest_read: usize,
    metadata_attempts: u32,
    metadata_completed: u32,
    set_len_attempts: u32,
    set_len_completed: u32,
    sync_attempts: u32,
    sync_completed: u32,
    seek_attempts: u32,
    seek_completed: u32,
}

enum Control {
    None,
    Grow(PathBuf),
    Shrink,
    Prefix(PathBuf),
    #[cfg(unix)]
    Replace {
        path: PathBuf,
        saved: PathBuf,
    },
    #[cfg(windows)]
    RenameRefusal {
        path: PathBuf,
        saved: PathBuf,
        identity: filesystem::FileIdentity,
    },
}

struct Adapter {
    file: GuardedFile,
    horizon: Horizon,
    key: &'static str,
    fault: Option<Fault>,
    witness: Arc<()>,
    injected: u32,
    counts: Counts,
    control: Control,
    control_ran: bool,
    #[cfg(unix)]
    owned: Rc<RefCell<BTreeMap<PathBuf, ObjectIdentity>>>,
    #[cfg(not(unix))]
    _owned: Rc<RefCell<BTreeMap<PathBuf, ObjectIdentity>>>,
}

impl Adapter {
    fn fault_at(&mut self, point: Point) -> io::Result<()> {
        if let Some(fault) = self.fault.filter(|fault| fault.point == point) {
            self.injected += 1;
            return Err(io::Error::new(
                fault.kind,
                Marker {
                    key: self.key,
                    witness: Arc::clone(&self.witness),
                },
            ));
        }
        Ok(())
    }

    fn control_at(&mut self, ordinal: u32) -> io::Result<()> {
        let selected = match &self.control {
            Control::None => false,
            Control::Prefix(_) => ordinal == 1,
            _ => ordinal == 2,
        };
        if !selected {
            return Ok(());
        }
        let control = std::mem::replace(&mut self.control, Control::None);
        let h = self.horizon;
        let key = self.key;
        match control {
            Control::Grow(path) => {
                let mut writer = h.io(key, "control-open", || {
                    filesystem::open_private(&path, OpenOptions::new().append(true))
                })?;
                h.io(key, "control-grow", || writer.write_all(&[0xa5]))?;
                drop(writer);
            }
            Control::Shrink => {
                let len = h
                    .io(key, "control-metadata", || self.file.metadata())?
                    .len();
                h.io(key, "control-shrink", || self.file.set_len(len - 1))?;
            }
            Control::Prefix(path) => {
                let mut writer = h.io(key, "control-open", || {
                    filesystem::open_private(&path, OpenOptions::new().append(true))
                })?;
                h.io(key, "control-prefix", || {
                    writer.write_all(&encode_frame(&second_frame()))
                })?;
                drop(writer);
            }
            #[cfg(unix)]
            Control::Replace { path, saved } => {
                h.io(key, "control-rename", || std::fs::rename(&path, &saved))?;
                let identity = self
                    .owned
                    .borrow_mut()
                    .remove(&path)
                    .expect("owned original leaf");
                self.owned.borrow_mut().insert(saved, identity);
                let mut replacement = h.io(key, "control-create", || {
                    filesystem::create_private_new(&path)
                })?;
                h.io(key, "control-replacement", || {
                    replacement.write_all(b"replacement object")
                })?;
                h.io(key, "control-sync", || replacement.sync_all())?;
                let replacement_identity = h.io(key, "replacement-owned-id", || {
                    object_identity(&replacement)
                })?;
                self.owned.borrow_mut().insert(path, replacement_identity);
                drop(replacement);
            }
            #[cfg(windows)]
            Control::RenameRefusal {
                path,
                saved,
                identity,
            } => {
                let result = h.io(key, "control-rename", || std::fs::rename(&path, &saved));
                let Err(error) = result else {
                    panic!("output-qualification key={key} phase=rename-not-refused");
                };
                h.require(
                    key,
                    "native-sharing",
                    matches!(error.raw_os_error(), Some(32 | 33)),
                );
                let after = h.io(key, "control-identity", || {
                    filesystem::file_identity(&self.file)
                })?;
                h.require(key, "full-identity", after == identity);
                exact_bytes(h, key, &path, &seed_tail());
            }
            Control::None => unreachable!("selected control has no operation"),
        }
        self.control_ran = true;
        h.check(key, "control-released");
        Ok(())
    }

    fn no_repair_writes(&self) {
        self.horizon.require(
            self.key,
            "no-repair-writes",
            self.counts.set_len_attempts == 0 && self.counts.sync_attempts == 0,
        );
    }

    fn single_repair_writes(&self) {
        self.horizon.require(
            self.key,
            "single-truncate-sync",
            self.counts.set_len_attempts == 1
                && self.counts.set_len_completed == 1
                && self.counts.sync_attempts == 1
                && self.counts.sync_completed == 1,
        );
    }
}

impl Read for Adapter {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.horizon.check(self.key, "read");
        self.counts.read_attempts += 1;
        self.counts.largest_read = self.counts.largest_read.max(buffer.len());
        let result = self
            .fault_at(Point::ReadAt(self.counts.bytes_read))
            .and_then(|()| (*self.file).read(buffer));
        self.horizon.check(self.key, "read");
        if let Ok(count) = &result {
            self.counts.bytes_read += *count as u64;
        }
        result
    }
}

impl Seek for Adapter {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.horizon.check(self.key, "seek");
        self.counts.seek_attempts += 1;
        let result = self
            .fault_at(Point::Seek)
            .and_then(|()| self.file.seek(position));
        self.horizon.check(self.key, "seek");
        if result.is_ok() {
            self.counts.seek_completed += 1;
        }
        result
    }
}

impl RepairFile for Adapter {
    fn repair_len(&mut self) -> io::Result<u64> {
        self.horizon.check(self.key, "metadata");
        self.counts.metadata_attempts += 1;
        let ordinal = self.counts.metadata_attempts;
        let result = (|| {
            self.fault_at(Point::Metadata(ordinal))?;
            if ordinal == 2 {
                self.control_at(ordinal)?;
            }
            let len = self.file.metadata()?.len();
            self.counts.metadata_completed += 1;
            if ordinal == 1 {
                self.control_at(ordinal)?;
            }
            Ok(len)
        })();
        self.horizon.check(self.key, "metadata");
        result
    }

    fn repair_set_len(&mut self, len: u64) -> io::Result<()> {
        self.horizon.check(self.key, "set-len");
        self.counts.set_len_attempts += 1;
        let result = self
            .fault_at(Point::SetLen)
            .and_then(|()| self.file.set_len(len));
        self.horizon.check(self.key, "set-len");
        if result.is_ok() {
            self.counts.set_len_completed += 1;
        }
        result
    }

    fn repair_sync(&mut self) -> io::Result<()> {
        self.horizon.check(self.key, "sync");
        self.counts.sync_attempts += 1;
        let result = self
            .fault_at(Point::Sync)
            .and_then(|()| self.file.sync_all());
        self.horizon.check(self.key, "sync");
        if result.is_ok() {
            self.counts.sync_completed += 1;
        }
        result
    }
}

fn encode_frame(frame: &Frame) -> Vec<u8> {
    let mut header = Vec::with_capacity(25 + frame.payload.len());
    header.push(frame.channel as u8);
    header.extend_from_slice(&frame.sequence.to_le_bytes());
    header.extend_from_slice(&frame.elapsed_micros.to_le_bytes());
    header.extend_from_slice(&u32::try_from(frame.payload.len()).unwrap().to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&header);
    crc.update(&frame.payload);
    header.extend_from_slice(&crc.finalize().to_le_bytes());
    header.extend_from_slice(&frame.payload);
    header
}

fn encoded(frames: &[Frame]) -> Vec<u8> {
    let mut bytes = b"LOCRON\0\x01".to_vec();
    for frame in frames {
        bytes.extend_from_slice(&encode_frame(frame));
    }
    bytes
}

fn seed_frame() -> Frame {
    Frame {
        channel: Channel::Stdout,
        sequence: 0,
        elapsed_micros: 1,
        payload: vec![0x62, 0x61, 0x73, 0x65, 0, 0xff],
    }
}
fn second_frame() -> Frame {
    Frame {
        channel: Channel::Stdout,
        sequence: 1,
        elapsed_micros: 2,
        payload: b"tailbytes".to_vec(),
    }
}
fn seed() -> Vec<u8> {
    encoded(&[seed_frame()])
}
fn seed_tail() -> Vec<u8> {
    let mut bytes = seed();
    bytes.extend_from_slice(&[1, 1, 0]);
    bytes
}

struct Golden {
    key: &'static str,
    bytes: Vec<u8>,
    frames: Vec<Frame>,
    open_error: Option<io::ErrorKind>,
}

impl Golden {
    fn valid(key: &'static str, frames: Vec<Frame>) -> Self {
        Self {
            key,
            bytes: encoded(&frames),
            frames,
            open_error: None,
        }
    }
    fn tail(key: &'static str, tail: &[u8]) -> Self {
        let mut bytes = seed();
        bytes.extend_from_slice(tail);
        Self {
            key,
            bytes,
            frames: vec![seed_frame()],
            open_error: None,
        }
    }
}

fn golden_rows() -> Vec<Golden> {
    const SHORT: [&str; 8] = [
        "engine:F-MAGIC-SHORT-00",
        "engine:F-MAGIC-SHORT-01",
        "engine:F-MAGIC-SHORT-02",
        "engine:F-MAGIC-SHORT-03",
        "engine:F-MAGIC-SHORT-04",
        "engine:F-MAGIC-SHORT-05",
        "engine:F-MAGIC-SHORT-06",
        "engine:F-MAGIC-SHORT-07",
    ];
    let mut rows = Vec::new();
    for (length, key) in SHORT.into_iter().enumerate() {
        rows.push(Golden {
            key,
            bytes: b"LOCRON\0\x01"[..length].to_vec(),
            frames: Vec::new(),
            open_error: Some(io::ErrorKind::UnexpectedEof),
        });
    }
    rows.push(Golden {
        key: "engine:F-MAGIC-BAD",
        bytes: b"LOCRON\0\x02".to_vec(),
        frames: Vec::new(),

        open_error: Some(io::ErrorKind::InvalidData),
    });
    rows.push(Golden::valid("engine:F-VALID-EMPTY", Vec::new()));
    rows.push(Golden::valid(
        "engine:F-VALID-BINARY",
        vec![Frame {
            channel: Channel::Stdout,
            sequence: 0,
            elapsed_micros: 1,
            payload: vec![0, 0xff, 0x0a],
        }],
    ));
    rows.push(Golden::valid(
        "engine:F-VALID-CHANNELS",
        vec![
            Frame {
                channel: Channel::Stdout,
                sequence: 0,
                elapsed_micros: 10,
                payload: vec![0x53, 0],
            },
            Frame {
                channel: Channel::Stderr,
                sequence: 1,
                elapsed_micros: 20,
                payload: vec![0x45, 0xff],
            },
            Frame {
                channel: Channel::Body,
                sequence: 2,
                elapsed_micros: 30,
                payload: vec![0x42, 0x0a],
            },
        ],
    ));
    rows.push(Golden::valid(
        "engine:F-VALID-ZERO",
        vec![Frame {
            channel: Channel::Stdout,
            sequence: 0,
            elapsed_micros: 1,
            payload: Vec::new(),
        }],
    ));
    rows.push(Golden::valid(
        "engine:F-VALID-MAX",
        vec![Frame {
            channel: Channel::Stdout,
            sequence: 0,
            elapsed_micros: 1,
            payload: vec![0xa5; 1_048_576],
        }],
    ));
    let many = (0_u8..17)
        .map(|n| Frame {
            channel: [Channel::Stdout, Channel::Stderr, Channel::Body][usize::from(n % 3)],
            sequence: u64::from(n),
            elapsed_micros: u64::from(n) + 1,
            payload: vec![n, 0, 0xff, 0x0a, 0xa5],
        })
        .collect();
    rows.push(Golden::valid("engine:F-VALID-MANY", many));
    let second = encode_frame(&second_frame());
    rows.push(Golden::tail("engine:F-TAIL-HEADER-01", &second[..1]));
    rows.push(Golden::tail("engine:F-TAIL-HEADER-24", &second[..24]));
    rows.push(Golden::tail("engine:F-TAIL-PAYLOAD-00", &second[..25]));
    rows.push(Golden::tail("engine:F-TAIL-PAYLOAD-08", &second[..33]));
    let mut channel = second[..25].to_vec();
    channel[0] = 4;
    let mut channel_row = Golden::tail("engine:F-CHANNEL-BAD", &channel);
    channel_row.open_error = Some(io::ErrorKind::InvalidData);
    rows.push(channel_row);
    let mut crc = second.clone();
    crc[21] ^= 1;
    rows.push(Golden::tail("engine:F-CRC-BAD", &crc));
    let mut gap = second_frame();
    gap.sequence = 3;
    rows.push(Golden::tail("engine:F-SEQUENCE-GAP", &encode_frame(&gap)));
    for (key, len) in [
        ("engine:F-OVERSIZE-MAXPLUS", 1_048_577_u32),
        ("engine:F-OVERSIZE-U32MAX", u32::MAX),
    ] {
        let mut header = second[..25].to_vec();
        header[17..21].copy_from_slice(&len.to_le_bytes());
        rows.push(Golden::tail(key, &header));
    }
    rows
}

fn original_error<T>(
    h: Horizon,
    key: &str,
    phase: &str,
    result: io::Result<T>,
    kind: io::ErrorKind,
) -> io::Error {
    let Err(error) = result else {
        panic!("output-qualification key={key} phase={phase}-not-refused");
    };
    h.require(key, phase, error.kind() == kind);
    error
}

fn require_marker(h: Horizon, key: &str, error: &io::Error, adapter: &Adapter) {
    let marker = error
        .get_ref()
        .and_then(|error| error.downcast_ref::<Marker>());
    h.require(
        key,
        "original-marker",
        marker.is_some_and(|marker| {
            marker.key == key && Arc::ptr_eq(&marker.witness, &adapter.witness)
        }) && error.raw_os_error().is_none()
            && adapter.injected == 1,
    );
}

#[test]
fn format_contracts() {
    let mut rows = Rows::new();
    let h = rows.horizon;
    h.check("engine:format", "golden-setup");
    let golden = golden_rows();
    h.require("engine:format", "golden-count", golden.len() == 24);
    for row in golden {
        let fixture = Fixture::new(h, row.key, &row.bytes);
        let result = h.io(row.key, "public-reader", || read_frames(&fixture.path));
        if let Some(kind) = row.open_error {
            let error = original_error(h, row.key, "reader-refusal", result, kind);
            if row.key == "engine:F-CHANNEL-BAD" {
                h.require(
                    row.key,
                    "channel-message",
                    error.to_string() == "unknown output channel",
                );
            }
        } else {
            let actual = h.need(row.key, "public-reader", result);
            h.require(row.key, "complete-reader-tuples", actual == row.frames);
        }
        exact_bytes(h, row.key, &fixture.path, &row.bytes);
        let mut adapter = fixture.admit();
        let result = h.io(row.key, "repair", || repair_opened(&mut adapter));
        if let Some(kind) = row.open_error {
            original_error(h, row.key, "repair-refusal", result, kind);
            adapter.no_repair_writes();
            exact_bytes(h, row.key, &fixture.path, &row.bytes);
        } else {
            let prefix = encoded(&row.frames);
            let expected = OutputStats {
                retained_bytes: row
                    .frames
                    .iter()
                    .map(|frame| frame.payload.len() as u64)
                    .sum(),
                physical_bytes: prefix.len() as u64,
                ..OutputStats::default()
            };
            h.require(
                row.key,
                "repair-counters",
                h.need(row.key, "repair", result) == expected,
            );
            adapter.single_repair_writes();
            exact_bytes(h, row.key, &fixture.path, &prefix);
            if row.key.starts_with("engine:F-OVERSIZE-") {
                h.require(
                    row.key,
                    "no-payload-read",
                    adapter.counts.bytes_read == 64 && adapter.counts.largest_read == 25,
                );
            }
        }
        drop(adapter);
        h.check(row.key, "adapter-released");
        fixture.close();
        rows.complete(row.key);
    }
    rows.finish(24);
}

#[test]
fn io_failure_contracts() {
    let mut rows = Rows::new();
    let h = rows.horizon;
    let cases = [
        (
            "engine:I-HEADER-PERMISSION_DENIED",
            Point::ReadAt(39),
            io::ErrorKind::PermissionDenied,
        ),
        (
            "engine:I-HEADER-OTHER",
            Point::ReadAt(39),
            io::ErrorKind::Other,
        ),
        (
            "engine:I-HEADER-IO_INVALID_DATA",
            Point::ReadAt(39),
            io::ErrorKind::InvalidData,
        ),
        (
            "engine:I-PAYLOAD-PERMISSION_DENIED",
            Point::ReadAt(64),
            io::ErrorKind::PermissionDenied,
        ),
        (
            "engine:I-PAYLOAD-OTHER",
            Point::ReadAt(64),
            io::ErrorKind::Other,
        ),
        (
            "engine:I-PAYLOAD-IO_INVALID_DATA",
            Point::ReadAt(64),
            io::ErrorKind::InvalidData,
        ),
        (
            "engine:I-MAGIC-OTHER",
            Point::ReadAt(0),
            io::ErrorKind::Other,
        ),
        (
            "engine:I-METADATA-FIRST",
            Point::Metadata(1),
            io::ErrorKind::PermissionDenied,
        ),
        (
            "engine:I-METADATA-LAST",
            Point::Metadata(2),
            io::ErrorKind::Other,
        ),
        (
            "engine:I-SETLEN",
            Point::SetLen,
            io::ErrorKind::PermissionDenied,
        ),
        ("engine:I-SYNC", Point::Sync, io::ErrorKind::Other),
        ("engine:I-SEEK", Point::Seek, io::ErrorKind::Other),
    ];
    for (key, point, kind) in cases {
        let bytes = if matches!(point, Point::ReadAt(39 | 64)) {
            encoded(&[seed_frame(), second_frame()])
        } else {
            seed_tail()
        };
        let fixture = Fixture::new(h, key, &bytes);
        let mut adapter = fixture.admit();
        if matches!(point, Point::ReadAt(_) | Point::Seek) {
            let mut collector = fixture.admit();
            collector.fault = Some(Fault { point, kind });
            let result = h.io(key, "collect-fault", || {
                collect_valid_frames(&mut collector)
            });
            let error = original_error(h, key, "no-successful-prefix", result, kind);
            require_marker(h, key, &error, &collector);
            collector.no_repair_writes();
            exact_bytes(h, key, &fixture.path, &bytes);
            drop(collector);
            h.check(key, "collector-released");
        }
        adapter.fault = Some(Fault { point, kind });
        let result = h.io(key, "repair-fault", || repair_opened(&mut adapter));
        let error = original_error(h, key, "primary-error", result, kind);
        require_marker(h, key, &error, &adapter);
        match point {
            Point::SetLen => h.require(
                key,
                "setlen-precedence",
                adapter.counts.set_len_attempts == 1
                    && adapter.counts.set_len_completed == 0
                    && adapter.counts.sync_attempts == 0,
            ),
            Point::Sync => h.require(
                key,
                "sync-precedence",
                adapter.counts.set_len_attempts == 1
                    && adapter.counts.set_len_completed == 1
                    && adapter.counts.sync_attempts == 1
                    && adapter.counts.sync_completed == 0,
            ),
            _ => adapter.no_repair_writes(),
        }
        match point {
            Point::ReadAt(offset) => {
                h.require(key, "read-boundary", adapter.counts.bytes_read == offset)
            }
            Point::Metadata(1) => h.require(
                key,
                "no-scan",
                adapter.counts.read_attempts == 0 && adapter.counts.metadata_completed == 0,
            ),
            Point::Metadata(2) => h.require(
                key,
                "last-metadata",
                adapter.counts.metadata_attempts == 2 && adapter.counts.metadata_completed == 1,
            ),
            Point::Seek => h.require(
                key,
                "seek-before-write",
                adapter.counts.seek_attempts == 1
                    && adapter.counts.seek_completed == 0
                    && adapter.counts.metadata_attempts == 1,
            ),
            Point::SetLen | Point::Sync => h.require(
                key,
                "metadata-before-write",
                adapter.counts.metadata_attempts == 2 && adapter.counts.metadata_completed == 2,
            ),
            Point::Metadata(_) => unreachable!("fixed metadata ordinal"),
        }
        let retained = if point == Point::Sync { seed() } else { bytes };
        exact_bytes(h, key, &fixture.path, &retained);
        drop(adapter);
        h.check(key, "adapter-released");
        fixture.close();
        rows.complete(key);
    }
    rows.finish(12);
}

#[test]
fn length_drift_contracts() {
    let mut rows = Rows::new();
    let h = rows.horizon;
    for key in [
        "engine:D-GROW-LAST",
        "engine:D-SHRINK-LAST",
        "engine:D-PREFIX-BEYOND-FIRST",
    ] {
        let bytes = if key == "engine:D-PREFIX-BEYOND-FIRST" {
            seed()
        } else {
            seed_tail()
        };
        let fixture = Fixture::new(h, key, &bytes);
        let mut adapter = fixture.admit();
        adapter.control = match key {
            "engine:D-GROW-LAST" => Control::Grow(fixture.path.clone()),
            "engine:D-SHRINK-LAST" => Control::Shrink,
            _ => Control::Prefix(fixture.path.clone()),
        };
        let error = original_error(
            h,
            key,
            "size-drift",
            h.io(key, "repair-drift", || repair_opened(&mut adapter)),
            io::ErrorKind::InvalidData,
        );
        h.require(
            key,
            "size-message",
            error.to_string() == "output file length changed during repair",
        );
        h.require(key, "actual-control", adapter.control_ran);
        adapter.no_repair_writes();
        let mut expected = bytes;
        match key {
            "engine:D-GROW-LAST" => expected.push(0xa5),
            "engine:D-SHRINK-LAST" => {
                expected.pop();
            }
            _ => expected.extend_from_slice(&encode_frame(&second_frame())),
        }
        h.require(
            key,
            "metadata-short-circuit",
            adapter.counts.metadata_attempts
                == if key == "engine:D-PREFIX-BEYOND-FIRST" {
                    1
                } else {
                    2
                },
        );
        exact_bytes(h, key, &fixture.path, &expected);
        drop(adapter);
        h.check(key, "adapter-released");
        fixture.close();
        rows.complete(key);
    }
    rows.finish(3);
}

#[test]
fn managed_open_refusals() {
    let mut rows = Rows::new();
    let h = rows.horizon;
    for key in ["engine:O-MISSING", "engine:O-DIRECTORY"] {
        let fixture = Fixture::new(h, key, b"owned sentinel");
        let absent = fixture.path.with_file_name("refused.partial");
        if key == "engine:O-DIRECTORY" {
            h.need(
                key,
                "control-directory",
                h.io(key, "control-directory", || std::fs::create_dir(&absent)),
            );
        }
        let result = h.io(key, "public-repair", || repair_partial(&absent));
        if key == "engine:O-MISSING" {
            original_error(h, key, "missing", result, io::ErrorKind::NotFound);
            let absent_result = h.io(key, "absent-oracle", || std::fs::symlink_metadata(&absent));
            original_error(
                h,
                key,
                "not-created",
                absent_result,
                io::ErrorKind::NotFound,
            );
        } else {
            #[cfg(unix)]
            {
                let error = original_error(
                    h,
                    key,
                    "native-directory",
                    result,
                    io::ErrorKind::IsADirectory,
                );
                h.require(key, "native-eisdir", error.raw_os_error() == Some(21));
            }
            #[cfg(windows)]
            {
                let error = original_error(
                    h,
                    key,
                    "native-directory",
                    result,
                    io::ErrorKind::PermissionDenied,
                );
                h.require(
                    key,
                    "native-directory-code",
                    error.raw_os_error() == Some(5),
                );
            }
            let metadata = h.need(
                key,
                "directory-oracle",
                h.io(key, "directory-oracle", || {
                    std::fs::symlink_metadata(&absent)
                }),
            );
            h.require(key, "directory-preserved", metadata.is_dir());
            h.need(
                key,
                "cleanup-owned-directory",
                h.io(key, "cleanup-owned-directory", || {
                    std::fs::remove_dir(&absent)
                }),
            );
        }
        exact_bytes(h, key, &fixture.path, b"owned sentinel");
        fixture.close();
        rows.complete(key);
    }
    rows.finish(2);
}

#[cfg(unix)]
#[test]
fn retained_identity_unix() {
    use std::os::unix::fs::MetadataExt as _;
    let mut rows = Rows::new();
    let h = rows.horizon;
    let key = "engine:U-REPLACE-PATH";
    let fixture = Fixture::new(h, key, &seed_tail());
    let saved = fixture.path.with_file_name("saved.partial");
    let mut adapter = fixture.admit();
    let initial = h.need(
        key,
        "retained-identity",
        h.io(key, "retained-identity", || adapter.file.metadata()),
    );
    adapter.control = Control::Replace {
        path: fixture.path.clone(),
        saved: saved.clone(),
    };
    let repair = h.need(
        key,
        "repair",
        h.io(key, "repair", || repair_opened(&mut adapter)),
    );
    adapter.single_repair_writes();
    h.require(
        key,
        "repair-counters",
        repair
            == OutputStats {
                retained_bytes: 6,
                physical_bytes: 39,
                ..OutputStats::default()
            },
    );
    let retained = h.need(
        key,
        "retained-identity",
        h.io(key, "retained-identity", || adapter.file.metadata()),
    );
    let replacement = h.need(
        key,
        "replacement-identity",
        h.io(key, "replacement-identity", || {
            filesystem::open_private(&fixture.path, OpenOptions::new().read(true))
        }),
    );
    let replacement_id = h.need(
        key,
        "replacement-metadata",
        h.io(key, "replacement-metadata", || replacement.metadata()),
    );
    h.require(
        key,
        "original-inode",
        (initial.dev(), initial.ino()) == (retained.dev(), retained.ino()),
    );
    h.require(
        key,
        "distinct-replacement",
        (initial.dev(), initial.ino()) != (replacement_id.dev(), replacement_id.ino()),
    );
    exact_bytes(h, key, &saved, &seed());
    exact_bytes(h, key, &fixture.path, b"replacement object");
    drop(replacement);
    drop(adapter);
    h.check(key, "guards-released");
    fixture.close();
    rows.complete(key);
    rows.finish(1);
}

#[cfg(windows)]
#[test]
fn retained_identity_windows() {
    let mut rows = Rows::new();
    let h = rows.horizon;
    let key = "engine:W-RENAME-NODELETE";
    let fixture = Fixture::new(h, key, &seed_tail());
    let saved = fixture.path.with_file_name("saved.partial");
    let mut adapter = fixture.admit();
    let identity = h.need(
        key,
        "full-identity",
        h.io(key, "full-identity", || {
            filesystem::file_identity(&adapter.file)
        }),
    );
    adapter.control = Control::RenameRefusal {
        path: fixture.path.clone(),
        saved: saved.clone(),
        identity,
    };
    let repair = h.need(
        key,
        "repair",
        h.io(key, "repair", || repair_opened(&mut adapter)),
    );
    adapter.single_repair_writes();
    h.require(key, "actual-refusal", adapter.control_ran);
    h.require(
        key,
        "repair-counters",
        repair
            == OutputStats {
                retained_bytes: 6,
                physical_bytes: 39,
                ..OutputStats::default()
            },
    );
    exact_bytes(h, key, &fixture.path, &seed());
    drop(adapter);
    h.check(key, "guards-released");
    h.need(
        key,
        "same-source-positive",
        h.io(key, "same-source-positive", || {
            std::fs::rename(&fixture.path, &saved)
        }),
    );
    fixture.moved(&saved);
    let final_file = h.need(
        key,
        "positive-open",
        h.io(key, "positive-open", || {
            filesystem::open_private(&saved, OpenOptions::new().read(true))
        }),
    );
    let after = h.need(
        key,
        "positive-identity",
        h.io(key, "positive-identity", || {
            filesystem::file_identity(&final_file)
        }),
    );
    h.require(key, "positive-full-identity", after == identity);
    exact_bytes(h, key, &saved, &seed());
    drop(final_file);
    h.check(key, "positive-released");
    fixture.close();
    rows.complete(key);

    let key = "engine:W-OPEN-NOWRITE";
    let fixture = Fixture::new(h, key, &seed_tail());
    let mut held = h.need(
        key,
        "held-read",
        h.io(key, "held-read", || {
            filesystem::open_private_read_stable(&fixture.path)
        }),
    );
    let held_identity = h.need(
        key,
        "held-full-identity",
        h.io(key, "held-full-identity", || {
            filesystem::file_identity(&held)
        }),
    );
    h.require(
        key,
        "held-owned-object",
        fixture.owned.borrow().get(&fixture.path) == Some(&held_identity),
    );
    let result = h.io(key, "public-repair-refusal", || {
        repair_partial(&fixture.path)
    });
    let Err(error) = result else {
        panic!("output-qualification key={key} phase=write-not-refused");
    };
    h.require(
        key,
        "native-sharing",
        matches!(error.raw_os_error(), Some(32 | 33)),
    );
    exact_reader(h, key, &mut *held, &seed_tail());
    drop(held);
    h.check(key, "exact-blocker-released");
    let repair = h.need(
        key,
        "same-source-positive",
        h.io(key, "same-source-positive", || {
            repair_partial(&fixture.path)
        }),
    );
    h.require(
        key,
        "positive-counters",
        repair
            == OutputStats {
                retained_bytes: 6,
                physical_bytes: 39,
                ..OutputStats::default()
            },
    );
    let final_reader = h.need(
        key,
        "positive-object-open",
        h.io(key, "positive-object-open", || {
            filesystem::open_private(&fixture.path, OpenOptions::new().read(true))
        }),
    );
    let final_identity = h.need(
        key,
        "positive-full-object",
        h.io(key, "positive-full-object", || {
            filesystem::file_identity(&final_reader)
        }),
    );
    h.require(
        key,
        "same-owned-positive-object",
        final_identity == held_identity,
    );
    drop(final_reader);
    h.check(key, "positive-object-released");
    exact_bytes(h, key, &fixture.path, &seed());
    fixture.close();
    rows.complete(key);
    rows.finish(2);
}
