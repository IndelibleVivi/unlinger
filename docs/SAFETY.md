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

Unlinger rejects another user, UID 0, system/root processes, itself or its ancestors, ordinary browsers and standard profiles, headed/manual automation, attached CDP browsers, persistent or shared profiles, and any process whose graph, arguments, executable identity, or birth identity is incomplete or contradictory.

Pressure, CPU, RSS, age, orphan PPID, names, and flags may affect scan urgency or explanation. They never remove a protection or satisfy a missing hard gate.

## Frozen execution

The cleanup engine freezes exact targets and evidence from a CONFIRMED report. Before each signal stage it captures a fresh native snapshot, rebuilds classification, verifies that the incident has not acquired a protection or new member, and resolves every surviving target by PID, start microseconds, executable device, and executable inode.

Execution order is controller/root TERM, bounded grace, remaining-member TERM, bounded grace, then KILL only for exact survivors that already received TERM. The implementation never uses `killall`, process-name `pkill`, or a broad process-group kill. A fresh post-KILL scan is required before CLEARED.

Revival checks occur at 15 and 60 seconds. A matching session becomes REVIVED and stops; no automatic kill/revival loop is permitted. Runtime failures after a delivered signal retain the actions in a terminal FAILED receipt before the daemon reports unhealthy.

SIGTERM/SIGINT request a graceful daemon shutdown. A requested shutdown prevents a new cleanup from starting after observation, while an already-started cleanup runs through its terminal receipt. The LaunchAgent grants 120 seconds before forced termination; the IPC server uses its exact owned socket as a blocking wake source and removes that socket on clean exit.

## Activation boundary

These paths have synthetic/owned-child verification, one owner-approved full-timing Chrome-for-Testing enforcement result, and one opt-in fast Field Lab harness result. The harness admits exact identities only from its unique test profile tree and rejects every out-of-scope signal before the macOS adapter.

The first owner-approved per-user LaunchAgent is now installed and loaded for private ambient dogfood. Persistent report-only operation crossed multiple real sweeps before mode activation. The service then completed an enforce → report-only → enforce reload/rollback sequence, with a fresh PID and completed first scan in every mode. Ordinary Chrome and an active dedicated Chrome-for-Testing session retained their process identities; no incident or cleanup receipt was produced. The final installed mode is enforce, while direct `unlingerd` invocation still defaults to report-only.

This establishes installation, launchd ownership, persistent reconciliation, protected-session coexistence, and operational mode rollback on one host. It does **not** establish ambient supported-incident cleanup, multi-day zero-false-positive acceptance, sleep/wake survival, broad family/version safety, signing, or release readiness.

No source path currently deletes profiles or runtime artifacts. Exit/wake/pressure event integration, the broader supported-family matrix, chaos completion, and sustained field false-positive evidence remain separate gates.
