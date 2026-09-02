# Safety model

Unlinger treats process termination as a deterministic authorization problem, not a stale-process score.

## Automatic-cleanup gate

An incident may enter a frozen cleanup plan only when all gates are true:

```text
same_user
AND strong_automation_provenance
AND confirmed_abandonment
AND isolated_session
AND stable_across_two_observations
AND process_identity_unchanged
AND no_protection_rule
```

`confirmed_abandonment` is durable history, not a synonym for orphan PPID. A redacted SQLite cooling record must retain the same tracking key, root identity, and member fingerprint for the 90-second grace. Changed identity/membership, a non-COOLING state, or a continuity gap longer than 120 seconds resets it. Two observations separated by 15 seconds independently prove short-term stability.

## Hard protections

Unlinger rejects another user, UID 0, system/root processes, itself or its ancestors, ordinary browsers and standard profiles, headed/manual automation, attached CDP browsers, persistent or shared profiles, and any process whose graph, arguments, executable identity, or birth identity is incomplete or contradictory. One macOS exception is explicitly proved rather than guessed: a PID whose `KERN_PROC_PID` status is `SZOMB` is an exited zombie and therefore gone for live-identity purposes. It is omitted from the snapshot/lookup instead of poisoning coverage as an unreadable live process; every other unexplained read failure remains fail closed.

Pressure, CPU, RSS, age, orphan PPID, names, and flags may affect scan urgency or explanation. They never remove a protection or satisfy a missing hard gate.

## Frozen execution

The cleanup engine freezes exact targets and evidence from a CONFIRMED report. Before each signal stage it captures a fresh native snapshot, rebuilds classification, verifies that the incident has not acquired a protection or new member, and resolves every surviving target by PID, start microseconds, executable device, and executable inode. It durably commits a PREPARED action before every possible signal delivery. A restart converts any prepared-but-unfinished delivery into `delivery_unknown`, blocks automatic retry and managed arming, and never resends it without an explicit incident retry authorization.

Execution order is controller/root TERM, bounded grace, remaining-member TERM, bounded grace, then KILL only for exact survivors that already received TERM. The implementation never uses `killall`, process-name `pkill`, or a broad process-group kill. A fresh post-KILL scan is required before CLEARED. Terminal receipts are rebuilt from canonical durable action rows rather than trusting caller-supplied action data.

Revival checks occur at 15 and 60 seconds. A matching session becomes REVIVED and stops; no automatic kill/revival loop is permitted. Runtime failures after a delivered signal retain the actions in a terminal FAILED receipt before the daemon reports unhealthy.

A terminal receipt does not let artifact outcome erase process outcome. Once exact liveness and all revival checks prove the tree gone, an artifact refusal with a known no-removal disposition (`unsafe`, `referenced`, `identity_mismatch`, or `rejected`) projects `process_outcome = cleared`, `artifact_outcome = residue`, and `overall_outcome = cleared_with_residue`. The whole-plan incident state remains FAILED and keeps its named attention/retry block, but that known residue alone does not fail the entire managed daemon closed. Any signal/artifact `delivery_unknown`, open PREPARED action, or failure after a delivered side effect whose terminal result cannot be proved still triggers global fail-close.

SIGTERM/SIGINT request a graceful daemon shutdown. A requested shutdown prevents a new cleanup from starting after observation; an already-started attempt either completes its terminal receipt or records the exact pre-delivery cancellation/interruption state. The IPC server uses its exact owned socket as a blocking wake source and removes that socket on clean exit. Raw OS-signal shutdown does not masquerade as an explicit service drain: a same-generation replacement may carry only the durable requested-enforce intent, starts effective report-only, completes recovery and a signal-free first scan, and then creates a fresh enforcement epoch only when no open-attempt or delivery-unknown blocker exists. `Disarm`, `BeginDrain`, managed failure, and a generation change clear that intent.

`Failed` is terminal for the current managed instance. A later successful scan cannot treat it as FirstScanReportOnly or mark it ready. `Disarm` may retry and persist the fail-close but preserves Failed/unhealthy/not-ready; a service transaction may continue only through exact generation/instance validation, `BeginDrain`, captured-process bootout, and a fresh replacement. Stable report-only acceptance requires both scan and cleanup activity to be false. Arm checks the volatile lifecycle phase as well as durable generation/epoch state, so stale ReadyEnforce storage cannot reopen a live signal gate after a fail-close.

Pause/resume, named retry and exact protection changes are evaluated through one shared public-action policy. For schema-v3/v4 frontend mutations, the durable state change, typed mutation receipt and durable cleanup-policy revision advance in one immediate SQLite transaction while the ControlPlane lifecycle/status gate is held; the in-memory revision is only a post-commit mirror. Exact receipt replay occurs before current lifecycle policy and never advances state or revision again. Transaction failure advances nothing. Schema-v1 CLI/service compatibility remains separate.

The App writes namespace, mutation UUID and canonical intent to an owner-private crash-durable journal before any socket byte. Any post-send untrusted response remains delivery-uncertain; App restart and “check again” use only read-only mutation status and never resend the original command. A receipt can be absent conclusively only while the same receipt-authority namespace remains current. Authority loss retains the unresolved lock. A UI capability or visual dismissal never authorizes or erases a mutation.

A cycle must observe the same cleanup-policy revision and enforcement epoch again immediately before opening a cleanup attempt and before every signal. In particular, a retry committed after confirmation invalidates that cycle, clears only the named block/cooling candidate, and requires fresh cooling; it is not an immediate resend authorization.

Native process-exit, wake, and memory-pressure events are bounded scheduling hints. They never authorize cleanup or weaken the age, version, cooling, identity, or protection gates, and a failed event source falls back to periodic reconciliation. The product has no admitted threshold for notifying on an ambiguous incident under sustained pressure; an aggregate ambiguous count is not notification authority. Native App notifications derive only from trusted typed event/storage/daemon facts, baseline retained tokens on first refresh, and use durable claim-before-schedule duplicate avoidance. App mutation uncertainty is not a backend cleanup notification.

After the exact tree is gone and both revival checks pass, the cleanup engine can evaluate at most one admitted `DevToolsActivePort` file. The current process-only policy does not reach that path: every `0.3.0` pack sets `devtools_active_port = false`, the analyzer emits no runtime-artifact candidate, and both receipt and durable journal must contain zero artifact actions. Profiles, browser data, sockets, PID files, and runtime directories are never automatically deleted.

The dormant DAP path brackets a targeted Darwin `proc_listpidspath` query for the exact canonical pathname with frozen parent/file identity checks, completes a current-user argv scan, writes a durable PREPARED artifact row, atomically renames the exact entry to an exclusive same-directory quarantine name, repeats the targeted query against that actual pathname, and revalidates before unlink. Any incomplete query, process metadata, argv, ownership, type, device/inode/mode/link, parent, or pathname identity returns a non-removal disposition.

Two artifact P2s remain open and are not hidden by the fail-closed source tests or the historical controlled removals. A daemon crash after canonical-to-quarantine rename can strand the exact quarantine entry because restart recovery does not yet journal/reconcile its private random name. A same-UID adversary can still target the final interval between pathname `fstatat` revalidation and `unlinkat`. Generation 9 and generation 13 each removed one live DAP inside a controlled managed harness; those point results do not resolve the source-level residuals or authorize re-enabling artifact eligibility.

## Activation boundary

These paths have synthetic/owned-child verification, historical owner-approved full-timing Chrome-for-Testing enforcement results, and an opt-in fast Field Lab result. The harness admits exact identities only from its unique test profile tree and rejects every out-of-scope signal before the macOS adapter.

Accepted generation 15 and the schema-v4 App are installed and loaded for private dogfood. Generation 15 actively enforces the `0.3.0` process-only policy after exact-head CI, candidate-specific rollback/reinstall, controlled full-timing cleanup/restart, final containment, explicit arm and a stable later sweep. Direct `unlingerd` invocation still defaults to report-only.

This establishes installation, launchd ownership, persistent reconciliation, protected-session coexistence, and current process-only activation on one host. Generations 9 and 13 passed historical process/artifact runs; generation 15 separately passed the exact process programme with zero artifact candidates and zero receipt/journal artifact actions. Its later sweep kept four CfT 152 sessions protected. None of this establishes live wake/pressure, sleep survival, an ambient ordinary eligible-incident cleanup, multi-day zero-false-positive, broad family/version support, signing, or release readiness.

Generation 15 is accepted and has no pending rollback lease. Candidate A generation 14 began report-only, retained generation 13 through a fresh acceptance-scoped lease, passed exact report-only restart, and executed real rollback to healthy generation 13. Candidate B reinstalled the same exact source as generation 15, repeated readiness/restart/App checks, and was accepted before field activation. `DatabaseBackedUp` remains a conservative restore boundary, durable `RollbackInProgress` makes physical rollback replayable across candidate/prior/mixed selection cuts, and `AcceptanceInProgress` still recovers to the prior generation; only durable `Accepted` retires rollback material.
