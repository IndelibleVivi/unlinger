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

On macOS, executable dev/inode identity comes from the vnode already mapped by the running process through `PROC_PIDREGIONPATHINFO`; the returned vnode path must equal `pidpath` and the kernel response must contain a successful regular-file vnode stat. The sampler does not reopen that pathname, because it may now name a replacement file rather than the process image and a pathname open on a removable or stalled filesystem can block reconciliation indefinitely. Missing region data, ABI-size mismatch, non-regular or absent vnode stat, path mismatch, invalid size or any native lookup failure leaves executable identity incomplete and therefore cannot authorize a signal.

Pressure, CPU, RSS, age, orphan PPID, names, and flags may affect scan urgency or explanation. They never remove a protection or satisfy a missing hard gate.

Task ownership follows [TASKS.md](TASKS.md). The daemon verifies the exact peer-owned gated child before activation, tracks PID/birth/UID across exec, and releases only on native owner absence. Wrapper death alone is insufficient. A stale reserved-state observation cannot release a concurrently activated task. Release is immutable; new session-name reuse after its birth cutoff stays protected. Native client visibility, named Unix connections and surviving non-browser task work are rechecked before signal delivery. Verified same-bundle/version Crashpad helpers are excluded only from active-work blocking, not admitted as additional signal targets. Task release does not change mode, arm an epoch, reset a protection or increment impact.

An ordinary Playwright CLI controller is more ambiguous: in the evaluated `1.62.1` runtime, the detached daemon persists between one-shot clients, so PPID 1, an old registry timestamp, temporary socket idleness or a 90-second quiet interval cannot distinguish abandonment from a live task that may call again. Source therefore keeps such a controller protected unless an optional host adapter supplies a durable exact owner lifecycle. The source `0.6.0` Playwright pack additionally requires exact controller version `1.62.1`; a lease and process merely agreeing with each other is insufficient. Selector fingerprint, unchanged session name, version, UID, exact executable/birth identity and activation/release window must all match. Active owner, live client, incomplete visibility, controller reuse, a new lease over an older controller, unsupported version, absent adapter or any mismatch preserves protection. Released ownership is abandonment evidence only and never a signal target, mode change, cooling shortcut or cleanup result. Controllerless exact orphan handling remains independent of host integration.

## Frozen execution

The cleanup engine freezes exact targets and evidence from a CONFIRMED report. Before each signal stage it captures a fresh native snapshot, rebuilds classification, verifies that the incident has not acquired a protection or new member, and resolves every surviving target by PID, start microseconds, executable device, and executable inode. It durably commits a PREPARED action before every possible signal delivery. A restart converts any prepared-but-unfinished delivery into `delivery_unknown`, blocks automatic retry and managed arming, and never resends it without an explicit incident retry authorization.

Execution order is controller/root TERM, bounded grace, remaining-member TERM, bounded grace, then KILL only for exact survivors that already received TERM. The implementation never uses `killall`, process-name `pkill`, or a broad process-group kill. A fresh post-KILL scan is required before CLEARED. Terminal receipts are rebuilt from canonical durable action rows rather than trusting caller-supplied action data.

Revival checks occur at 15 and 60 seconds. A matching session becomes REVIVED and stops; no automatic kill/revival loop is permitted. Runtime failures after a delivered signal retain the actions in a terminal FAILED receipt before the daemon reports unhealthy.

A terminal receipt does not let artifact outcome erase process outcome. Once exact liveness and all revival checks prove the tree gone, an artifact refusal with a known no-removal disposition (`unsafe`, `referenced`, `identity_mismatch`, or `rejected`) projects `process_outcome = cleared`, `artifact_outcome = residue`, and `overall_outcome = cleared_with_residue`. The whole-plan incident state remains FAILED and keeps its named attention/retry block, but that known residue alone does not fail the entire managed daemon closed. Any signal/artifact `delivery_unknown`, open PREPARED action, or failure after a delivered side effect whose terminal result cannot be proved still triggers global fail-close.

SIGTERM/SIGINT request a graceful daemon shutdown. A requested shutdown prevents a new cleanup from starting after observation; an already-started attempt either completes its terminal receipt or records the exact pre-delivery cancellation/interruption state. The IPC server uses its exact owned socket as a blocking wake source and removes that socket on clean exit. Raw OS-signal shutdown does not masquerade as an explicit service drain: a same-generation replacement may carry only the durable requested-enforce intent, starts effective report-only, completes recovery and a signal-free first scan, and then creates a fresh enforcement epoch only when no open-attempt or delivery-unknown blocker exists. `Disarm`, `BeginDrain`, managed failure, and a generation change clear that intent.

`Failed` is terminal for the current managed instance. A later successful scan cannot treat it as FirstScanReportOnly or mark it ready. `Disarm` may retry and persist the fail-close but preserves Failed/unhealthy/not-ready; a service transaction may continue only through exact generation/instance validation, `BeginDrain`, captured-process bootout, and a fresh replacement. The same coordinated drain is permitted for an unhealthy, non-ready, non-draining `FirstScanReportOnly` instance only when durable transaction state proves that exact generation/instance is the transaction-owned candidate or prior selection being rolled back. Ordinary install and uninstall still require their stable ready or terminal-failed contract. Stable report-only acceptance requires both scan and cleanup activity to be false. Arm checks the volatile lifecycle phase as well as durable generation/epoch state, so stale ReadyEnforce storage cannot reopen a live signal gate after a fail-close.

Pause/resume, named retry and exact protection changes are evaluated through one shared public-action policy. For schema-v3/v4/v5 frontend mutations, the durable state change, typed mutation receipt and durable cleanup-policy revision advance in one immediate SQLite transaction while the ControlPlane lifecycle/status gate is held; the in-memory revision is only a post-commit mirror. Exact receipt replay occurs before current lifecycle policy and never advances state or revision again. Transaction failure advances nothing. Schema-v1 CLI/service compatibility remains separate.

Source SQLite v10 treats cleanup impact as independent authority rather than reconstructing it from bounded observation history. It retains the v9 action-attribution correction, v8 task ownership and v7 impact/span invariants, then adds private optional session-owner leases without changing cleanup-result authority. A terminal cleanup and its impact row commit atomically; recovery completes the same invariant. Only measurement-complete proved reclaim contributes memory totals, and an exact cleanup event token resolves recent settlement even when a later failed cleanup exists. Equivalent observations coalesce into bounded spans, but neither observation age nor count retention may evict the lifetime aggregate or cleanup detail before its separate minimum window.

The App writes namespace, mutation UUID and canonical intent to an owner-private crash-durable journal before any socket byte. Any post-send untrusted response remains delivery-uncertain; App restart and “check again” use only read-only mutation status and never resend the original command. A receipt can be absent conclusively only while the same receipt-authority namespace remains current. Authority loss retains the unresolved lock. A UI capability or visual dismissal never authorizes or erases a mutation.

A cycle must observe the same cleanup-policy revision and enforcement epoch again immediately before opening a cleanup attempt and before every signal. In particular, a retry committed after confirmation invalidates that cycle, clears only the named block/cooling candidate, and requires fresh cooling; it is not an immediate resend authorization.

Native process-exit, wake, and memory-pressure events are bounded scheduling hints. They never authorize cleanup or weaken the age, version, cooling, identity, or protection gates, and a failed event source falls back to periodic reconciliation. The product has no admitted threshold for notifying on an ambiguous incident under sustained pressure; an aggregate ambiguous count is not notification authority. Native App notifications derive only from trusted typed event/storage/daemon facts, baseline retained tokens on first refresh, and use durable claim-before-schedule duplicate avoidance. App mutation uncertainty is not a backend cleanup notification.

After the exact tree is gone and both revival checks pass, the cleanup engine can evaluate at most one admitted `DevToolsActivePort` file. The current process-only policies do not reach that path: every source and installed pack sets `devtools_active_port = false`, the analyzer emits no runtime-artifact candidate, and both receipt and durable journal must contain zero artifact actions. Profiles, browser data, sockets, PID files, and runtime directories are never automatically deleted.

The source Chrome code-sign clone path reads only the exact current-user temporary clone root, accepts the upstream `.app.bundle` and feature-disabled `.app` directory spellings, opens descendants relative to held directory descriptors without following symlinks, counts only regular-file logical bytes, treats descendant symlinks as leaves, and rejects symlink roots, unsupported nodes and incomplete traversal. Logical bytes remain an APFS accounting observation rather than promised physical reclaim.

Deletion is a distinct enforce-only gate inside that canonical path. Each exact candidate identity must appear unchanged in two consecutive 15-minute storage observations. A complete canonical native process snapshot then decides references per candidate: an executable or absolute argv path within that candidate, or a bundle-confirmed clone-cleanup helper carrying its exact six-character suffix, protects that candidate while stable unreferenced siblings remain eligible. Ordinary Chrome main, renderer, GPU and utility processes outside candidates do not block; a valid unrelated helper suffix also does not block another candidate. A Chrome-looking process with insufficient bundle/path/argv facts, a clone-cleanup helper with a missing, malformed or ambiguous suffix, or incomplete process coverage fails closed for every candidate. Report-only, pause, drain, startup/recovery, non-ready state, concurrent scan/cleanup state or unexpected filesystem shape prevents mutation; a new or changed candidate waits without resetting unchanged siblings. Eligible-subset deletion is descriptor-relative and no-follow, stays beneath scanner-validated candidate directories, rechecks each identity and immediately rescans before persisting public state. Failure retains detected or unavailable truth rather than inventing clear. Raw root/child paths never enter SQLite, IPC, logs or public DTOs; only previous candidate identities are retained in memory, so daemon restart restarts every stability window. This boundary protects ordinary user-computer concurrency; it does not claim a hostile same-UID race-resistant filesystem sandbox. This per-candidate source gate is not installed or field-verified.

The dormant DAP path brackets a targeted Darwin `proc_listpidspath` query for the exact canonical pathname with frozen parent/file identity checks, completes a current-user argv scan, writes a durable PREPARED artifact row, atomically renames the exact entry to an exclusive same-directory quarantine name, repeats the targeted query against that actual pathname, and revalidates before unlink. Any incomplete query, process metadata, argv, ownership, type, device/inode/mode/link, parent, or pathname identity returns a non-removal disposition.

Two artifact P2s remain open and are not hidden by the fail-closed source tests or the historical controlled removals. A daemon crash after canonical-to-quarantine rename can strand the exact quarantine entry because restart recovery does not yet journal/reconcile its private random name. A same-UID adversary can still target the final interval between pathname `fstatat` revalidation and `unlinkat`. Generation 9 and generation 13 each removed one live DAP inside a controlled managed harness; those point results do not resolve the source-level residuals or authorize re-enabling artifact eligibility.

## Activation boundary

These paths have synthetic/owned-child verification, historical owner-approved full-timing Chrome-for-Testing enforcement results, and an opt-in fast Field Lab result. The harness admits exact identities only from its unique test profile tree and rejects every out-of-scope signal before the macOS adapter.

The ordinary daemon default remains report-only. The maintainer's accepted reference installation is generation 30: Playwright `0.6.0`, other packs `0.4.0`, SQLite v10 and frontend schema v5. Generation 29 proved install/restart and real rollback to generation 27; the exact artifact was then freshly installed, accepted and explicitly armed as healthy `ReadyEnforce` generation 30. [Current state](current-state.md) owns the exact generation, activation and controlled-cleanup evidence. Installation and a healthy sweep do not establish multi-day safety, an ordinary ambient eligible cleanup, broad family/version support or release readiness.

A candidate retains rollback material until acceptance. `DatabaseBackedUp` is a conservative restore boundary; durable `RollbackInProgress` makes physical restoration replayable across transaction-owned selection states. `AcceptanceInProgress` recovers to the prior state; only durable `Accepted` retires the lease. Each replacement needs its own applicable install/restart/rollback evidence.

### Native artifact-reference verification gap

On 2026-09-08, parallel native tests intermittently returned no pathname reference despite an owned open ordinary or `O_EVTONLY` descriptor. The same exact tests passed serially; the cause is unresolved. Current packs disable all artifact admission, and process cleanup does not call this pathname query. Resolve this discrepancy before treating the dormant artifact engine as deletion-ready; the existing quarantine/crash and final-path-swap residuals remain separate.


## Observation and attribution candidate (2026-09-09)

The FD-count read from process metadata is only a sizing hint. The native
sampler retries a saturated descriptor list with bounded additional capacity;
errors, repeated saturation, and the capacity ceiling retain incomplete
visibility. An unsaturated enumeration is a bounded point observation, not an
atomic guarantee against later connection or process-identity changes. Existing
per-signal fresh snapshots, identity checks, arming, and client protections stay
in place; no browser versions or artifact admission flags are expanded.

A complete empty classification can support a clear overview. Missing process,
argument, or classification identity facts produce an unknown empty overview;
a known positive report is still shown. Unrelated socket visibility alone does
not turn an otherwise complete empty classification into a global failure.

`cleanup.tree_gone_without_signal` means the process tree was confirmed absent
without a delivered Unlinger signal. Another actor may have ended it; no natural
exit cause is asserted. It remains a terminal absence record, contributes no
reclaimed-session/process/memory impact, and sends no reclaim notification in
the candidate App. Signal attribution is a necessary condition for reclaim
accounting, not proof of exclusive causation. Memory impact remains an RSS-based
estimate, not a physical-memory measurement.

SQLite v9 preserves the original aggregate in `impact_attribution_legacy`, then
rebuilds the public proved-reclaim contribution from retained durable action
evidence in the same transaction. Incomplete retained history is labeled partial;
raw receipts, task authority, pause and protection are not reset. Managed upgrade
and report-only rollback must retain their exact prior-schema backup contract.
