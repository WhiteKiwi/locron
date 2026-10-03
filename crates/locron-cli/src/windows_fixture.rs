//! Test-owned private roots with retained canonical ancestry and precise cleanup.

use std::path::Path;

use locron_core::filesystem::{DirectoryGuard, is_private};

pub(super) struct PrivateFixture {
    // Drop retained handles before TempDir recursively removes its own container.
    guard: DirectoryGuard,
    _temporary: tempfile::TempDir,
}

impl PrivateFixture {
    pub(super) fn new(prefix: &str) -> Self {
        let temporary = tempfile::Builder::new().prefix(prefix).tempdir().unwrap();
        // The inherited container is not a managed private object and is never repaired.
        let ancestry = DirectoryGuard::ancestors(temporary.path()).unwrap();
        let guard = DirectoryGuard::private(&ancestry.normalized_path().join("private")).unwrap();
        assert!(is_private(guard.normalized_path(), true).unwrap());
        assert_eq!(
            guard.normalized_path(),
            std::fs::canonicalize(guard.normalized_path()).unwrap()
        );
        Self {
            guard,
            _temporary: temporary,
        }
    }

    pub(super) fn path(&self) -> &Path {
        self.guard.normalized_path()
    }
}
