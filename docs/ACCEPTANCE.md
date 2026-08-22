# Milestone 1 Acceptance Evidence

This matrix maps every frozen completion criterion in [SPEC.md](SPEC.md) to exact automated evidence.
Local Rust 1.94 validation and GitHub Actions run
[32506527959](https://github.com/WhiteKiwi/locron/actions/runs/32506527959) both passed all 194 tests.
The hosted run covered Linux x86_64, Linux arm64, macOS x86_64, and macOS arm64 on Rust 1.94 and
stable. Every job recorded `uname -a` and `rustc -vV`, ran format and Clippy with `-D warnings`, and
executed all workspace targets without filtering the process-lifetime or crash suites.

| # | Completion criterion | Exact automated evidence | Status |
|---:|---|---|---|
| 1 | Register exactly one cron, interval, or one-time schedule. | `locron-cli/acceptance_matrix: cron_every_and_at_jobs_register_and_execute_without_os_scheduler_mutation`; `locron-cli/cli: conflicting_schedule_selectors_fail_without_state` | **PASS** local and official matrix |
| 2 | Register process and HTTP work without OS scheduler edits. | `locron-cli/acceptance_matrix: cron_every_and_at_jobs_register_and_execute_without_os_scheduler_mutation`; `locron-engine/lib: runner::tests::http_500_is_retryable_and_body_is_captured` | **PASS** local and official matrix |
| 3 | Recognize new and updated jobs without restart. | `locron-cli/acceptance_matrix: running_daemon_recognizes_a_job_schedule_and_target_update_without_restart`; `locron-engine/lib: daemon::tests::durable_limit_changes_apply_to_next_admission_without_resizing_or_cancellation` | **PASS** local and official matrix |
| 4 | Complete job CRUD and manual trigger. | `locron-cli/acceptance_matrix: cli_covers_preview_and_the_complete_job_lifecycle`; `locron-store/lib: store::tests::soft_deleted_name_can_be_reused_and_history_survives` | **PASS** local and official matrix |
| 5 | Preview future times before enablement. | `locron-cli/acceptance_matrix: cli_covers_preview_and_the_complete_job_lifecycle`; `locron-cli/bin locron: tests::sparse_cron_does_not_require_full_preview_count` | **PASS** local and official matrix |
| 6 | Persist occurrence identity before execution. | `locron-store/lib: store::tests::duplicate_occurrence_is_idempotent`; `locron-engine/lib: daemon::tests::transient_mark_running_failure_retries_before_spawn` | **PASS** local and official matrix |
| 7 | Expose schedule/start/finish/source/outcome/duration/output. | `locron-cli/acceptance_matrix: scheduled_history_show_why_and_logs_render_available_run_facts`; `locron-cli/attempt_history: history_and_why_expose_complete_ordered_retry_attempts_without_secrets`; `locron-cli/attempt_history: history_persists_final_http_status_and_content_type` | **PASS** local and official matrix |
| 8 | Apply configurable timeout and process-tree cancellation. | `locron-engine/lib: runner::tests::cancellation_kills_a_live_process_grandchild`; `locron-cli/service_lifetime: sigterm_forces_long_process_tree_to_cancel_then_closes_lifetime_and_lock` | **PASS** local and official matrix |
| 9 | Apply explicit overlap and missed-run policies. | `locron-store/lib: store::tests::overlap_trigger_and_capacity_matrix_is_explainable_and_bounded`; `locron-cli/bin locron: tests::catch_up_limit_one_thousand_materializes_compactly_and_admits_oldest_first` | **PASS** local and official matrix |
| 10 | Retry only when configured and retain one ordered attempt history. | `locron-cli/acceptance_matrix: run_wait_renders_ordered_retry_output_frames_for_one_durable_run`; `locron-cli/attempt_history: history_and_why_expose_complete_ordered_retry_attempts_without_secrets` | **PASS** local and official matrix |
| 11 | Prevent restart-driven duplicate one-time execution. | `locron-cli/crash_boundaries: death_before_spawn_never_executes_or_retries_one_time_work`; `locron-store/lib: store::tests::one_time_occurrence_stays_unique_across_lifecycle_fault_boundaries` | **PASS** local and official matrix |
| 12 | Explain or recover unclean-shutdown active runs. | `locron-cli/crash_boundaries: death_after_spawn_recovers_unknown_without_a_second_side_effect`; `locron-cli/crash_boundaries: death_after_target_exit_before_final_commit_never_reexecutes`; `locron-cli/crash_boundaries: restart_does_not_signal_a_stale_process_identity` | **PASS** local and official matrix |
| 13 | Reject invalid schedules and conflicting options before activation. | `locron-cli/cli: conflicting_schedule_selectors_fail_without_state`; `locron-cli/cli: selector_specific_schedule_options_are_rejected_without_state`; `locron-core/lib: target::tests::rejects_relative_http_url_and_two_bodies` | **PASS** local and official matrix |
| 14 | Preserve state across restarts and schema upgrades. | `locron-store/lib: migration::tests::upgrades_existing_v1_database_without_inventing_disabled_history`; `locron-store/lib: migration::tests::upgrades_v3_settings_with_an_empty_global_environment`; `locron-store/lib: migration::tests::upgrades_v4_attempts_with_an_empty_http_content_type` | **PASS** local and official matrix |
| 15 | Automate time, downtime, DST, overlap, timeout, cancellation, retry, and restart tests. | `locron-core/lib: schedule::tests::spring_gap_is_absent_and_fall_fold_occurs_once`; `locron-store/lib: store::tests::overlap_trigger_and_capacity_matrix_is_explainable_and_bounded`; `locron-cli/crash_boundaries`; `locron-cli/service_lifetime` | **PASS** local and official matrix |
| 16 | Simulate mutations/manual runs and explain state without debug logs. | `locron-cli/acceptance_matrix: manual_run_dry_run_reports_eligibility_without_durable_mutation`; `locron-cli/cli: add_dry_run_is_non_mutating_and_machine_readable`; `locron-cli/acceptance_matrix: scheduled_history_show_why_and_logs_render_available_run_facts` | **PASS** local and official matrix |

Milestone-1 implementation and its required platform evidence are complete. Package publication and
the viewer/API, MCP, desktop, and App Store roadmap remain separate programs governed by their own
future specifications.
