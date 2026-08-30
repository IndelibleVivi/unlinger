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

## Activation boundary

These paths exist and have synthetic/owned-child verification, but ambient enforcement has not been installed or dogfood-accepted. The daemon default is report-only. `--enforce` is an explicit source/field-lab mode, not evidence of activation readiness.

No source path currently deletes profiles or runtime artifacts. launchd activation, exit/wake/pressure event integration, real supported-family cleanup, chaos completion, and field false-positive evidence remain separate gates.
