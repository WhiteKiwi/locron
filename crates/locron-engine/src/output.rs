//! Versioned, checksummed attempt-output framing.

use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crc32fast::Hasher;
use tokio::io::{AsyncWriteExt, BufWriter};

const MAGIC: &[u8; 8] = b"LOCRON\0\x01";
const FRAME_HEADER_LEN: usize = 25;
const MAX_FRAME_LEN: usize = 1024 * 1024;

/// Origin of one captured output frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Channel {
    /// Child standard output.
    Stdout = 1,
    /// Child standard error.
    Stderr = 2,
    /// HTTP response body.
    Body = 3,
}

impl TryFrom<u8> for Channel {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Stdout),
            2 => Ok(Self::Stderr),
            3 => Ok(Self::Body),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unknown output channel",
            )),
        }
    }
}

/// One decoded frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// Source stream.
    pub channel: Channel,
    /// File-local sequence.
    pub sequence: u64,
    /// Monotonic elapsed time since attempt start.
    pub elapsed_micros: u64,
    /// Uninterpreted target bytes.
    pub payload: Vec<u8>,
}

/// Capture counters persisted by the store.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct OutputStats {
    /// Target payload retained in frames.
    pub retained_bytes: u64,
    /// Target payload drained but not retained.
    pub discarded_bytes: u64,
    /// Physical file size, including framing.
    pub physical_bytes: u64,
    /// Whether capture reached its allowance.
    pub truncated: bool,
}

/// Asynchronous single-writer attempt output file.
pub struct OutputWriter {
    path: PathBuf,
    file: BufWriter<tokio::fs::File>,
    next_sequence: u64,
    limit: u64,
    stats: OutputStats,
}

impl OutputWriter {
    /// Creates a new private partial file and writes its format header.
    pub async fn create(path: impl AsRef<Path>, limit: u64) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut options = tokio::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            options.mode(0o600);
        }
        let mut file = BufWriter::new(options.open(&path).await?);
        file.write_all(MAGIC).await?;
        Ok(Self {
            path,
            file,
            next_sequence: 0,
            limit,
            stats: OutputStats {
                physical_bytes: MAGIC.len() as u64,
                ..OutputStats::default()
            },
        })
    }

    /// Appends captured bytes, retaining only the available allowance.
    pub async fn write(
        &mut self,
        channel: Channel,
        elapsed: Duration,
        payload: &[u8],
    ) -> io::Result<()> {
        let available = self.limit.saturating_sub(self.stats.retained_bytes);
        let retained_len =
            usize::try_from(available.min(payload.len() as u64)).unwrap_or(payload.len());
        if retained_len > 0 {
            for chunk in payload[..retained_len].chunks(MAX_FRAME_LEN) {
                self.write_frame(channel, elapsed, chunk).await?;
            }
        }
        let discarded = payload.len().saturating_sub(retained_len) as u64;
        self.stats.discarded_bytes = self.stats.discarded_bytes.saturating_add(discarded);
        self.stats.truncated |= discarded > 0;
        Ok(())
    }

    async fn write_frame(
        &mut self,
        channel: Channel,
        elapsed: Duration,
        payload: &[u8],
    ) -> io::Result<()> {
        let elapsed_micros = elapsed.as_micros().min(u128::from(u64::MAX)) as u64;
        let length = u32::try_from(payload.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "frame too large"))?;
        let mut checksum = Hasher::new();
        checksum.update(&[channel as u8]);
        checksum.update(&self.next_sequence.to_le_bytes());
        checksum.update(&elapsed_micros.to_le_bytes());
        checksum.update(&length.to_le_bytes());
        checksum.update(payload);
        let crc = checksum.finalize();
        self.file.write_all(&[channel as u8]).await?;
        self.file
            .write_all(&self.next_sequence.to_le_bytes())
            .await?;
        self.file.write_all(&elapsed_micros.to_le_bytes()).await?;
        self.file.write_all(&length.to_le_bytes()).await?;
        self.file.write_all(&crc.to_le_bytes()).await?;
        self.file.write_all(payload).await?;
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.stats.retained_bytes = self
            .stats
            .retained_bytes
            .saturating_add(payload.len() as u64);
        self.stats.physical_bytes = self
            .stats
            .physical_bytes
            .saturating_add(FRAME_HEADER_LEN as u64)
            .saturating_add(payload.len() as u64);
        Ok(())
    }

    /// Flushes and syncs the partial file, then atomically renames it.
    pub async fn finalize(mut self, final_path: impl AsRef<Path>) -> io::Result<OutputStats> {
        self.file.flush().await?;
        self.file.get_ref().sync_all().await?;
        drop(self.file);
        tokio::fs::rename(&self.path, final_path).await?;
        Ok(self.stats)
    }

    /// Flushes without renaming, leaving a recoverable partial file.
    pub async fn flush(&mut self) -> io::Result<()> {
        self.file.flush().await
    }

    /// Current capture counters.
    #[must_use]
    pub const fn stats(&self) -> OutputStats {
        self.stats
    }
}

/// Reads and validates all complete frames in a file.
pub fn read_frames(path: impl AsRef<Path>) -> io::Result<Vec<Frame>> {
    let mut file = std::fs::File::open(path)?;
    read_valid_frames(&mut file).map(|(frames, _)| frames)
}

/// Truncates an interrupted file after the last complete valid frame.
pub fn repair_partial(path: impl AsRef<Path>) -> io::Result<OutputStats> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    let (frames, valid_len) = read_valid_frames(&mut file)?;
    file.set_len(valid_len)?;
    file.sync_all()?;
    Ok(OutputStats {
        retained_bytes: frames.iter().map(|frame| frame.payload.len() as u64).sum(),
        physical_bytes: valid_len,
        ..OutputStats::default()
    })
}

fn read_valid_frames(file: &mut std::fs::File) -> io::Result<(Vec<Frame>, u64)> {
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid output magic",
        ));
    }
    let mut frames = Vec::new();
    let mut valid_len = MAGIC.len() as u64;
    loop {
        let mut header = [0_u8; FRAME_HEADER_LEN];
        match file.read_exact(&mut header) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        }
        let channel = Channel::try_from(header[0])?;
        let sequence = u64::from_le_bytes(header[1..9].try_into().expect("fixed slice"));
        let elapsed_micros = u64::from_le_bytes(header[9..17].try_into().expect("fixed slice"));
        let length = u32::from_le_bytes(header[17..21].try_into().expect("fixed slice")) as usize;
        let expected_crc = u32::from_le_bytes(header[21..25].try_into().expect("fixed slice"));
        if length > MAX_FRAME_LEN {
            break;
        }
        let mut payload = vec![0_u8; length];
        if file.read_exact(&mut payload).is_err() {
            break;
        }
        let mut checksum = Hasher::new();
        checksum.update(&header[..21]);
        checksum.update(&payload);
        if checksum.finalize() != expected_crc || sequence != frames.len() as u64 {
            break;
        }
        valid_len = valid_len.saturating_add(FRAME_HEADER_LEN as u64 + length as u64);
        frames.push(Frame {
            channel,
            sequence,
            elapsed_micros,
            payload,
        });
    }
    file.seek(SeekFrom::Start(valid_len))?;
    Ok((frames, valid_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[tokio::test]
    async fn preserves_interleaving_and_binary_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("a.partial");
        let final_path = temp.path().join("a.log");
        let mut writer = OutputWriter::create(&partial, 100).await.unwrap();
        writer
            .write(Channel::Stdout, Duration::from_micros(1), b"a\0")
            .await
            .unwrap();
        writer
            .write(Channel::Stderr, Duration::from_micros(2), b"b")
            .await
            .unwrap();
        let stats = writer.finalize(&final_path).await.unwrap();
        assert_eq!(stats.retained_bytes, 3);
        let frames = read_frames(final_path).unwrap();
        assert_eq!(frames[0].payload, b"a\0");
        assert_eq!(frames[1].channel, Channel::Stderr);
    }

    #[tokio::test]
    async fn truncates_capture_but_accounts_discarded_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("a.partial");
        let final_path = temp.path().join("a.log");
        let mut writer = OutputWriter::create(&partial, 3).await.unwrap();
        writer
            .write(Channel::Stdout, Duration::ZERO, b"abcdef")
            .await
            .unwrap();
        let stats = writer.finalize(&final_path).await.unwrap();
        assert_eq!(stats.retained_bytes, 3);
        assert_eq!(stats.discarded_bytes, 3);
        assert!(stats.truncated);
        assert_eq!(read_frames(final_path).unwrap()[0].payload, b"abc");
    }

    #[tokio::test]
    async fn repairs_incomplete_tail() {
        let temp = tempfile::tempdir().unwrap();
        let partial = temp.path().join("a.partial");
        let mut writer = OutputWriter::create(&partial, 100).await.unwrap();
        writer
            .write(Channel::Stdout, Duration::ZERO, b"ok")
            .await
            .unwrap();
        writer.flush().await.unwrap();
        drop(writer);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&partial)
            .unwrap();
        file.write_all(&[1, 2, 3]).unwrap();
        drop(file);
        let stats = repair_partial(&partial).unwrap();
        assert_eq!(stats.retained_bytes, 2);
        assert_eq!(read_frames(partial).unwrap().len(), 1);
    }
}
