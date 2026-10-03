# Native CLI fixture portability, 2026-10-04

Status: reviewed plan before Source. Owning Issue31 is in progress. SPEC is frozen;
there is no product/API behavior or platform-support amendment. Base4ab7b9cfcef0b53d21814721126fe59d96442230
has the same complete tree as reviewed10fe. Separate read-only report/receipt hashes
are recorded in FINDINGS. Root owns planning/review/publication; a separate developer
owns only `crates/locron-cli/tests/cli.rs` after exact Issue Verify readback.

The selected first slice changes prerequisites inside these existing selectors:

- due_one_time_job_catches_up_once_and_disables
- explain_distinguishes_success_only_history_from_an_active_latest_run
- explain_allows_latest_run_and_anomaly_to_match_then_retains_an_older_anomaly
- human_run_wait_streams_and_prints_the_terminal_outcome_line
- human_why_run_prints_immutable_run_facts
- human_add_update_enable_disable_remove_print_outcome_lines
- human_list_aligns_columns_across_name_widths
- human_list_all_marks_disabled_jobs_no
- disconnecting_run_wait_does_not_cancel_the_durable_run
- run_wait_streams_all_attempts_and_maps_target_outcomes
- durable_cancel_terminates_a_running_process (shell input only; its readiness remains pending)

Add only narrow helper definitions required by those existing functions. Keep all71
selectors, every assertion/outcome/terminal frame/attempt count/retry delay/schedule,
all original absolute clocks, genuine daemon kill/reap/restart and private-state cleanup.
No new #[test], skip/ignore, removed branch, weakened substring/table assertion,
synthetic returned success, added deadline, warm-up, worker reset or whole-suite serialization.
All other functions and Source blobs/modes remain exact, including both doctor selectors,
human_forms' doctor branch and wake_socket_makes_new_manual_run_promptly_visible_to_daemon.
The cancellation case's existing readiness loop is not altered or declared qualified.

1. **Finite native process input.** On Windows select the actual absolute UTF-8
   cargo_bin!("locron") image and fixed `--help` argv for selected successful-process
   fixtures. Fail explicitly if an absolute/UTF-8 native image cannot be represented;
   never substitute an arbitrary ambient PATH program or fictional executable. Unix
   retains `/usr/bin/true` and its exact original argv. Preserve definition changes,
   run admission/history/explanation/one-time disable semantics and dry-run non-mutation.
   **Verify:** reversible platform transformations conserve every Unix body/value/argv;
   actual fresh native selected cases run the built image and keep original durable facts.

2. **Explicit native shell inputs.** Keep Target::Shell. Windows uses only the validated
   absolute SystemRoot/System32/WindowsPowerShell/v1.0/powershell.exe through existing
   --shell-executable; shell_arguments supplies the existing no-profile/noninteractive
   flags. Use static .NET-only bodies: Sleep(1000)+Console.Write('survived'); Sleep(30000)
   for cancellation; retry marker via IO.File.Exists/WriteAllText($env:MARKER,'') with
   Console.Write('first')+exit7 or Console.Write('second')+exit0; separate Console.Write('failure')
   +exit9. Actual syntax is `exit 7/0/9`. Marker path stays environment data, never injected
   into PowerShell source. No execution-policy override/profile/tool installation or
   surrogate process replaces shell semantics. Unix scripts/executable defaults stay exact.
   This qualifies the supported explicit PowerShell contract only. **Verify:** exact output,
   retry1s/two attempts, genuine1s running window/disconnect survival and real30s cancellable
   process use unchanged assertions/clocks; a wake-readiness failure remains visible.

3. **Complete native human expectations.** Select the normalized absolute native target
   and its actual --help args for the selected cases; render independently computed full
   expected lines/tables with original headers/order/column widths/disabled marker. Preserve
   byte-equality table assertions and exact labels. Do not call the production human renderer
   to construct its own oracle, hide targets or relax equality to contains. Existing Unix
   literal snapshots remain exact. **Verify:** native complete byte outputs match expected
   target/argv and alignment; Unix expected strings and all other predicates are conserved.

4. **Review and measured publication.** Developer returns one-file diff/format checks,
   transformation/selector/assertion/clock inventory and all other blob/mode conservation.
   Root reviews all changes/context and deterministic OCR coverage before normal publication.
   Root may dispatch existing exploratory native x64stable/ARMstable/x64MSRV only for changed
   reviewed Source, check actual compiler/host and nonzero named selections/full71 results,
   and retain newly uncovered failures. Required ordinary CI must pass before any merge.
   **Verify:** Rust1.94/1.98 formatting, whitespace, exact preserved native/Unix contracts and
   fresh raw native results are recorded; no unsupported local typecheck/native claim.

Expected full success is deliberately not inferred. Existing doctor/wake readiness, feedback,
POSIX install/self-update versus actual native updater, variable Core ownership failures and
the41 full Clippy diagnoses remain visible. Unused updater/index wrappers are test-only;
do not wire live effects/cfg widening or suppress warnings. Actual two-user privacy,
standard-user Windows11 install/reboot, active daemon recovery and public unsigned
release/install/update/remove/WinGet acceptance are separate open gates. Signing37 is deferred.
