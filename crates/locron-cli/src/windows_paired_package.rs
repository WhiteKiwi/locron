//! Pure five-member Windows ZIP and paired PE qualification.
//! Actual image version/ABI probes and retained native pair guards remain separate gates.

use std::collections::BTreeMap;

use anyhow::{Result, ensure};

use super::sha256_hex;
use super::windows_package::{Inventory, read_archive, verify_pe_subsystem};
use super::windows_paired_receipt::EXECUTABLES;
use super::windows_receipt::{stable_version, valid_hash};

pub(super) struct Image {
    pub sha256: String,
    pub imports: Vec<String>,
}

/// The positions match the strict receipt: console first, GUI launcher second.
/// No member is extracted and no filesystem/task/PATH/journal effect is permitted.
pub(super) struct Archive {
    pub files: BTreeMap<String, Vec<u8>>,
    pub executables: [Image; 2],
}

pub(super) fn verify_archive(
    bytes: &[u8],
    version: &str,
    target: &str,
    expected_sha256: &str,
) -> Result<Archive> {
    stable_version(version)?;
    ensure!(
        matches!(target, "x86_64-pc-windows-msvc" | "aarch64-pc-windows-msvc"),
        "unsupported paired native target"
    );
    ensure!(
        valid_hash(expected_sha256),
        "invalid final paired archive digest"
    );
    let files = read_archive(bytes, version, target, expected_sha256, Inventory::Paired)?;
    let console = image(&files, EXECUTABLES[0], target, 3)?;
    let launcher = image(&files, EXECUTABLES[1], target, 2)?;
    Ok(Archive {
        files,
        executables: [console, launcher],
    })
}

fn image(
    files: &BTreeMap<String, Vec<u8>>,
    name: &str,
    target: &str,
    subsystem: u16,
) -> Result<Image> {
    let bytes = files
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("paired Windows ZIP is missing {name}"))?;
    Ok(Image {
        sha256: sha256_hex(bytes),
        imports: verify_pe_subsystem(bytes, target, subsystem)?,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    use super::super::windows_package::{LIMIT, PAIRED_FILES};
    use super::*;

    const TARGET: &str = "x86_64-pc-windows-msvc";
    const ARM: &str = "aarch64-pc-windows-msvc";

    fn pe(machine: u16, subsystem: u16, import: Option<(&str, bool)>) -> Vec<u8> {
        let mut bytes = vec![0; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&machine.to_le_bytes());
        bytes[134..136].copy_from_slice(&1_u16.to_le_bytes());
        bytes[148..150].copy_from_slice(&240_u16.to_le_bytes());
        bytes[150..152].copy_from_slice(&0x22_u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x20b_u16.to_le_bytes());
        bytes[220..222].copy_from_slice(&subsystem.to_le_bytes());
        bytes[260..264].copy_from_slice(&16_u32.to_le_bytes());
        if let Some((dll, delayed)) = import {
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
        }
        bytes
    }

    fn names(target: &str) -> Vec<String> {
        PAIRED_FILES
            .iter()
            .map(|name| format!("locron-v0.10.0-{target}/{name}"))
            .collect()
    }

    fn archive(console: &[u8], launcher: &[u8], members: &[String]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for name in members {
            writer.start_file(name, options).unwrap();
            let leaf = name.rsplit('/').next().unwrap();
            writer
                .write_all(match leaf {
                    "locron.exe" => console,
                    "locron-service-launcher.exe" => launcher,
                    _ => b"paired archive fixture bytes",
                })
                .unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    // Duplicate an actual raw local/catalog entry without relying on ZipWriter's
    // filename map. Count and spans remain valid, so the raw duplicate boundary is tested.
    fn duplicate_raw_member(original: &[u8]) -> Vec<u8> {
        let end = original.len() - 22;
        let u16_at = |at| u16::from_le_bytes(original[at..at + 2].try_into().unwrap());
        let u32_at = |at| u32::from_le_bytes(original[at..at + 4].try_into().unwrap());
        let mut at = u32_at(end + 16) as usize;
        let mut entries = Vec::new();
        for _ in 0..PAIRED_FILES.len() {
            let name_size = usize::from(u16_at(at + 28));
            assert_eq!(u16_at(at + 30), 0);
            assert_eq!(u16_at(at + 32), 0);
            let local = u32_at(at + 42) as usize;
            let local_end = local + 30 + name_size + u32_at(at + 20) as usize;
            entries.push((
                original[local..local_end].to_vec(),
                original[at..at + 46 + name_size].to_vec(),
            ));
            at += 46 + name_size;
        }
        assert_eq!(at, end);
        entries[3] = entries[2].clone();
        let mut bytes = Vec::new();
        let mut rows = Vec::new();
        for (local, mut row) in entries {
            row[42..46].copy_from_slice(&u32::try_from(bytes.len()).unwrap().to_le_bytes());
            bytes.extend_from_slice(&local);
            rows.extend_from_slice(&row);
        }
        let start = u32::try_from(bytes.len()).unwrap();
        let size = u32::try_from(rows.len()).unwrap();
        bytes.extend_from_slice(&rows);
        let mut footer = original[end..].to_vec();
        footer[12..16].copy_from_slice(&size.to_le_bytes());
        footer[16..20].copy_from_slice(&start.to_le_bytes());
        bytes.extend_from_slice(&footer);
        bytes
    }

    fn normal(target: &str, machine: u16) -> Vec<u8> {
        archive(&pe(machine, 3, None), &pe(machine, 2, None), &names(target))
    }

    fn verify(bytes: &[u8], target: &str) -> Result<Archive> {
        verify_archive(bytes, "0.10.0", target, &sha256_hex(bytes))
    }

    #[test]
    fn exact_five_members_bind_both_native_images_and_ordered_final_hashes() {
        for (target, machine) in [(TARGET, 0x8664), (ARM, 0xaa64)] {
            let bytes = normal(target, machine);
            let parsed = verify(&bytes, target).unwrap();
            assert_eq!(parsed.files.len(), 5);
            assert_eq!(
                parsed.executables[0].sha256,
                sha256_hex(&pe(machine, 3, None))
            );
            assert_eq!(
                parsed.executables[1].sha256,
                sha256_hex(&pe(machine, 2, None))
            );
            assert!(
                parsed
                    .executables
                    .iter()
                    .all(|image| image.imports.is_empty())
            );
            for name in PAIRED_FILES {
                assert!(parsed.files.contains_key(name));
            }
        }
    }

    #[test]
    fn pair_refuses_single_image_extra_members_and_unsafe_member_spellings() {
        let console = pe(0x8664, 3, None);
        let launcher = pe(0x8664, 2, None);
        for missing in PAIRED_FILES {
            let members: Vec<_> = names(TARGET)
                .into_iter()
                .filter(|name| !name.ends_with(&format!("/{missing}")))
                .collect();
            assert!(verify(&archive(&console, &launcher, &members), TARGET).is_err());
        }
        let mut extra = names(TARGET);
        extra.push(format!("locron-v0.10.0-{TARGET}/FONT-LICENSE"));
        assert!(verify(&archive(&console, &launcher, &extra), TARGET).is_err());
        for bad in [
            "../locron-service-launcher.exe",
            "/outside",
            "C:/outside",
            "directory\\outside",
            "locron-v0.10.0-x86_64-pc-windows-msvc/LOCRON-service-launcher.exe",
        ] {
            let mut members = names(TARGET);
            members[1] = bad.to_owned();
            assert!(verify(&archive(&console, &launcher, &members), TARGET).is_err());
        }
    }

    #[test]
    fn both_images_must_match_architecture_unsigned_status_and_distinct_subsystems() {
        let console = pe(0x8664, 3, None);
        let launcher = pe(0x8664, 2, None);
        for (wrong_console, wrong_launcher) in [
            (launcher.clone(), console.clone()),
            (console.clone(), pe(0xaa64, 2, None)),
            (pe(0xaa64, 3, None), launcher.clone()),
            (pe(0x8664, 1, None), launcher.clone()),
            (console.clone(), pe(0x8664, 3, None)),
        ] {
            assert!(
                verify(
                    &archive(&wrong_console, &wrong_launcher, &names(TARGET)),
                    TARGET
                )
                .is_err()
            );
        }
        for console_signed in [false, true] {
            let mut signed_console = console.clone();
            let mut signed_launcher = launcher.clone();
            let selected = if console_signed {
                &mut signed_console
            } else {
                &mut signed_launcher
            };
            selected[296..300].copy_from_slice(&480_u32.to_le_bytes());
            assert!(
                verify(
                    &archive(&signed_console, &signed_launcher, &names(TARGET)),
                    TARGET
                )
                .is_err()
            );
        }
    }

    #[test]
    fn both_images_enforce_the_same_stock_normal_and_delay_import_allowlist() {
        for delayed in [false, true] {
            for console_import in [false, true] {
                for dll in [
                    "KERNEL32.dll",
                    "VCRUNTIME140.dll",
                    "application.dll",
                    "api-ms-win-core-synch-l1-3-0.dll",
                ] {
                    let console = pe(
                        0x8664,
                        3,
                        if console_import {
                            Some((dll, delayed))
                        } else {
                            None
                        },
                    );
                    let launcher = pe(
                        0x8664,
                        2,
                        if console_import {
                            None
                        } else {
                            Some((dll, delayed))
                        },
                    );
                    let result = verify(&archive(&console, &launcher, &names(TARGET)), TARGET);
                    if dll == "KERNEL32.dll" {
                        let parsed = result.unwrap();
                        let selected = if console_import { 0 } else { 1 };
                        assert_eq!(parsed.executables[selected].imports, ["kernel32.dll"]);
                        assert!(parsed.executables[1 - selected].imports.is_empty());
                    } else {
                        assert!(
                            result.is_err(),
                            "{dll} delayed={delayed} console={console_import}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn raw_catalog_conflicts_reparse_links_size_bombs_and_trailing_data_refuse() {
        let original = normal(TARGET, 0x8664);
        let duplicate = duplicate_raw_member(&original);
        let error = verify(&duplicate, TARGET).err().unwrap();
        assert!(error.to_string().contains("duplicate Windows ZIP filename"));
        let end = original.len() - 22;
        let central = u32::from_le_bytes(original[end + 16..end + 20].try_into().unwrap()) as usize;
        for (offset, replacement) in [
            (end + 8, 4_u16.to_le_bytes().to_vec()),
            (central + 8, 1_u16.to_le_bytes().to_vec()),
            (central + 10, 99_u16.to_le_bytes().to_vec()),
            (central + 38, 0x0400_u32.to_le_bytes().to_vec()),
            (central + 38, 0xa000_0000_u32.to_le_bytes().to_vec()),
            (6, 1_u16.to_le_bytes().to_vec()),
            (14, 0_u32.to_le_bytes().to_vec()),
        ] {
            let mut changed = original.clone();
            changed[offset..offset + replacement.len()].copy_from_slice(&replacement);
            assert!(verify(&changed, TARGET).is_err());
        }
        let mut bomb = original.clone();
        let excessive = u32::try_from(LIMIT + 1).unwrap().to_le_bytes();
        bomb[central + 24..central + 28].copy_from_slice(&excessive);
        bomb[22..26].copy_from_slice(&excessive);
        assert!(verify(&bomb, TARGET).is_err());
        let mut prefixed = b"unexpected prefix".to_vec();
        prefixed.extend_from_slice(&original);
        assert!(verify(&prefixed, TARGET).is_err());
        let mut trailed = original;
        trailed.push(0);
        assert!(verify(&trailed, TARGET).is_err());
    }

    #[test]
    fn final_checksum_precedes_zip_interpretation_and_no_old_version_or_target_fallback_exists() {
        let error = verify_archive(b"not a ZIP", "0.10.0", TARGET, &"00".repeat(32))
            .err()
            .unwrap();
        assert!(error.to_string().contains("digest mismatch"));
        let bytes = normal(TARGET, 0x8664);
        assert!(verify_archive(&bytes, "0.10.1", TARGET, &sha256_hex(&bytes)).is_err());
        assert!(verify_archive(&bytes, "0.9.6", TARGET, &sha256_hex(&bytes)).is_err());
        assert!(
            verify_archive(
                &bytes,
                "0.10.0",
                "x86_64-pc-windows-gnu",
                &sha256_hex(&bytes)
            )
            .is_err()
        );
        assert!(
            verify_archive(&bytes, "0.10.0", TARGET, &sha256_hex(&bytes).to_uppercase()).is_err()
        );
    }
}
