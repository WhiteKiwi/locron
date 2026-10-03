//! Strict in-memory verification of the unsigned Windows release archive.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use anyhow::{Context, Result, ensure};
use zip::read::HasZipMetadata;

use super::sha256_hex;

pub(super) const FILES: [&str; 4] = ["locron.exe", "README.md", "LICENSE-MIT", "LICENSE-APACHE"];
pub(super) const PAIRED_FILES: [&str; 5] = [
    "locron.exe",
    "locron-service-launcher.exe",
    "README.md",
    "LICENSE-MIT",
    "LICENSE-APACHE",
];

/// Explicit internal layouts; the one-image route is historical test evidence only.
pub(super) enum Inventory {
    SingleFixture,
    Paired,
}

impl Inventory {
    fn names(&self) -> &'static [&'static str] {
        match self {
            Self::SingleFixture => &FILES,
            Self::Paired => &PAIRED_FILES,
        }
    }
}

pub(super) const LIMIT: usize = 64 * 1024 * 1024;
const SYSTEM_DLLS: &[&str] = &[
    "api-ms-win-core-synch-l1-2-0.dll",
    "advapi32.dll",
    "bcrypt.dll",
    "bcryptprimitives.dll",
    "cfgmgr32.dll",
    "combase.dll",
    "crypt32.dll",
    "dbghelp.dll",
    "gdi32.dll",
    "iphlpapi.dll",
    "kernel32.dll",
    "kernelbase.dll",
    "netapi32.dll",
    "normaliz.dll",
    "ntdll.dll",
    "ole32.dll",
    "oleaut32.dll",
    "psapi.dll",
    "rpcrt4.dll",
    "sechost.dll",
    "secur32.dll",
    "shell32.dll",
    "shlwapi.dll",
    "ucrtbase.dll",
    "user32.dll",
    "userenv.dll",
    "version.dll",
    "winhttp.dll",
    "winmm.dll",
    "wintrust.dll",
    "ws2_32.dll",
    "wtsapi32.dll",
];

/// Verified exact inventory, retained in memory until guarded file creation.
pub(super) struct VerifiedArchive {
    pub files: BTreeMap<String, Vec<u8>>,
    pub binary_sha256: String,
}

fn bytes_at(bytes: &[u8], offset: usize, size: usize) -> Result<&[u8]> {
    let end = offset
        .checked_add(size)
        .ok_or_else(|| anyhow::anyhow!("header offset overflow"))?;
    bytes
        .get(offset..end)
        .ok_or_else(|| anyhow::anyhow!("truncated archive/executable header"))
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_le_bytes(bytes_at(bytes, offset, 2)?.try_into()?))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(bytes_at(bytes, offset, 4)?.try_into()?))
}

/// Validate stable numeric versions before using them in a filename or URL.
pub(super) fn valid_version(version: &str) -> bool {
    let parts: Vec<_> = version.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && (part.len() == 1 || !part.starts_with('0'))
                && part.bytes().all(|byte| byte.is_ascii_digit())
                && part.parse::<u64>().is_ok()
        })
}

/// Verify both catalog and local records, including duplicate catalog entries
/// that a ZIP reader could otherwise normalize into one filename.
fn catalog(bytes: &[u8], expected: &BTreeSet<String>) -> Result<()> {
    ensure!(
        bytes.len() <= LIMIT && bytes.len() >= 22,
        "invalid Windows ZIP size"
    );
    let end = bytes.len() - 22;
    ensure!(
        u32_at(bytes, end)? == 0x0605_4b50 && u16_at(bytes, end + 20)? == 0,
        "Windows ZIP must end at its uncommented catalog"
    );
    ensure!(
        u16_at(bytes, end + 4)? == 0
            && u16_at(bytes, end + 6)? == 0
            && usize::from(u16_at(bytes, end + 8)?) == expected.len()
            && usize::from(u16_at(bytes, end + 10)?) == expected.len(),
        "Windows ZIP must have the exact single-disk member inventory"
    );
    let start = u32_at(bytes, end + 16)? as usize;
    ensure!(
        start.checked_add(u32_at(bytes, end + 12)? as usize) == Some(end),
        "invalid Windows ZIP catalog span"
    );
    let mut offset = start;
    let mut names = BTreeSet::new();
    let mut ranges = Vec::new();
    for _ in 0..expected.len() {
        ensure!(
            u32_at(bytes, offset)? == 0x0201_4b50,
            "invalid Windows ZIP central header"
        );
        let flags = u16_at(bytes, offset + 8)?;
        let method = u16_at(bytes, offset + 10)?;
        ensure!(
            flags & !0x0800 == 0 && matches!(method, 0 | 8),
            "encrypted or unsupported Windows ZIP member"
        );
        let size = u32_at(bytes, offset + 20)? as usize;
        let name_length = usize::from(u16_at(bytes, offset + 28)?);
        let extra = usize::from(u16_at(bytes, offset + 30)?);
        let comment = usize::from(u16_at(bytes, offset + 32)?);
        ensure!(
            extra == 0 && comment == 0 && u16_at(bytes, offset + 34)? == 0,
            "unexpected Windows ZIP metadata"
        );
        let name = std::str::from_utf8(bytes_at(bytes, offset + 46, name_length)?)?;
        ensure!(
            expected.contains(name) && names.insert(name.to_ascii_lowercase()),
            "unexpected or duplicate Windows ZIP filename"
        );
        let attributes = u32_at(bytes, offset + 38)?;
        ensure!(
            matches!((attributes >> 16) & 0xf000, 0 | 0x8000) && attributes & 0x0410 == 0,
            "Windows ZIP has linked, reparse or directory members"
        );
        let local = u32_at(bytes, offset + 42)? as usize;
        ensure!(
            u32_at(bytes, local)? == 0x0403_4b50
                && u16_at(bytes, local + 6)? == flags
                && u16_at(bytes, local + 8)? == method
                && u32_at(bytes, local + 14)? == u32_at(bytes, offset + 16)?
                && u32_at(bytes, local + 18)? as usize == size
                && u32_at(bytes, local + 22)? == u32_at(bytes, offset + 24)?
                && usize::from(u16_at(bytes, local + 26)?) == name_length
                && u16_at(bytes, local + 28)? == 0,
            "Windows ZIP local and central records disagree"
        );
        ensure!(
            bytes_at(bytes, local + 30, name_length)? == name.as_bytes(),
            "Windows ZIP local filename differs"
        );
        let local_end = local
            .checked_add(30 + name_length)
            .and_then(|at| at.checked_add(size))
            .ok_or_else(|| anyhow::anyhow!("Windows ZIP member offset overflow"))?;
        ensure!(
            local_end <= start,
            "Windows ZIP payload overlaps its catalog"
        );
        ranges.push((local, local_end));
        offset = offset
            .checked_add(46 + name_length)
            .ok_or_else(|| anyhow::anyhow!("catalog offset overflow"))?;
    }
    ensure!(
        offset == end && names.len() == expected.len(),
        "Windows ZIP has extra catalog data"
    );
    ranges.sort_unstable();
    let mut previous = 0;
    for (begin, finish) in ranges {
        ensure!(
            begin == previous,
            "Windows ZIP has overlapping or extra payload data"
        );
        previous = finish;
    }
    ensure!(
        previous == start,
        "Windows ZIP payload inventory is incomplete"
    );
    Ok(())
}

/// Check final digest before interpreting any member and never extract arbitrary paths.
pub(super) fn verify_archive(
    bytes: &[u8],
    version: &str,
    target: &str,
    expected: &str,
) -> Result<VerifiedArchive> {
    let files = read_archive(bytes, version, target, expected, Inventory::SingleFixture)?;
    let binary = files
        .get("locron.exe")
        .ok_or_else(|| anyhow::anyhow!("Windows ZIP has no executable"))?;
    verify_pe(binary, target)?;
    Ok(VerifiedArchive {
        binary_sha256: sha256_hex(binary),
        files,
    })
}

/// Shared bounded byte parser; this returns no live ownership or version/ABI authority.
pub(super) fn read_archive(
    bytes: &[u8],
    version: &str,
    target: &str,
    expected: &str,
    inventory: Inventory,
) -> Result<BTreeMap<String, Vec<u8>>> {
    ensure!(valid_version(version), "invalid Windows release version");
    ensure!(
        expected.len() == 64
            && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
            && sha256_hex(bytes) == expected.to_ascii_lowercase(),
        "Windows ZIP digest mismatch"
    );
    let root = format!("locron-v{version}-{target}/");
    let names = inventory
        .names()
        .iter()
        .map(|name| format!("{root}{name}"))
        .collect();
    catalog(bytes, &names)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        archive.len() == inventory.names().len(),
        "Windows ZIP reader inventory differs"
    );
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let member = archive.by_index(index)?;
        ensure!(
            member.is_file()
                && !member.is_symlink()
                && !member.encrypted()
                && member.get_metadata().external_attributes & 0x400 == 0,
            "unsafe Windows ZIP member"
        );
        total = total
            .checked_add(member.size())
            .ok_or_else(|| anyhow::anyhow!("ZIP size overflow"))?;
        ensure!(
            total <= LIMIT as u64,
            "Windows ZIP expands beyond its size limit"
        );
        let name = member
            .name()
            .strip_prefix(&root)
            .ok_or_else(|| anyhow::anyhow!("invalid ZIP root"))?
            .to_owned();
        let size = member.size();
        let mut data = Vec::new();
        member.take(LIMIT as u64 + 1).read_to_end(&mut data)?;
        ensure!(data.len() as u64 == size, "Windows ZIP member size differs");
        ensure!(
            inventory.names().contains(&name.as_str()) && files.insert(name, data).is_none(),
            "unexpected Windows ZIP member"
        );
    }
    Ok(files)
}

/// Inspect architecture, unsigned status and direct/delayed native dependencies.
pub(super) fn verify_pe(bytes: &[u8], target: &str) -> Result<Vec<String>> {
    ensure!(bytes_at(bytes, 0, 2)? == b"MZ", "missing Windows PE header");
    let pe = u32_at(bytes, 60)? as usize;
    ensure!(
        bytes_at(bytes, pe, 4)? == b"PE\0\0",
        "invalid Windows PE header"
    );
    let machine = match target {
        "x86_64-pc-windows-msvc" => 0x8664,
        "aarch64-pc-windows-msvc" => 0xaa64,
        _ => anyhow::bail!("unsupported native Windows target"),
    };
    ensure!(
        u16_at(bytes, pe + 4)? == machine && u16_at(bytes, pe + 22)? & 0x2002 == 2,
        "incorrect Windows executable architecture/type"
    );
    let count = usize::from(u16_at(bytes, pe + 6)?);
    ensure!((1..=96).contains(&count), "invalid PE section count");
    let optional = pe + 24;
    let optional_size = usize::from(u16_at(bytes, pe + 20)?);
    let directories = u32_at(bytes, optional + 108)? as usize;
    ensure!(
        (5..=16).contains(&directories)
            && optional_size >= 112 + directories * 8
            && u16_at(bytes, optional)? == 0x20b,
        "invalid PE32+ optional header"
    );
    bytes_at(bytes, optional, optional_size)?;
    ensure!(
        u32_at(bytes, optional + 144)? == 0 && u32_at(bytes, optional + 148)? == 0,
        "initial Windows release must be unsigned"
    );
    let table = optional + optional_size;
    bytes_at(bytes, table, count * 40)?;
    let mut sections = Vec::new();
    for index in 0..count {
        let row = table + index * 40;
        let length = u32_at(bytes, row + 8)?.max(u32_at(bytes, row + 16)?) as usize;
        let start = u32_at(bytes, row + 12)? as usize;
        let size = u32_at(bytes, row + 16)? as usize;
        let raw = u32_at(bytes, row + 20)? as usize;
        bytes_at(bytes, raw, size)?;
        sections.push((start, length, raw, size));
    }
    let headers = u32_at(bytes, optional + 60)? as usize;
    let at = |rva: usize, size: usize| -> Result<usize> {
        let end = rva
            .checked_add(size)
            .ok_or_else(|| anyhow::anyhow!("PE RVA overflow"))?;
        let mut matches: Vec<_> = sections
            .iter()
            .filter(|&&(start, length, _, raw_size)| {
                start <= rva && end - start <= length && end - start <= raw_size
            })
            .map(|&(start, _, raw, _)| raw + rva - start)
            .collect();
        if end <= headers.min(bytes.len()) {
            matches.push(rva);
        }
        ensure!(matches.len() == 1, "invalid or ambiguous PE import RVA");
        Ok(matches[0])
    };
    let mut imports = BTreeSet::new();
    for (directory, stride, name_offset) in [(1, 20, 12), (13, 32, 4)] {
        if directory >= directories {
            continue;
        }
        let rva = u32_at(bytes, optional + 112 + directory * 8)? as usize;
        let size = u32_at(bytes, optional + 116 + directory * 8)? as usize;
        if rva == 0 && size == 0 {
            continue;
        }
        ensure!(
            rva > 0 && (stride..=stride * 256).contains(&size),
            "invalid PE import directory"
        );
        let start = at(rva, size)?;
        let mut terminated = false;
        for index in 0..size / stride {
            let descriptor = bytes_at(bytes, start + index * stride, stride)?;
            if descriptor.iter().all(|&byte| byte == 0) {
                terminated = true;
                break;
            }
            ensure!(
                directory != 13 || u32_at(descriptor, 0)? == 1,
                "unsupported PE delayed-import attributes"
            );
            let name = u32_at(descriptor, name_offset)? as usize;
            let mut dll = Vec::new();
            let mut ended = false;
            for index in 0..256 {
                let byte = bytes[at(name + index, 1)?];
                if byte == 0 {
                    ended = true;
                    break;
                }
                dll.push(byte);
            }
            ensure!(ended, "unterminated PE DLL name");
            let dll = std::str::from_utf8(&dll)?.to_ascii_lowercase();
            ensure!(
                SYSTEM_DLLS.contains(&dll.as_str()),
                "non-stock Windows executable dependency: {dll}"
            );
            imports.insert(dll);
        }
        ensure!(terminated, "unterminated PE import directory");
    }
    Ok(imports.into_iter().collect())
}

/// Keep subsystem qualification separate from the already-checked native imports.
pub(super) fn verify_pe_subsystem(
    bytes: &[u8],
    target: &str,
    subsystem: u16,
) -> Result<Vec<String>> {
    ensure!(
        matches!(subsystem, 2 | 3),
        "unsupported paired PE subsystem"
    );
    let imports = verify_pe(bytes, target)?;
    let optional = (u32_at(bytes, 60)? as usize)
        .checked_add(24)
        .context("PE optional header overflow")?;
    ensure!(
        u16_at(bytes, optional + 68)? == subsystem,
        "paired PE subsystem differs"
    );
    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const TARGET: &str = "x86_64-pc-windows-msvc";

    fn pe(machine: u16) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&machine.to_le_bytes());
        bytes[134..136].copy_from_slice(&1_u16.to_le_bytes());
        bytes[148..150].copy_from_slice(&240_u16.to_le_bytes());
        bytes[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
        bytes
    }

    fn archive(binary: &[u8], extra: Option<&str>, wrong_name: Option<&str>) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for file in FILES {
            let name = if file == "README.md" {
                wrong_name
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("locron-v0.10.0-{TARGET}/{file}"))
            } else {
                format!("locron-v0.10.0-{TARGET}/{file}")
            };
            zip.start_file(name, options).unwrap();
            zip.write_all(if file == "locron.exe" {
                binary
            } else {
                b"fixture"
            })
            .unwrap();
        }
        if let Some(name) = extra {
            zip.start_file(name, options).unwrap();
            zip.write_all(b"unexpected").unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn archive_rejects_wrong_hash_version_architecture_and_member_paths() {
        let bytes = archive(&pe(0x8664), None, None);
        let verified = verify_archive(&bytes, "0.10.0", TARGET, &sha256_hex(&bytes)).unwrap();
        assert_eq!(verified.files.len(), 4);
        assert_eq!(verified.binary_sha256, sha256_hex(&pe(0x8664)));
        assert!(verify_archive(&bytes, "0.10.0", TARGET, &"00".repeat(32)).is_err());
        assert!(verify_archive(&bytes, "0.10.1", TARGET, &sha256_hex(&bytes)).is_err());
        let bytes = archive(&pe(0xaa64), None, None);
        assert!(verify_archive(&bytes, "0.10.0", TARGET, &sha256_hex(&bytes)).is_err());
        for name in [
            "../outside",
            "/outside",
            "C:/outside",
            "dir\\outside",
            "locron-v0.10.0-x86_64-pc-windows-msvc/README.MD",
        ] {
            let bytes = archive(&pe(0x8664), None, Some(name));
            assert!(verify_archive(&bytes, "0.10.0", TARGET, &sha256_hex(&bytes)).is_err());
        }
        let bytes = archive(&pe(0x8664), Some("extra"), None);
        assert!(verify_archive(&bytes, "0.10.0", TARGET, &sha256_hex(&bytes)).is_err());
    }

    #[test]
    fn initial_executable_must_be_unsigned_and_have_correct_native_architecture() {
        verify_pe(&pe(0xaa64), "aarch64-pc-windows-msvc").unwrap();
        assert!(verify_pe(&pe(0x14c), TARGET).is_err());
        assert!(verify_pe(b"truncated", TARGET).is_err());
        let mut signed = pe(0x8664);
        signed[296..300].copy_from_slice(&480_u32.to_le_bytes());
        assert!(verify_pe(&signed, TARGET).is_err());
    }

    #[test]
    fn archive_refuses_conflicting_raw_headers_special_members_and_size_bombs() {
        let original = archive(&pe(0x8664), None, None);
        let central = u32_at(&original, original.len() - 6).unwrap() as usize;
        for (offset, bytes) in [
            // The same checksum can describe a ZIP with disagreeing parser views.
            (central + 8, 1_u16.to_le_bytes().to_vec()),
            (central + 10, 99_u16.to_le_bytes().to_vec()),
            (central + 38, 0x0400_u32.to_le_bytes().to_vec()),
            (central + 38, 0xa000_0000_u32.to_le_bytes().to_vec()),
            (6, 1_u16.to_le_bytes().to_vec()),
            (14, 0_u32.to_le_bytes().to_vec()),
        ] {
            let mut modified = original.clone();
            modified[offset..offset + bytes.len()].copy_from_slice(&bytes);
            assert!(verify_archive(&modified, "0.10.0", TARGET, &sha256_hex(&modified)).is_err());
        }
        let mut bomb = original.clone();
        let excessive = u32::try_from(LIMIT + 1).unwrap().to_le_bytes();
        bomb[central + 24..central + 28].copy_from_slice(&excessive);
        bomb[22..26].copy_from_slice(&excessive);
        assert!(verify_archive(&bomb, "0.10.0", TARGET, &sha256_hex(&bomb)).is_err());

        let mut prefixed = b"unexpected prefix".to_vec();
        prefixed.extend_from_slice(&original);
        assert!(verify_archive(&prefixed, "0.10.0", TARGET, &sha256_hex(&prefixed)).is_err());
        let mut trailed = original.clone();
        trailed.push(0);
        assert!(verify_archive(&trailed, "0.10.0", TARGET, &sha256_hex(&trailed)).is_err());
        for version in [
            "",
            "1",
            "1.0",
            "01.0.0",
            "1.0.0-beta",
            "1.0.0+build",
            "../0.10.0",
        ] {
            assert!(verify_archive(&original, version, TARGET, &sha256_hex(&original)).is_err());
        }
    }

    #[test]
    fn both_import_formats_refuse_external_crt_and_application_dlls() {
        for delayed in [false, true] {
            for dll in [
                "KERNEL32.dll",
                "api-ms-win-core-synch-l1-2-0.dll",
                "VCRUNTIME140.dll",
                "MSVCP140.dll",
                "ucrtbased.dll",
                "application.dll",
                "api-ms-win-core-synch-l1-3-0.dll",
                "api-ms-win-crt-runtime-l1-1-0.dll",
            ] {
                let mut bytes = pe(0x8664);
                bytes.resize(1024, 0);
                for (offset, value) in [(400, 512_u32), (404, 0x1000), (408, 512), (412, 512)] {
                    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                }
                let directory = if delayed { 368 } else { 272 };
                bytes[directory..directory + 4].copy_from_slice(&0x1000_u32.to_le_bytes());
                bytes[directory + 4..directory + 8]
                    .copy_from_slice(&(if delayed { 64_u32 } else { 40 }).to_le_bytes());
                let name = if delayed {
                    bytes[512..516].copy_from_slice(&1_u32.to_le_bytes());
                    516
                } else {
                    524
                };
                bytes[name..name + 4].copy_from_slice(&0x1080_u32.to_le_bytes());
                bytes[640..640 + dll.len()].copy_from_slice(dll.as_bytes());
                let result = verify_pe(&bytes, TARGET);
                if matches!(dll, "KERNEL32.dll" | "api-ms-win-core-synch-l1-2-0.dll") {
                    assert_eq!(result.unwrap(), [dll.to_ascii_lowercase()]);
                } else {
                    assert!(result.is_err(), "{dll} delayed={delayed}");
                }
            }
        }
    }
}
