UPDATE settings
SET run_retention_age_us = 7776000000000
WHERE singleton = 1 AND run_retention_age_us IS NULL;

CREATE TABLE run_retention_pending (
    run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE RESTRICT,
    selected_at_us INTEGER NOT NULL
) STRICT;

CREATE INDEX run_retention_pending_selected
    ON run_retention_pending(selected_at_us, run_id);
