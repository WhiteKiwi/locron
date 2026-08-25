//! `locron self-update` — replace the running binary with the latest stable release.
//!
//! The latest release is resolved through the GitHub releases API, the matching
//! tarball and the release's `SHA256SUMS.txt` are downloaded, the tarball
//! checksum is verified before anything is touched, and the binary is replaced
//! with one temp file plus an atomic rename in the executable's directory. A
//! failed or interrupted update leaves the existing binary installed and
//! working. The Homebrew marker is honored first; every other installation
//! must carry the exact standalone-installer receipt beside the executable.

use std::env;
use std::error::Error as StdError;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

const ENV_API_BASE: &str = "LOCRON_UPDATE_API_BASE";
const ENV_ASSET_BASE: &str = "LOCRON_UPDATE_ASSET_BASE";
const API_BASE: &str = "https://api.github.com";
const ASSET_BASE: &str = "https://github.com/WhiteKiwi/locron";
/// Package-manager marker, relative to the canonicalized executable directory.
const MANAGED_MARKER: &str = "../lib/.disable-self-update";
const INSTALL_RECEIPT: &str = ".locron-install-receipt-v1";
const INSTALL_RECEIPT_PAYLOAD: &[u8] = b"locron.install/v1\nstandalone\n";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const CHECKSUM_LEN: usize = 64;

/// Outcome of a self-update attempt.
#[derive(Debug)]
pub(crate) struct UpdateOutcome {
    pub current_version: String,
    pub new_version: String,
    pub updated: bool,
    /// Best-effort post-replace service-registration failures; the update
    /// itself stays successful.
    pub warnings: Vec<String>,
}

/// Stable self-update failure categories.
#[derive(Debug)]
pub(crate) enum SelfUpdateError {
    /// The running binary was built for a platform with no published release.
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },
    /// A package-manager marker next to the executable refuses self-update.
    ManagedInstall,
    /// No exact standalone-installer receipt authorizes replacing this binary.
    UnownedInstall,
    /// The GitHub API rate limit was exceeded.
    RateLimited,
    /// The API or an asset could not be reached or returned an error status.
    Network(String),
    /// The release document is malformed or misses a required asset or entry.
    ReleaseMetadata(String),
    /// The downloaded tarball does not match the published checksum.
    ChecksumMismatch {
        asset: String,
        expected: String,
        actual: String,
    },
    /// Filesystem failure while extracting or replacing the binary.
    Io(String),
}

impl fmt::Display for SelfUpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SelfUpdateError::UnsupportedPlatform { os, arch } => write!(
                formatter,
                "locron does not publish a {os}/{arch} build; self-update supports aarch64 and \
                 x86_64 on macOS and glibc Linux"
            ),
            SelfUpdateError::ManagedInstall => write!(
                formatter,
                "this locron is installed by a package manager and self-update is disabled; \
                 update it with 'brew upgrade locron'"
            ),
            SelfUpdateError::UnownedInstall => write!(
                formatter,
                "self-update is available only for a standalone installer-owned locron; Cargo \
                 users should run 'cargo install --locked locron', older script installations \
                 should rerun the standalone installer once to adopt the receipt, and other \
                 installations should use their installation channel"
            ),
            SelfUpdateError::RateLimited => write!(
                formatter,
                "the GitHub API rate limit was exceeded (60 requests/hour unauthenticated); \
                 wait a while and retry"
            ),
            SelfUpdateError::Network(message) => write!(
                formatter,
                "failed to reach the locron release server: {message}; check the network \
                 connection and retry"
            ),
            SelfUpdateError::ReleaseMetadata(message) => {
                write!(formatter, "the latest release is incomplete: {message}")
            }
            SelfUpdateError::ChecksumMismatch {
                asset,
                expected,
                actual,
            } => write!(
                formatter,
                "checksum mismatch for {asset}: expected {expected} but downloaded {actual}; \
                 the download may be corrupted — re-run self-update"
            ),
            SelfUpdateError::Io(message) => write!(formatter, "self-update failed: {message}"),
        }
    }
}

impl StdError for SelfUpdateError {}

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
}

/// Resolve the latest release and replace the running binary when it is newer.
pub(crate) async fn update(state_dir: &Path) -> Result<UpdateOutcome> {
    let current_version = env!("CARGO_PKG_VERSION");
    require_standalone_install()?;
    let target = detect_target()?;

    let client = reqwest::Client::builder()
        .user_agent(format!("locron/{current_version} self-update"))
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| SelfUpdateError::Network(error.to_string()))?;

    let latest = fetch_latest(&client).await?;
    let new_version = latest
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&latest.tag_name);

    if version_at_least(current_version, new_version) {
        return Ok(UpdateOutcome {
            current_version: current_version.to_owned(),
            new_version: new_version.to_owned(),
            updated: false,
            warnings: Vec::new(),
        });
    }

    let tarball_name = format!("locron-v{new_version}-{target}.tar.gz");
    require_asset(&latest, &tarball_name)?;
    require_asset(&latest, "SHA256SUMS.txt")?;

    let asset_base = env::var(ENV_ASSET_BASE)
        .unwrap_or_else(|_| ASSET_BASE.to_owned())
        .trim_end_matches('/')
        .to_owned();
    let release_dir = format!("{asset_base}/releases/download/{}", latest.tag_name);
    let tarball_url = format!("{release_dir}/{tarball_name}");
    let sums_url = format!("{release_dir}/SHA256SUMS.txt");

    let sums = String::from_utf8(download_bytes(&client, &sums_url).await?)
        .context("SHA256SUMS.txt is not UTF-8")?;
    let expected = checksum_for(&sums, &tarball_name)?;
    let tarball = download_bytes(&client, &tarball_url).await?;
    let actual = sha256_hex(&tarball);
    if actual != expected {
        return Err(SelfUpdateError::ChecksumMismatch {
            asset: tarball_name,
            expected,
            actual,
        }
        .into());
    }

    let temp_dir = TempDir::create()?;
    extract_archive(&tarball, temp_dir.path())?;
    let binary = locate_binary(temp_dir.path())?;
    let binary = fs::read(binary).context("cannot read the extracted binary")?;
    let executable = canonical_executable()?;
    replace_binary(&binary, &executable)?;

    // The update is complete; registering the new binary as a login service is
    // best-effort and must never turn a successful update into a failure. A
    // registered dashboard service is refreshed the same way, using the same
    // pre-replace canonical-path capture.
    let mut warnings = register_service(&executable);
    warnings.extend(register_dashboard(&executable, state_dir));

    Ok(UpdateOutcome {
        current_version: current_version.to_owned(),
        new_version: new_version.to_owned(),
        updated: true,
        warnings,
    })
}

/// Register the freshly replaced binary as a login service by running
/// `service install` on the pre-captured executable path, which re-reads its
/// own location and refreshes an existing registration or performs a first
/// registration. The child inherits this process's environment; its output is
/// captured so the update envelope stays clean. Failures are returned as
/// warnings, never as errors.
fn register_service(executable: &Path) -> Vec<String> {
    let output = match Command::new(executable)
        .args(["service", "install"])
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return vec![format!(
                "could not run 'locron service install' after update: {error}"
            )];
        }
    };
    if !output.status.success() {
        return vec![format!(
            "could not register locron as a login service after update (exit {}); \
             run 'locron service install' to retry",
            output.status.code().unwrap_or(-1)
        )];
    }
    Vec::new()
}

/// Refresh a registered dashboard service onto the freshly replaced binary.
///
/// Only a dashboard the operator enabled is touched: `dashboard status --json`
/// reports the registration, and the refresh runs `dashboard enable` (the
/// idempotent register/refresh/start flow) exactly when one exists. The child
/// inherits this process's environment; its output is captured so the update
/// envelope stays clean. Failures are returned as warnings, never as errors.
fn register_dashboard(executable: &Path, state_dir: &Path) -> Vec<String> {
    let status_output = match Command::new(executable)
        .args(["dashboard", "status", "--state-dir"])
        .arg(state_dir)
        .arg("--json")
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return vec![format!(
                "could not run 'locron dashboard status' after update: {error}"
            )];
        }
    };
    if !status_output.status.success() {
        return vec![format!(
            "could not check the dashboard registration after update (exit {}); \
             run 'locron dashboard status' to retry",
            status_output.status.code().unwrap_or(-1)
        )];
    }
    let envelope: Value = match serde_json::from_slice(&status_output.stdout) {
        Ok(envelope) => envelope,
        Err(error) => {
            return vec![format!(
                "could not read 'locron dashboard status' output after update: {error}"
            )];
        }
    };
    match envelope
        .pointer("/data/registered")
        .and_then(Value::as_bool)
    {
        Some(true) => {}
        Some(false) => {
            // The dashboard was never enabled; nothing to refresh.
            return Vec::new();
        }
        None => {
            return vec![
                "could not read 'locron dashboard status' after update: data.registered must be a boolean; the dashboard service was not changed"
                    .to_owned(),
            ];
        }
    }
    let output = match Command::new(executable)
        .args(["dashboard", "enable", "--state-dir"])
        .arg(state_dir)
        .output()
    {
        Ok(output) => output,
        Err(error) => {
            return vec![format!(
                "could not run 'locron dashboard enable' after update: {error}"
            )];
        }
    };
    if !output.status.success() {
        return vec![format!(
            "could not refresh the dashboard service after update (exit {}); \
             run 'locron dashboard enable' to retry",
            output.status.code().unwrap_or(-1)
        )];
    }
    Vec::new()
}

/// Map the running platform to the published target triple, refusing any
/// platform without an official build.
fn detect_target() -> Result<String> {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let target = match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", _) if cfg!(target_env = "musl") => {
            return Err(SelfUpdateError::UnsupportedPlatform {
                os,
                arch: "musl linux",
            }
            .into());
        }
        _ => {
            return Err(SelfUpdateError::UnsupportedPlatform { os, arch }.into());
        }
    };
    Ok(target.to_owned())
}

/// Require positive standalone ownership, after honoring Homebrew's marker.
fn require_standalone_install() -> Result<()> {
    let executable = canonical_executable()?;
    let directory = executable.parent().unwrap_or_else(|| Path::new("/"));
    let marker = directory.join(MANAGED_MARKER);
    if marker.exists() {
        return Err(SelfUpdateError::ManagedInstall.into());
    }
    let receipt = directory.join(INSTALL_RECEIPT);
    let metadata = fs::symlink_metadata(&receipt).map_err(|_| SelfUpdateError::UnownedInstall)?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(SelfUpdateError::UnownedInstall.into());
    }
    let payload = fs::read(&receipt).map_err(|_| SelfUpdateError::UnownedInstall)?;
    if payload != INSTALL_RECEIPT_PAYLOAD {
        return Err(SelfUpdateError::UnownedInstall.into());
    }
    Ok(())
}

/// Fetch and parse the latest release document from the GitHub API.
async fn fetch_latest(client: &reqwest::Client) -> Result<LatestRelease> {
    let api_base = env::var(ENV_API_BASE)
        .unwrap_or_else(|_| API_BASE.to_owned())
        .trim_end_matches('/')
        .to_owned();
    let url = format!("{api_base}/repos/WhiteKiwi/locron/releases/latest");
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| SelfUpdateError::Network(error.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .unwrap_or_default()
            .to_ascii_lowercase();
        if is_rate_limited(status, &body) {
            return Err(SelfUpdateError::RateLimited.into());
        }
        return Err(SelfUpdateError::Network(format!("GET {url} returned HTTP {status}")).into());
    }
    response
        .json::<LatestRelease>()
        .await
        .map_err(|error| SelfUpdateError::ReleaseMetadata(error.to_string()).into())
}

/// A rate-limit response is 403/429 with the remaining-requests header at zero
/// or a message naming the limit.
fn is_rate_limited(status: StatusCode, body: &str) -> bool {
    (status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS)
        && (body.contains("rate limit") || body.contains("ratelimit"))
}

/// Download a whole asset into memory.
async fn download_bytes(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| SelfUpdateError::Network(error.to_string()))?;
    if !response.status().is_success() {
        return Err(SelfUpdateError::Network(format!(
            "GET {url} returned HTTP {}",
            response.status()
        ))
        .into());
    }
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| SelfUpdateError::Network(error.to_string()).into())
}

/// Extract the published checksum for an asset from the release checksum file.
fn checksum_for(sums: &str, asset: &str) -> Result<String> {
    for line in sums.lines() {
        let mut fields = line.split_whitespace();
        let (Some(hash), Some(name)) = (fields.next(), fields.next()) else {
            continue;
        };
        if name != asset {
            continue;
        }
        if hash.len() != CHECKSUM_LEN || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(SelfUpdateError::ReleaseMetadata(format!(
                "malformed checksum entry for {asset}"
            ))
            .into());
        }
        return Ok(hash.to_owned());
    }
    Err(SelfUpdateError::ReleaseMetadata(format!(
        "no checksum entry for {asset} in SHA256SUMS.txt"
    ))
    .into())
}

/// Unpack the release archive into the temporary directory.
fn extract_archive(tarball: &[u8], destination: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(tarball);
    let mut archive = tar::Archive::new(decoder);
    archive
        .unpack(destination)
        .map_err(|error| {
            SelfUpdateError::Io(format!("cannot extract the release archive: {error}"))
        })
        .map_err(Into::into)
}

/// Locate the `locron` binary inside the unpacked archive directory.
fn locate_binary(root: &Path) -> Result<PathBuf> {
    for entry in fs::read_dir(root).map_err(|error| {
        SelfUpdateError::Io(format!("cannot read the extraction directory: {error}"))
    })? {
        let entry = entry.map_err(|error| {
            SelfUpdateError::Io(format!("cannot read the extraction directory: {error}"))
        })?;
        let candidate = if entry.path().is_dir() {
            entry.path().join("locron")
        } else {
            entry.path()
        };
        if candidate.is_file() && candidate.file_name().is_some_and(|name| name == "locron") {
            return Ok(candidate);
        }
    }
    Err(SelfUpdateError::ReleaseMetadata(
        "the release archive does not contain a locron binary".to_owned(),
    )
    .into())
}

/// Write the new binary to a temp file next to the running executable and
/// atomically rename it over the executable. The running process keeps its old
/// inode; the next invocation runs the new binary.
fn replace_binary(binary: &[u8], executable: &Path) -> Result<()> {
    let directory = executable
        .parent()
        .ok_or_else(|| anyhow!("cannot determine the executable directory"))?;
    let mode = fs::metadata(executable)
        .map_err(|error| SelfUpdateError::Io(format!("cannot read the current binary: {error}")))?
        .permissions();
    let temp = create_temp_in(directory)?;
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .open(&temp)
            .map_err(|error| {
                SelfUpdateError::Io(format!("cannot write the replacement binary: {error}"))
            })?;
        file.write_all(binary).map_err(|error| {
            SelfUpdateError::Io(format!("cannot write the replacement binary: {error}"))
        })?;
        file.sync_all().map_err(|error| {
            SelfUpdateError::Io(format!("cannot write the replacement binary: {error}"))
        })?;
        // Close the write handle before the rename: while it stays open the
        // replacement's inode is writable at the executable path, which on
        // Linux makes any concurrent exec or write-open of that path fail
        // with ETXTBSY ("Text file busy") until this process exits.
        drop(file);
        fs::set_permissions(&temp, mode).map_err(|error| {
            SelfUpdateError::Io(format!(
                "cannot set the replacement binary permissions: {error}"
            ))
        })?;
        fs::rename(&temp, executable).map_err(|error| {
            SelfUpdateError::Io(format!("cannot replace {}: {error}", executable.display()))
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// The canonicalized path of the running executable.
fn canonical_executable() -> Result<PathBuf> {
    let executable = env::current_exe().map_err(|error| {
        SelfUpdateError::Io(format!("cannot locate the current executable: {error}"))
    })?;
    fs::canonicalize(&executable)
        .map_err(|error| {
            SelfUpdateError::Io(format!("cannot locate the current executable: {error}"))
        })
        .map_err(Into::into)
}

/// A private temporary directory removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn create() -> Result<Self> {
        let base = env::temp_dir();
        for attempt in 0..100_u32 {
            let candidate = base.join(format!(
                "locron-self-update-{}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self(candidate)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(SelfUpdateError::Io(format!(
                        "cannot create a temporary directory: {error}"
                    ))
                    .into());
                }
            }
        }
        Err(SelfUpdateError::Io("cannot create a temporary directory".to_owned()).into())
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A unique temp file in `directory` created with `create_new` semantics.
fn create_temp_in(directory: &Path) -> Result<PathBuf> {
    for attempt in 0..100_u32 {
        let candidate = directory.join(format!(
            ".locron-update-{}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(SelfUpdateError::Io(format!(
                    "cannot create a temporary file in {}: {error}",
                    directory.display()
                ))
                .into());
            }
        }
    }
    Err(SelfUpdateError::Io(format!(
        "cannot create a temporary file in {}",
        directory.display()
    ))
    .into())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// True when `current` is the same version as or newer than `other`.
fn version_at_least(current: &str, other: &str) -> bool {
    match (parse_version(current), parse_version(other)) {
        (Some(current), Some(other)) => {
            for (a, b) in current.iter().zip(other.iter()) {
                if a != b {
                    return a > b;
                }
            }
            current.len() >= other.len()
        }
        _ => current >= other,
    }
}

fn parse_version(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// Require the release to publish the named asset.
fn require_asset(release: &LatestRelease, name: &str) -> Result<()> {
    if release.assets.iter().any(|asset| asset.name == name) {
        Ok(())
    } else {
        Err(
            SelfUpdateError::ReleaseMetadata(format!("the latest release does not publish {name}"))
                .into(),
        )
    }
}
