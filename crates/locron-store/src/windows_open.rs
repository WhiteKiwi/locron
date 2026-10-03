//! Fixed failure facts; diagnostics never alter an admitted operation or its error.

#[cfg(debug_assertions)]
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{StoreError, StoreResult};

#[cfg(debug_assertions)]
static NEXT_OPEN: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Root,
    Outputs,
    Temporary,
    StateGuard,
    DatabaseOpen,
    DatabaseCreate,
    DatabaseAdmission,
    WalOpen,
    WalCreate,
    ShmOpen,
    ShmCreate,
    Connection,
    ConfigureWal,
    ConfigureSettings,
    Migrate,
    DatabaseFinal,
    WalFinal,
    ShmFinal,
}

impl Stage {
    #[cfg(debug_assertions)]
    fn label(self) -> &'static str {
        match self {
            Self::Root => "directory-root",
            Self::Outputs => "directory-outputs",
            Self::Temporary => "directory-tmp",
            Self::StateGuard => "state-guard",
            Self::DatabaseOpen => "database-open",
            Self::DatabaseCreate => "database-create-new",
            Self::DatabaseAdmission => "database-admission",
            Self::WalOpen => "wal-open",
            Self::WalCreate => "wal-create-new",
            Self::ShmOpen => "shm-open",
            Self::ShmCreate => "shm-create-new",
            Self::Connection => "sqlite-connection",
            Self::ConfigureWal => "sqlite-configure-wal",
            Self::ConfigureSettings => "sqlite-configure-settings",
            Self::Migrate => "sqlite-migrate",
            Self::DatabaseFinal => "database-final-open",
            Self::WalFinal => "wal-final-open",
            Self::ShmFinal => "shm-final-open",
        }
    }
}

pub(crate) struct OpenTrace {
    #[cfg(debug_assertions)]
    id: u64,
}

impl OpenTrace {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(debug_assertions)]
            id: NEXT_OPEN.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub(crate) fn io<T>(&self, stage: Stage, result: std::io::Result<T>) -> std::io::Result<T> {
        result.inspect_err(|error| self.io_failure(stage, error))
    }

    pub(crate) fn store<T>(&self, stage: Stage, result: StoreResult<T>) -> StoreResult<T> {
        result.inspect_err(|error| match error {
            StoreError::Io(error) => self.io_failure(stage, error),
            StoreError::Sqlite(error) => self.sqlite_failure(stage, error),
            _ => {
                #[cfg(debug_assertions)]
                eprintln!("{} category=store", self.prefix(stage));
            }
        })
    }

    pub(crate) fn sqlite<T>(
        &self,
        stage: Stage,
        result: rusqlite::Result<T>,
    ) -> rusqlite::Result<T> {
        result.inspect_err(|error| self.sqlite_failure(stage, error))
    }

    pub(crate) fn io_failure(&self, stage: Stage, error: &std::io::Error) {
        #[cfg(debug_assertions)]
        eprintln!("{}", self.io_line(stage, error));
        #[cfg(not(debug_assertions))]
        let _ = (stage, error);
    }

    fn sqlite_failure(&self, stage: Stage, error: &rusqlite::Error) {
        #[cfg(debug_assertions)]
        match error {
            rusqlite::Error::SqliteFailure(code, _) => eprintln!(
                "{} category=sqlite primary={:?} extended={}",
                self.prefix(stage),
                code.code,
                code.extended_code
            ),
            _ => eprintln!("{} category=sqlite", self.prefix(stage)),
        }
        #[cfg(not(debug_assertions))]
        let _ = (stage, error);
    }

    #[cfg(debug_assertions)]
    fn prefix(&self, stage: Stage) -> String {
        format!(
            "windows-store-open operation={} stage={}",
            self.id,
            stage.label()
        )
    }

    #[cfg(debug_assertions)]
    fn io_line(&self, stage: Stage, error: &std::io::Error) -> String {
        format!(
            "{} category=io kind={:?} raw_os={:?}",
            self.prefix(stage),
            error.kind(),
            error.raw_os_error()
        )
    }
}

#[cfg(all(test, debug_assertions))]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;

    use locron_core::filesystem::{DirectoryGuard, create_private_new, open_private};

    use super::{OpenTrace, Stage};

    #[test]
    fn native_failure_facts_preserve_error_codes_and_existing_bytes() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("private");
        let _guard = DirectoryGuard::private(&root).unwrap();
        let path = root.join("private-content-must-not-render");
        let mut file = create_private_new(&path).unwrap();
        file.write_all(b"unchanged").unwrap();
        drop(file);

        let trace = OpenTrace::new();
        let error = trace
            .io(Stage::DatabaseCreate, create_private_new(&path))
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(matches!(error.raw_os_error(), Some(80 | 183)));
        assert_eq!(std::fs::read(&path).unwrap(), b"unchanged");
        let line = trace.io_line(Stage::DatabaseCreate, &error);
        assert!(line.contains("stage=database-create-new"));
        assert!(line.contains("kind=AlreadyExists"));
        assert!(!line.contains("private-content-must-not-render"));
        assert!(line.len() < 200);

        let missing = root.join("missing-private-content");
        let error = trace
            .io(
                Stage::WalOpen,
                open_private(&missing, OpenOptions::new().read(true)),
            )
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        assert_eq!(error.raw_os_error(), Some(2));
        assert!(!missing.exists());
        assert!(
            !trace
                .io_line(Stage::WalOpen, &error)
                .contains("missing-private-content")
        );
    }
}
