//! Package-manager ownership markers shared by update and service admission.

use std::path::Path;

/// Whether a canonical executable belongs to a marker-bearing installation.
///
/// Homebrew keeps new markers inside the keg-private `libexec` directory so
/// linking the keg cannot collide with another formula's shared `lib` marker.
/// Retain the old path for already-installed kegs and version-only formula bumps.
pub(crate) fn has_marker(executable: &Path) -> bool {
    let directory = executable.parent().unwrap_or_else(|| Path::new("/"));
    [
        "../libexec/locron/.disable-self-update",
        "../lib/.disable-self-update",
    ]
    .iter()
    .any(|marker| directory.join(marker).exists())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::has_marker;

    fn assert_marker_is_recognized(marker: &str) {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        let executable = bin.join("locron");
        assert!(!has_marker(&executable));
        let marker = root.path().join(marker);
        fs::create_dir_all(marker.parent().unwrap()).unwrap();
        fs::write(&marker, "").unwrap();
        assert!(has_marker(&executable));
        fs::remove_file(marker).unwrap();
        assert!(!has_marker(&executable));
    }

    #[test]
    fn keg_private_marker_is_recognized() {
        assert_marker_is_recognized("libexec/locron/.disable-self-update");
    }

    #[test]
    fn legacy_marker_is_recognized() {
        assert_marker_is_recognized("lib/.disable-self-update");
    }

    #[test]
    fn shared_prefix_marker_does_not_mark_an_unmarked_keg() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("Cellar/locron/1.2.3/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(root.path().join("lib")).unwrap();
        fs::write(root.path().join("lib/.disable-self-update"), "").unwrap();
        assert!(!has_marker(&bin.join("locron")));
    }
}
