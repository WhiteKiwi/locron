use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

/// Length in bytes of the fixed frame header.
pub const FRAME_HEADER_LEN: usize = 25;
/// Maximum payload length in bytes carried by a single frame.
pub const MAX_FRAME_PAYLOAD: usize = 1024 * 1024;
const MAGIC: &[u8; 8] = b"LOCRON\0\x01";

/// Output stream channel a frame's payload belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameChannel {
    /// Standard output of the executed process.
    Stdout = 1,
    /// Standard error of the executed process.
    Stderr = 2,
    /// Body payload distinct from the process streams.
    Body = 3,
}

/// One framed output chunk from an attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// Channel the payload belongs to.
    pub channel: FrameChannel,
    /// Zero-based frame number within the file.
    pub sequence: u64,
    /// Elapsed time in microseconds at the moment the payload was captured.
    pub elapsed_us: u64,
    /// Raw payload bytes carried by the frame.
    pub payload: Vec<u8>,
}

/// Result of truncating a partial frame file back to its complete frames.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputRepair {
    /// Number of complete frames retained.
    pub frames: u64,
    /// Total payload bytes across the retained frames.
    pub payload_bytes: u64,
    /// File length in bytes after truncation.
    pub physical_bytes: u64,
    /// Number of trailing bytes removed.
    pub tail_removed: u64,
}

/// Appends an owner-only, framed output stream to a newly created file.
pub struct FrameWriter {
    file: File,
    sequence: u64,
    physical: u64,
}
impl FrameWriter {
    /// Creates a new frame file with the magic header, failing if it exists.
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(MAGIC)?;
        Ok(Self {
            file,
            sequence: 0,
            physical: MAGIC.len() as u64,
        })
    }
    /// Appends the payload as one or more frames, each at most
    /// [`MAX_FRAME_PAYLOAD`] bytes long.
    pub fn write(
        &mut self,
        channel: FrameChannel,
        elapsed_us: u64,
        payload: &[u8],
    ) -> io::Result<()> {
        for chunk in payload.chunks(MAX_FRAME_PAYLOAD) {
            let len = u32::try_from(chunk.len())
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
            let mut hasher = crc32fast::Hasher::new();
            hasher.update(&[channel as u8]);
            hasher.update(&self.sequence.to_le_bytes());
            hasher.update(&elapsed_us.to_le_bytes());
            hasher.update(&len.to_le_bytes());
            hasher.update(chunk);
            let crc = hasher.finalize();
            self.file.write_all(&[channel as u8])?;
            self.file.write_all(&self.sequence.to_le_bytes())?;
            self.file.write_all(&elapsed_us.to_le_bytes())?;
            self.file.write_all(&len.to_le_bytes())?;
            self.file.write_all(&crc.to_le_bytes())?;
            self.file.write_all(chunk)?;
            self.sequence += 1;
            self.physical += FRAME_HEADER_LEN as u64 + chunk.len() as u64;
        }
        Ok(())
    }
    /// Flushes and fsyncs the file, returning its physical length in bytes.
    pub fn sync(&mut self) -> io::Result<u64> {
        self.file.flush()?;
        self.file.sync_all()?;
        Ok(self.physical)
    }
}

/// Reads back a framed stream produced by [`FrameWriter`].
pub struct FrameReader {
    file: File,
    next: u64,
    valid: u64,
}
impl FrameReader {
    /// Opens a frame file and validates its magic header.
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let mut magic = [0; 8];
        file.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad output header",
            ));
        }
        Ok(Self {
            file,
            next: 0,
            valid: 8,
        })
    }
    /// Reads the next frame, returning `None` at end of stream.
    pub fn next_frame(&mut self) -> io::Result<Option<Frame>> {
        let mut header = [0; FRAME_HEADER_LEN];
        match self.file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(error) => return Err(error),
        }
        let channel = match header[0] {
            1 => FrameChannel::Stdout,
            2 => FrameChannel::Stderr,
            3 => FrameChannel::Body,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "bad channel")),
        };
        // The slices below are compile-time constants of the exact array length,
        // so the conversions cannot fail and the default is never used.
        let sequence = u64::from_le_bytes(header[1..9].try_into().unwrap_or_default());
        let elapsed_us = u64::from_le_bytes(header[9..17].try_into().unwrap_or_default());
        let len = u32::from_le_bytes(header[17..21].try_into().unwrap_or_default()) as usize;
        let expected = u32::from_le_bytes(header[21..25].try_into().unwrap_or_default());
        if len > MAX_FRAME_PAYLOAD || sequence != self.next {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid frame"));
        }
        let mut payload = vec![0; len];
        self.file.read_exact(&mut payload)?;
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&header[..21]);
        hasher.update(&payload);
        if hasher.finalize() != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "checksum mismatch",
            ));
        }
        self.next += 1;
        self.valid += FRAME_HEADER_LEN as u64 + len as u64;
        Ok(Some(Frame {
            channel,
            sequence,
            elapsed_us,
            payload,
        }))
    }
}

/// Truncates a partial frame file to its last complete frame and reports the repair.
pub fn repair_partial(path: &Path) -> io::Result<OutputRepair> {
    let original = std::fs::metadata(path)?.len();
    let mut reader = FrameReader::open(path)?;
    let mut repair = OutputRepair {
        physical_bytes: 8,
        ..OutputRepair::default()
    };
    while let Ok(Some(frame)) = reader.next_frame() {
        repair.frames += 1;
        repair.payload_bytes += frame.payload.len() as u64;
        repair.physical_bytes = reader.valid;
    }
    repair.tail_removed = original.saturating_sub(repair.physical_bytes);
    drop(reader);
    let file = OpenOptions::new().write(true).open(path)?;
    file.set_len(repair.physical_bytes)?;
    file.sync_all()?;
    Ok(repair)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_tail_is_removed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("x.partial");
        let mut writer = FrameWriter::create(&path).unwrap();
        writer.write(FrameChannel::Stdout, 1, b"ok").unwrap();
        writer.sync().unwrap();
        drop(writer);
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"broken")
            .unwrap();
        let repair = repair_partial(&path).unwrap();
        assert_eq!(repair.frames, 1);
        assert_eq!(repair.tail_removed, 6);
    }
}
