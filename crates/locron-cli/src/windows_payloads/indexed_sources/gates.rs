//! No-write dual-image exclusion for a previously qualified WinGet source set.
//! Package ownership is never widened into standalone replacement authority.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Result, ensure};
use locron_core::filesystem::{FileIdentity, file_identity, open_owned_executable_exclusive};
use sha2::{Digest, Sha256};

use super::super::super::windows_paired_receipt::EXECUTABLES;
use super::super::super::windows_protocol::maintenance_path;
use super::super::super::windows_receipt::same_path;
use super::{IndexedSources, Leaf, Sources, clock, observe, text};

struct ExpectedImage {
    path: PathBuf,
    identity: FileIdentity,
    length: u64,
    sha256: String,
}

impl ExpectedImage {
    fn capture(leaf: &Leaf, deadline: Instant) -> Result<Self> {
        observe(deadline, || {
            ensure!(
                file_identity(&leaf.file)? == leaf.identity
                    && leaf.file.metadata()?.len() == leaf.length,
                "indexed image changed before exclusive admission"
            );
            Ok(Self {
                path: leaf.file.normalized_path().to_owned(),
                identity: leaf.identity,
                length: leaf.length,
                sha256: leaf.sha256.clone(),
            })
        })
    }

    fn open(self, deadline: Instant) -> Result<Leaf> {
        maintenance_path(text(&self.path)?)?;
        let mut file = observe(deadline, || {
            Ok(open_owned_executable_exclusive(&self.path)?)
        })?;
        ensure!(
            observe(deadline, || Ok(file_identity(&file)?))? == self.identity
                && same_path(text(file.normalized_path())?, text(&self.path)?)?
                && observe(deadline, || Ok(file.metadata()?.len()))? == self.length,
            "exclusive indexed image is not the original exact object"
        );
        observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
        let mut digest = Sha256::new();
        let mut read = 0_u64;
        let mut chunk = [0_u8; 64 * 1024];
        loop {
            let capacity = usize::try_from((self.length + 1 - read).min(chunk.len() as u64))?;
            let count = observe(deadline, || Ok(file.read(&mut chunk[..capacity])?))?;
            if count == 0 {
                break;
            }
            read += count as u64;
            ensure!(read <= self.length, "exclusive indexed image grew");
            digest.update(&chunk[..count]);
        }
        ensure!(
            read == self.length
                && format!("{:x}", digest.finalize()) == self.sha256
                && observe(deadline, || Ok(file_identity(&file)?))? == self.identity
                && observe(deadline, || Ok(file.metadata()?.len()))? == self.length,
            "exclusive indexed image differs from canonical source bytes"
        );
        observe(deadline, || Ok(file.seek(SeekFrom::Start(0))?))?;
        Ok(Leaf {
            file,
            identity: self.identity,
            sha256: self.sha256,
            length: self.length,
        })
    }
}

fn acquire_after(
    sources: Sources,
    deadline: Instant,
    after_release: impl FnOnce() -> Result<()>,
) -> Result<Sources> {
    clock(deadline)?;
    let expected = [
        ExpectedImage::capture(&sources.files[EXECUTABLES[0]], deadline)?,
        ExpectedImage::capture(&sources.files[EXECUTABLES[1]], deadline)?,
    ];
    let Sources { root, mut files } = sources;
    // Only image reads are released. All three document and ancestry guards survive.
    for name in EXECUTABLES {
        drop(files.remove(name).expect("qualified fixed image inventory"));
    }
    // Private fixture boundary: no caller can inject admission or alternate facts.
    after_release()?;
    clock(deadline)?;
    for (name, image) in EXECUTABLES.into_iter().zip(expected) {
        files.insert(name.to_owned(), image.open(deadline)?);
    }
    clock(deadline)?;
    Ok(Sources { root, files })
}

fn held_facts(sources: &Sources, deadline: Instant) -> Result<()> {
    clock(deadline)?;
    for (name, leaf) in &sources.files {
        observe(deadline, || {
            ensure!(
                file_identity(&leaf.file)? == leaf.identity
                    && leaf.file.metadata()?.len() == leaf.length
                    && same_path(
                        text(leaf.file.normalized_path())?,
                        text(&sources.root.normalized_path().join(name))?,
                    )?,
                "retained indexed object no longer matches its source binding"
            );
            Ok(())
        })?;
    }
    clock(deadline)
}

/// Both images are exclusively held; all other source/index/ancestor reads remain owned.
/// No mutable-handle escape or filesystem/task/package operation is available.
pub(in crate::self_update) struct ExclusiveIndexedSources {
    source: IndexedSources,
}

impl IndexedSources {
    pub(in crate::self_update) fn into_exclusive(self) -> Result<ExclusiveIndexedSources> {
        let deadline = self.original_deadline;
        self.revalidate_until(deadline)?;
        let Self {
            index,
            sources,
            version,
            target,
            archive_sha256,
            original_deadline,
        } = self;
        let sources = acquire_after(sources, deadline, || Ok(()))?;
        held_facts(&sources, deadline)?;
        // This reads the registration/index only, not the now-exclusive image paths.
        index.revalidate_until(deadline)?;
        clock(deadline)?;
        Ok(ExclusiveIndexedSources {
            source: Self {
                index,
                sources,
                version,
                target,
                archive_sha256,
                original_deadline,
            },
        })
    }
}

impl ExclusiveIndexedSources {
    pub(in crate::self_update) fn directory(&self) -> &Path {
        self.source.directory()
    }

    pub(in crate::self_update) fn file_facts(
        &self,
    ) -> impl Iterator<Item = (&str, FileIdentity, &str, u64)> {
        self.source.file_facts()
    }

    pub(in crate::self_update) fn revalidate_until(&self, deadline: Instant) -> Result<()> {
        let deadline = deadline.min(self.source.original_deadline);
        held_facts(&self.source.sources, deadline)?;
        self.source.index.revalidate_until(deadline)?;
        clock(deadline)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::windows::fs::OpenOptionsExt;
    use std::time::Duration;

    use locron_core::filesystem::{
        create_private_new_exclusive, is_private, open_private_exclusive, read_owned_executable,
    };

    use super::super::super::super::sha256_hex;
    use super::super::super::super::windows_fixture::PrivateFixture;
    use super::super::super::super::windows_package::PAIRED_FILES;
    use super::super::super::super::windows_paired_package::{Archive, Image};
    use super::*;

    fn deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    fn write_new(path: &Path, bytes: &[u8]) {
        let mut file = create_private_new_exclusive(path).unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }

    fn fixture() -> (PrivateFixture, Archive) {
        let root = PrivateFixture::new("locron-indexed-exclusion-");
        // These are never executed or claimed to qualify PE/version/registry identity.
        // The parent constructor's canonical archive admission is tested separately.
        let files: BTreeMap<_, _> = PAIRED_FILES
            .into_iter()
            .map(|name| {
                (
                    name.to_owned(),
                    format!("source fixture: {name}").into_bytes(),
                )
            })
            .collect();
        for (name, bytes) in &files {
            write_new(&root.path().join(name), bytes);
        }
        let executables = EXECUTABLES.map(|name| Image {
            sha256: sha256_hex(&files[name]),
            imports: Vec::new(),
        });
        (root, Archive { files, executables })
    }

    #[test]
    fn both_image_gates_and_all_document_reads_overlap_until_drop() {
        let (root, archive) = fixture();
        let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
        let original: Vec<_> = sources.files.values().map(|leaf| leaf.identity).collect();
        let exclusive = acquire_after(sources, deadline(), || Ok(())).unwrap();
        held_facts(&exclusive, deadline()).unwrap();
        assert_eq!(
            exclusive
                .files
                .values()
                .map(|leaf| leaf.identity)
                .collect::<Vec<_>>(),
            original
        );
        for name in PAIRED_FILES {
            let path = root.path().join(name);
            assert!(OpenOptions::new().write(true).open(&path).is_err());
            assert!(fs::rename(&path, root.path().join("moved")).is_err());
            if EXECUTABLES.contains(&name) {
                assert!(OpenOptions::new().read(true).open(&path).is_err());
            } else {
                assert_eq!(fs::read(&path).unwrap(), archive.files[name]);
            }
        }
        drop(exclusive);
        for name in EXECUTABLES {
            assert_eq!(
                fs::read(root.path().join(name)).unwrap(),
                archive.files[name]
            );
        }
    }

    #[test]
    fn second_image_contention_releases_first_gate_without_writes() {
        let (root, archive) = fixture();
        let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
        let held = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(root.path().join(EXECUTABLES[1]))
            .unwrap();
        assert!(acquire_after(sources, deadline(), || Ok(())).is_err());
        assert!(open_owned_executable_exclusive(&root.path().join(EXECUTABLES[0])).is_ok());
        drop(held);
        for name in PAIRED_FILES {
            assert_eq!(
                fs::read(root.path().join(name)).unwrap(),
                archive.files[name]
            );
        }
    }

    #[test]
    fn either_same_byte_substitution_during_the_gap_is_left_untouched() {
        for name in EXECUTABLES {
            let (root, archive) = fixture();
            let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
            let original = sources.files[name].identity;
            let path = root.path().join(name);
            let saved = root.path().join("preserved-original");
            let result = acquire_after(sources, deadline(), || {
                fs::rename(&path, &saved)?;
                write_new(&path, &archive.files[name]);
                Ok(())
            });
            assert!(result.is_err(), "{name}");
            let changed = read_owned_executable(&path).unwrap();
            assert_ne!(file_identity(&changed).unwrap(), original);
            assert_eq!(fs::read(&path).unwrap(), archive.files[name]);
            assert_eq!(fs::read(&saved).unwrap(), archive.files[name]);
        }
    }

    #[test]
    fn package_read_acl_remains_accepted_without_private_acl_repair() {
        let (root, archive) = fixture();
        let paths = EXECUTABLES.map(|name| root.path().join(name));
        locron_core::windows::run_script_json(
            r"
            foreach ($path in $request.paths) {
                $acl = [IO.File]::GetAccessControl([string]$path)
                $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new(
                    [Security.Principal.SecurityIdentifier]::new('S-1-1-0'), 'ReadAndExecute', 'Allow'))
                [IO.File]::SetAccessControl([string]$path, $acl)
            }
            @{changed=$true} | & $locronToJson -Compress
            ",
            &serde_json::json!({"paths": paths}),
        )
        .unwrap();
        for path in &paths {
            assert!(!is_private(path, false).unwrap());
            assert!(open_private_exclusive(path).is_err());
        }
        let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
        let exclusive = acquire_after(sources, deadline(), || Ok(())).unwrap();
        held_facts(&exclusive, deadline()).unwrap();
        drop(exclusive);
        for path in &paths {
            assert!(!is_private(path, false).unwrap());
        }
    }

    #[test]
    fn readonly_and_external_hardlinks_refuse_without_repair() {
        for name in EXECUTABLES {
            let (root, archive) = fixture();
            let path = root.path().join(name);
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions).unwrap();
            let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
            let result = acquire_after(sources, deadline(), || Ok(()));
            let unchanged = fs::metadata(&path).unwrap().permissions().readonly();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_readonly(false);
            fs::set_permissions(&path, permissions).unwrap();
            assert!(result.is_err());
            assert!(unchanged);
            let alias = root.path().join("external-alias");
            fs::hard_link(&path, &alias).unwrap();
            let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
            assert!(acquire_after(sources, deadline(), || Ok(())).is_err());
            assert_eq!(fs::read(&alias).unwrap(), archive.files[name]);
            assert_eq!(fs::read(&path).unwrap(), archive.files[name]);
        }
    }

    #[test]
    fn expiry_and_missing_images_never_recreate_a_leaf_or_admit_a_late_gate() {
        let (root, archive) = fixture();
        let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
        let expired = Instant::now() - Duration::from_secs(1);
        let result = acquire_after(sources, expired, || panic!("expired admission"));
        assert!(result.is_err());
        let sources = Sources::read(root.path(), &archive, deadline()).unwrap();
        let missing = root.path().join(EXECUTABLES[1]);
        let result = acquire_after(sources, deadline(), || {
            fs::remove_file(&missing)?;
            Ok(())
        });
        assert!(result.is_err());
        assert!(!missing.exists());
        assert!(open_owned_executable_exclusive(&root.path().join(EXECUTABLES[0])).is_ok());
    }
}
