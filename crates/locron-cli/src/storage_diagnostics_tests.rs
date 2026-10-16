use super::*;

#[test]
fn structured_core_storage_category_survives_context_and_redacts_it() {
    for (kind, primary) in [("busy", Some(5)), ("locked", Some(6)), ("io", None)] {
        let diagnostic = locron_core::PersistenceDiagnostic {
            operation: "remove_job",
            stage: "begin",
            kind,
            elapsed_us: 5_000_100,
            admission_wait_us: 10,
            connection_wait_us: 20,
            sqlite_wait_us: 5_000_000,
            transaction_us: 0,
            commit_us: 0,
            retries: 4,
            sqlite_primary_code: primary,
            sqlite_extended_code: primary,
            os_error_code: None,
            retryable: kind == "busy",
            durable_outcome: "not_committed",
            lock_owner: "unknown",
        };
        let error = anyhow::Error::from(CoreError::PersistenceDiagnostic(Box::new(
            diagnostic.clone(),
        )))
        .context("private-argv-env-header-body-canary");
        assert_eq!(error_code(&error), "state_error");
        assert_eq!(exit_code(&error), 5);
        let document = error_document(&error);
        assert_eq!(document["storage"], json!(diagnostic));
        assert_eq!(document["message"], diagnostic.to_string());
        assert_eq!(safe_error_chain(&error), diagnostic.to_string());
        assert!(!document.to_string().contains("canary"));
        assert!(!safe_error_chain(&error).contains("canary"));
    }
    let validation = anyhow!("ordinary invalid request");
    assert_eq!(error_code(&validation), "invalid_request");
    assert_eq!(exit_code(&validation), 2);
    assert!(error_document(&validation).get("storage").is_none());
    let conflict = anyhow::Error::from(StoreError::Conflict("revision changed".into()));
    assert_eq!(error_code(&conflict), "durable_conflict");
    assert_eq!(exit_code(&conflict), 3);
    assert!(error_document(&conflict).get("storage").is_none());
}
