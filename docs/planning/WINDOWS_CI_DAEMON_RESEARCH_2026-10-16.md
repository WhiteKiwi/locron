# Windows CI daemon readiness and lifecycle research

Research source: `f8d77077967b9aab1a87b0397a84dfd0204bbf54`, whose daemon fixture
and product paths below are unchanged from the failed product source
`536671f80b9ea4b17f4fc0362b3fa0ba60e43607`. Owning execution issues remain #172
and #31. The user selected all current foundation failures. This document proposes
the next narrow fixture correction; parent review, the applicable implementation
amendment and issue readbacks must precede source edits. No source, test, workflow,
runtime or production-state changes were made for this research.

## Actual receipts and what they establish

Run [38850575751](https://github.com/WhiteKiwi/locron/actions/runs/38850575751)
contains x64 job
[116810653593](https://github.com/WhiteKiwi/locron/actions/runs/38850575751/job/116810653593)
and ARM job
[116810653525](https://github.com/WhiteKiwi/locron/actions/runs/38850575751/job/116810653525).
The three selectors below fail on x64 and pass on ARM in that same run.

| x64 selector | Actual first result and safe observations |
| --- | --- |
| `wake_socket_makes_new_manual_run_promptly_visible_to_daemon` | Expired/Connect; zero frame bytes; PipeOpen/NotFound/raw2. Total elapsed983035us; daemon spawn returned376978us from fixture origin. Later bounded diagnostic ends at begin_lifetime/ok,596094us on its separate daemon clock. |
| `durable_cancel_terminates_a_running_process` | Expired/Connect; zero frame bytes; PipeOpen/NotFound/raw2. Total elapsed1007432us; daemon spawn returned376545us from fixture origin. Last daemon diagnostic is store_open/enter,411719us on its separate clock, without a returned stage/code. |
| `windows_cli_control::native_cli_output_capture_contract` | Cancellation positive control returns Run/Progress at4886826us. The original authenticated probe exchanged31 bytes. Initial Run exits0 and records enqueue/hint success. First history observation is scalar state5; progress is absent. Matched daemon attempt records mark_running=true and execution/enter, without execution/completion return. |

The connection probe is **200ms**, not one second. `Owner::probe` establishes it
at entry before runtime construction, endpoint open, peer queries or frame I/O.
The approximately one-second receipts include earlier fixture setup and ownership
checks. Case deadlines are still5s/8s after actual daemon spawn return, clipped by
the original30s outer horizon; capture controls share their original outer origin.

History scalar5 means **other**, including `starting`; it does not establish a
failed or terminal run. No preserved safe receipt identifies which other state
was returned. A starting-state race is a demonstrated logical gap in the fixture,
not the established historical cause of this particular state5 receipt.

`daemon_status={op=CleanupWait,code=1}` appears in both failed x64 and successful
ARM capture. `Owner::reap` emits CleanupWait only after its still-live child has
entered the fixture's kill-and-wait branch. This is owned cleanup evidence, not
proof that the daemon independently failed or exited1.

## Startup ordering and existing observation mechanisms

`tests/support/private_state.rs` creates only a fresh private directory. The
daemon performs first database creation/configuration/migration. Windows
`daemon()` acquires and publishes the daemon role lock before `Store::open`.
The test's `expected_role` proves actual PID, held lock and repeatable metadata;
it does not establish that Store initialization or endpoint binding completed.
Wake binding occurs later in `StoreAdapter::begin_lifetime`, followed by the
durable lifetime operation. The test currently starts its200ms probe directly
after role proof, so cold initialization can consume this probe's entire budget.

Existing debug-only version2 observations already contain role_lock, store_open,
settings and begin_lifetime enter/return pairs. `WakeLedger.ops[5]` retains the
validated begin_lifetime result even after later reconciliation records. An
on-time validated `edge=ok`, returned/nonpending pair is a useful **startup hint**.
It is not endpoint or identity authority: the production wake-binding adapter
deliberately preserves durable fallback when endpoint creation fails, so
begin_lifetime can return successfully even when no wake endpoint exists.

The existing producer reader is retained at cursor0 until exact child reap, then
used for the independent bounded diagnostic and positive-control assertions.
Reading it early or seeking a cloned descriptor would violate that contract.
The emitter and product execution futures must remain unchanged. Existing scalar
snapshots and late diagnostics must not become readiness or ownership authority.

## Recommended minimal source selection, pending parent approval

Select only `crates/locron-cli/tests/support/windows_cli_control.rs`. No product
module, workflow, dependency, generated asset, Unix body or original negative-peer
control needs a source edit for this selection.

| Case | Current prerequisite gap | Proposed prerequisite correction | Success oracle retained |
| --- | --- | --- | --- |
| Genuine Wake | Role metadata can precede cold Store startup and listener creation. | Observe the validated startup return within the same5s/30s, then enter the original200ms probe once. | Exact authenticated peer/ACK/receipt and Succeeded history. |
| Genuine Cancel and cancellation positive control | Same startup gap; Starting is treated as other while progress is awaited. | Same hint within8s/30s; explicitly wait through Starting before Running. | Running plus two progress samples before requesting cancel; exact Cancelled and unchanged later counters. |
| Output capture | Its cancellation positive control can hit the same lifecycle gap. | Reuse that corrected Cancel path without resetting the capture's outer origin. | All original cap/live/collision/identity/producer-return assertions and cleanup remain. |
| Negative peers and listener-free queued control | No genuine-daemon startup prerequisite. | No hint observer or new wait. | Original refusal, frame, expiry and retained-owner checks remain. |
| Cleanup | Every real handle remains owned until exact reap. | Add the independent reader to the same owned release discipline. | First work result, reap, root removal and CLEANED proof remain authoritative. |

1. **Add one test-private independent startup-hint observer for debug daemon cases.**
   Independently open the already-created guarded actual daemon stderr carrier;
   verify its full file identity against the retained original writer. Keep this
   additional reader on the existing Owner through exact reap and release it in
   the existing cleanup path. The original producer reader and writer cursors are
   untouched. Use a total64KiB+1 byte ceiling and the existing version2 decoder,
   expected context, daemon role, sequence and operation validation. Parse only a
   complete newline-terminated prefix; an empty file or incomplete final row is
   pending, never success. The retained bounded prefix allows later file appends
   without interpreting a temporary EOF as a returned daemon result. Malformed,
   wrong-context, wrong-role, rebound or oversized data refuses setup; no raw bytes
   or private identifiers enter rendered diagnostics. `ops[5].edge==Some(1)` with
   an actual returned/nonpending result supplies only the startup hint.
   **Verify:** bounded byte controls exercise empty, split-row/concatenated-append,
   wrong-context/role/sequence, begin_lifetime-enter-only/error, malformed and capped
   snapshots. These are parser controls, not real late-writer or rebound-file
   acceptance. Genuine native cases verify the independently opened reader's actual
   full identity and both original producer reader positions remain0. All file/native checks occur
   on the existing worker under current case/outer deadline checks, with no new
   thread, process, marker write, filesystem polling outside the guarded carrier,
   connection attempt or protocol frame. The new reader is closed only after exact
   child reap, including error paths. Closed operation labels may identify the
   added open/read/identity steps without enlarging existing call-word layouts.
2. **Use the hint only to sequence genuine-daemon setup before the original probe.**
   In debug Wake and Cancel paths, wait for this hint after the original guard,
   live-child and role checks, using the existing5ms checked pause and the remaining
   original5s/8s/30s allowances. Cancellation's positive producer control follows
   that same Cancel path. Then call `probe` exactly once: its200ms expression,
   deadline publication, runtime/open/query ordering, expected PID/role/SID checks,
   ACK/receipt and31-byte exchange remain literal. Recheck the actual child before
   entering the probe. No startup wait runs for foreign/malformed/idle/withheld
   peers, release-profile cases lacking the debug carrier, or listener-free queued
   controls. Such release cases retain their existing behavior and are not claimed
   repaired by this debug foundation fixture correction.
   **Verify:** the source retains exactly one authoritative probe after the hint,
   and existing actual foreign/malformed/idle/withheld native controls retain their
   original200ms expiry/ownership assertions. A resource-free already-expired
   Control plus valid startup bytes verifies that data cannot override the closed
   admission gate. Missing/held real startup returns must expire the original case
   without a named-pipe open; that production-helper boundary is reviewed against
   its checks. Synthetic bytes do not claim an actual held daemon or an actual
   foreign endpoint following a valid hint; those fault scenarios would require a
   separately admitted native fixture and are not claimed by this selection.
3. **Handle the known Starting state without accepting unspecified states.**
   Introduce a closed test-private history classification preserving scalar codes
   queued1/running2/cancelled3/succeeded4/other5 and adding starting6 in the unused
   existing3-bit domain. Retain the layout and old literal word controls; extend
   only History validation to code6 and add an independent code6 round-trip/control.
   `other` remains refusal, including failed/timed_out/interrupted_unknown,
   unexpected retry_wait and unknown text. Permit Starting only while Wake awaits
   success and while Cancel awaits its first Running plus at least two progress
   samples. Post-cancel state/refusal handling stays unchanged.
   **Verify:** explicit queued→starting→running observations do not prematurely
   refuse; starting alone never authorizes cancellation or success. Terminal and
   unknown states still refuse immediately. Cancel still requires exact Cancelled
   history, two progress samples and unchanged later counters; Wake still requires
   exact Succeeded. Existing numeric history5 receipts remain other and are never
   relabeled retroactively. A new actual state6 receipt is required before claiming
   that a particular native failure observed Starting.
4. **Qualify the complete selected helper and unchanged native oracles.**
   Parent review checks the exact helper diff, unchanged product/dependency/workflow
   tree and source inverse for every existing clock, protocol, negative-peer,
   producer capture, cleanup and Unix region outside this selection.
   **Verify:** Rust1.94/1.98 formatting and strict affected Clippy; existing native
   selectors below on x64 stable/MSRV and ARM, all on the same changed source with
   authentic job/toolchain receipts. Full foundation outcomes, PR144/prune residuals
   and unrelated acceptance remain separate and cannot inherit a PASS from this
   subset. No retry, skip, weaker error oracle or elapsed allowance is introduced.

## Bounded controls in the existing capture entry

Run two new pure control functions immediately after the existing
`assert_call_word_domains()` call in `native_cli_output_capture_contract`. Its
original `entered` and `deadline` are already created. Check that same deadline
before and after the control block; do not create a later case origin, add a CI
step or build another binary. The original capture/pair/producer case order stays
unchanged. Controls allocate bounded byte vectors only and perform no filesystem,
process, named-pipe, Store, sleep or worker operations.

The startup control reuses the existing eight startup rows and decoder, proving
that ops[5] is returned/nonpending only after an ordered successful lifetime return.
Use at most32 fixed vectors, including empty, complete, enter-only, error, split
final row then concatenated completion, valid unrelated later rows, wrong context,
wrong role, duplicate/skipped sequence, malformed row and65536/65537-byte boundaries.
An already-expired resource-free Control exercises the same checked decode gate
with valid data; it is an expired-gate control rather than a wall-clock measurement
or real withheld-return test. Temporary EOF and byte append model cases establish
parser behavior only. Actual streaming append, hostile file replacement and a
foreign endpoint after a genuine return hint are not simulated native acceptance.

The history control uses an independent expected matrix: Queued and Starting stay
pending for zero, one or two progress samples; Running stays pending for zero/one
and authorizes cancellation preparation at two. Wake accepts only Succeeded as its
completion. Post-cancel handling accepts only Cancelled as completion. Other known
terminal states, unexpected retry_wait, unknown text and wrong spelling refuse;
Starting never supplies Running or post-cancel completion. Keep the original
history1..5 literal word controls and add code6 round-trip plus code7 refusal.

This adds bounded linear byte/model work, including a65KiB cap-boundary vector,
inside the original30s allowance. No native timing measurement has established its
exact cost; do not promise zero overhead or a specific duration. Native verification
must retain the original outer horizon and report any exhaustion as a failure.

## Alternatives considered

Reusing the producer reader breaks its independent zero-cursor proof. A separate
product ready file, new emitter operation or debug startup channel adds runtime
effects to a problem that already has a bounded private observation carrier.
Opening a pipe merely to test availability and then opening again for the probe
adds a client operation and can consume/replenish a server instance; reject that
approach. `WaitNamedPipe` itself is not a startup barrier when no instance exists,
and interprocess2.4.4 exposes its readiness-only wrapper only as pub(crate). A new
unsafe Windows binding or dependency is unnecessary. Database prewarming would
remove the actual first-use precondition. A fixed sleep cannot establish startup
completion and consumes time without observing the condition.

The selected helper adds one guarded independent read handle and bounded live-file
reads. That is an explicit fixture behavior change requiring review; describing
the observations as hints does not remove those costs. It preserves product
behavior and uses the existing authenticated transaction as the sole protocol
proof. A return hint can still be followed by a genuine endpoint failure; that
failure must remain visible rather than triggering a replay or fallback success.

## Original native Verify commands

Run the existing exact selectors; do not replace their success assertions:

```text
cargo test -p locron --test cli --locked wake_socket_makes_new_manual_run_promptly_visible_to_daemon -- --exact
cargo test -p locron --test cli --locked durable_cancel_terminates_a_running_process -- --exact
cargo test -p locron --test cli --locked windows_cli_control::native_cli_output_capture_contract -- --exact --nocapture
cargo test -p locron --test cli --locked windows_cli_control::foreign_wake_peer_is_refused_before_frame -- --exact
cargo test -p locron --test cli --locked windows_cli_control::malformed_wake_ack_is_refused -- --exact
cargo test -p locron --test cli --locked windows_cli_control::idle_wake_peer_expires_original_probe_deadline -- --exact
cargo test -p locron --test cli --locked windows_cli_control::withheld_native_return_retains_case_owner_at_deadline -- --exact
cargo test -p locron --test cli --locked windows_cli_control::private_peer_publication_preserves_no_clobber_boundary -- --exact
```

## Primary references and remaining uncertainty

[Tokio ClientOptions](https://docs.rs/tokio/latest/tokio/net/windows/named_pipe/struct.ClientOptions.html)
distinguishes absent pipes from busy instances. Locked Tokio1.53.1 source at
`src/net/windows/named_pipe.rs` additionally explains creating the initial server
early and replenishing instances before dispatch to avoid transient NotFound.
[Microsoft WaitNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-waitnamedpipew)
returns immediately when there is no named pipe instance; it does not wait for
initial server creation. [Rust Read](https://doc.rust-lang.org/std/io/trait.Read.html)
documents that a regular-file temporary EOF can later yield appended bytes.

No preserved receipt identifies Cancel's pending Store initialization substage,
the historical scalar5 state's exact text or a native product error behind it.
This plan repairs two independently established fixture assumptions without
claiming either was the sole historical cause. The SID fixture correction and
obsolete-PR cancellation amendment remain independent. Native execution of this
new selection is not performed or claimed by this research document.
