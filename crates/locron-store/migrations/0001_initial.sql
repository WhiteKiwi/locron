CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY CHECK (version > 0),
    name TEXT NOT NULL,
    checksum TEXT NOT NULL,
    binary_version TEXT NOT NULL,
    applied_at_us INTEGER NOT NULL
) STRICT;

CREATE TABLE settings (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    global_concurrency INTEGER NOT NULL CHECK (global_concurrency BETWEEN 1 AND 64),
    execution_path TEXT NOT NULL,
    run_retention_count INTEGER NOT NULL CHECK (run_retention_count >= 0),
    run_retention_age_us INTEGER CHECK (run_retention_age_us IS NULL OR run_retention_age_us >= 0),
    output_limit_bytes INTEGER NOT NULL CHECK (output_limit_bytes >= 0),
    per_run_output_limit_bytes INTEGER NOT NULL CHECK (per_run_output_limit_bytes >= 0),
    updated_at_us INTEGER NOT NULL
) STRICT;
INSERT INTO settings VALUES (1, 16, '/usr/local/bin:/usr/bin:/bin', 10000, NULL, 268435456, 10485760, 0);

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL COLLATE BINARY,
    description TEXT,
    tags_json TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    created_at_us INTEGER NOT NULL,
    updated_at_us INTEGER NOT NULL,
    removed_at_us INTEGER,
    current_revision INTEGER NOT NULL CHECK (current_revision > 0),
    FOREIGN KEY (id, current_revision) REFERENCES job_revisions(job_id, revision)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE job_revisions (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL CHECK (revision > 0),
    definition_json TEXT NOT NULL,
    created_at_us INTEGER NOT NULL,
    created_by TEXT NOT NULL CHECK (created_by IN ('add', 'update', 'import')),
    PRIMARY KEY (job_id, revision)
) STRICT;

CREATE UNIQUE INDEX jobs_live_name ON jobs(name COLLATE BINARY) WHERE removed_at_us IS NULL;
CREATE INDEX jobs_enabled_updated ON jobs(enabled, updated_at_us, id) WHERE removed_at_us IS NULL;

CREATE TABLE schedule_cursors (
    job_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    cursor_us INTEGER NOT NULL,
    interval_anchor_us INTEGER,
    one_time_resolved INTEGER NOT NULL DEFAULT 0 CHECK (one_time_resolved IN (0, 1)),
    updated_at_us INTEGER NOT NULL,
    PRIMARY KEY (job_id, revision),
    FOREIGN KEY (job_id, revision) REFERENCES job_revisions(job_id, revision) ON DELETE RESTRICT
) STRICT;

CREATE TABLE scheduler_lifetimes (
    id TEXT PRIMARY KEY,
    pid INTEGER NOT NULL CHECK (pid > 0),
    binary_version TEXT NOT NULL,
    started_at_us INTEGER NOT NULL,
    heartbeat_at_us INTEGER NOT NULL,
    ended_at_us INTEGER,
    exit_class TEXT CHECK (exit_class IS NULL OR exit_class IN ('clean', 'fatal', 'replaced', 'stale'))
) STRICT;
CREATE INDEX lifetimes_open ON scheduler_lifetimes(ended_at_us, started_at_us, id);

CREATE TABLE admission_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    last_admitted_job_id TEXT,
    next_queue_sequence INTEGER NOT NULL CHECK (next_queue_sequence > 0),
    FOREIGN KEY (last_admitted_job_id) REFERENCES jobs(id) ON DELETE SET NULL
) STRICT;
INSERT INTO admission_state VALUES (1, NULL, 1);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE RESTRICT,
    revision INTEGER NOT NULL,
    trigger TEXT NOT NULL CHECK (trigger IN ('scheduled', 'catch_up', 'manual')),
    nominal_us INTEGER,
    requested_at_us INTEGER NOT NULL,
    eligible_at_us INTEGER NOT NULL,
    queue_sequence INTEGER NOT NULL UNIQUE CHECK (queue_sequence > 0),
    snapshot_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued', 'starting', 'running', 'retry_wait', 'succeeded', 'failed', 'timed_out', 'cancelled', 'skipped_overlap', 'skipped_concurrency', 'interrupted_unknown')),
    reason TEXT,
    catch_up_batch TEXT,
    catch_up_position INTEGER CHECK (catch_up_position IS NULL OR catch_up_position > 0),
    replacement_candidate INTEGER NOT NULL DEFAULT 0 CHECK (replacement_candidate IN (0, 1)),
    cancellation_requested_at_us INTEGER,
    cancellation_reason TEXT,
    finished_at_us INTEGER,
    FOREIGN KEY (job_id, revision) REFERENCES job_revisions(job_id, revision) ON DELETE RESTRICT,
    CHECK ((trigger = 'manual' AND nominal_us IS NULL) OR (trigger != 'manual' AND nominal_us IS NOT NULL)),
    CHECK ((catch_up_batch IS NULL) = (catch_up_position IS NULL))
) STRICT;
CREATE UNIQUE INDEX runs_scheduled_occurrence ON runs(job_id, revision, nominal_us) WHERE trigger != 'manual';
CREATE UNIQUE INDEX runs_replacement_candidate ON runs(job_id) WHERE replacement_candidate = 1;
CREATE INDEX runs_admission ON runs(state, eligible_at_us, queue_sequence, job_id);
CREATE INDEX runs_history ON runs(job_id, requested_at_us DESC, id);
CREATE INDEX runs_retention ON runs(state, finished_at_us, id);

CREATE TABLE attempts (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE RESTRICT,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    lifetime_id TEXT NOT NULL REFERENCES scheduler_lifetimes(id) ON DELETE RESTRICT,
    state TEXT NOT NULL CHECK (state IN ('starting', 'running', 'succeeded', 'failed', 'timed_out', 'cancelled', 'interrupted_unknown')),
    started_at_us INTEGER NOT NULL,
    running_at_us INTEGER,
    finished_at_us INTEGER,
    duration_us INTEGER CHECK (duration_us IS NULL OR duration_us >= 0),
    resolved_executable TEXT,
    process_id INTEGER,
    process_group_id INTEGER,
    exit_code INTEGER,
    http_status INTEGER CHECK (http_status IS NULL OR http_status BETWEEN 100 AND 599),
    result_class TEXT,
    error_message TEXT,
    PRIMARY KEY (run_id, attempt_number)
) STRICT;
CREATE INDEX attempts_active ON attempts(state, lifetime_id, started_at_us, run_id);

CREATE TABLE retry_intents (
    run_id TEXT PRIMARY KEY REFERENCES runs(id) ON DELETE CASCADE,
    prior_attempt_number INTEGER NOT NULL CHECK (prior_attempt_number > 0),
    not_before_us INTEGER NOT NULL,
    classification TEXT NOT NULL,
    created_at_us INTEGER NOT NULL,
    FOREIGN KEY (run_id, prior_attempt_number) REFERENCES attempts(run_id, attempt_number) ON DELETE CASCADE
) STRICT;
CREATE INDEX retries_due ON retry_intents(not_before_us, run_id);

CREATE TABLE events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at_us INTEGER NOT NULL,
    kind TEXT NOT NULL,
    job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    details_json TEXT NOT NULL
) STRICT;
CREATE INDEX events_job ON events(job_id, id);
CREATE INDEX events_run ON events(run_id, id);

CREATE TABLE output_artifacts (
    run_id TEXT NOT NULL,
    attempt_number INTEGER NOT NULL,
    relative_path TEXT NOT NULL UNIQUE,
    state TEXT NOT NULL CHECK (state IN ('pending', 'active', 'finalized', 'prune_pending', 'pruned', 'missing')),
    retained_payload_bytes INTEGER NOT NULL DEFAULT 0 CHECK (retained_payload_bytes >= 0),
    physical_bytes INTEGER NOT NULL DEFAULT 0 CHECK (physical_bytes >= 0),
    discarded_bytes INTEGER NOT NULL DEFAULT 0 CHECK (discarded_bytes >= 0),
    truncated INTEGER NOT NULL DEFAULT 0 CHECK (truncated IN (0, 1)),
    truncated_at_us INTEGER,
    finalized_at_us INTEGER,
    prune_started_at_us INTEGER,
    pruned_at_us INTEGER,
    PRIMARY KEY (run_id, attempt_number),
    FOREIGN KEY (run_id, attempt_number) REFERENCES attempts(run_id, attempt_number) ON DELETE RESTRICT
) STRICT;
CREATE INDEX output_retention ON output_artifacts(state, finalized_at_us, run_id, attempt_number);
