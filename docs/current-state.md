# Current state

**Updated:** 2026-08-30
**Programme:** Unlinger 0.1
**Source tranche:** P0/P1 source candidate plus active private P2 LaunchAgent dogfood
**Remote:** private origin configured
**Activation:** per-user LaunchAgent installed and loaded; final mode enforce; short-duration dogfood active

The workspace now contains native macOS snapshots, process identity/graph/sessionization, embedded rules, deterministic gates, durable 90-second cooling history, frozen cleanup plans, fresh revalidation, exact TERM/KILL sequencing, post-scan/revival handling, completion-timed redacted SQLite receipts, blocking 0600 Unix IPC, a periodic daemon, the specified ordinary CLI, and a transactional service lifecycle. `unlingerd` still defaults to report-only when invoked directly. The installed plist explicitly selects enforce for the first owner-approved private dogfood host.

Current source verification: `cargo fmt --all -- --check`, strict workspace clippy, all 46 ordinary Rust tests, and the optimized workspace build pass; the explicit live CfT cleanup remains ignored by default. New tests cover SIGTERM clean exit/socket removal, shutdown suppression of new cleanup, plist parsing, transactional failure rollback, concurrent service-command exclusion, managed-path refusal, permission reporting, CLI socket compatibility, and service-path contracts. The exact ordinary macOS signal test still targets only an isolated child created by the test.

A live report-only smoke used a temporary database/socket: 452/452 current-user processes were inspected, zero were unreadable, the snapshot completed in 28 ms, no incident was present, and CLI status/history/pause/resume/doctor completed through IPC. The temporary daemon was stopped. This is read-only/local-control evidence only.

The first isolated enforcement used Chrome for Testing 151.0.7922.34 with a dedicated temporary profile and simultaneous ordinary Chrome. The supported incident survived the full 90-second durable grace, reached CONFIRMED with eight exact members, received root TERM followed by exact KILL only after revalidation, and ended CLEARED with zero survivors and two completed revival checks. Ordinary Chrome remained live. A second host-level dry-run verified that the crashpad-helper fix leaves one main COOLING incident and no standalone crashpad noise. All temporary field artifacts were moved to Trash after exact process checks.

An ignored, explicit CfT harness now creates a unique profile, refuses stable Chrome, preflights unrelated COOLING incidents, and rejects any signal not admitted from its exact profile tree. Its first fast run passed in 14.41 seconds: one ordinary Chrome root retained exact identity, the field root exited after delivered TERM, zero survivors remained, and two revival checks completed. The live SQLite terminal event was 4.194 seconds later than RECLAIMING, verifying the completion-time fix on the host. The retained profile/evidence was moved recoverably to Trash after process-reference checks.

Before installation, the release doctor inspected 417/417 current-user processes with zero unreadable entries, and a dry-run found no incident while a dedicated Chrome-for-Testing session and ordinary Chrome were both active. Persistent report-only mode then completed multiple 60-second sweeps with a stable launchd-owned PID, zero confirmed/ambiguous incidents, private file modes, and an empty service log.

The first installed candidate exposed intermittent IPC timeouts caused by nonblocking `accept` plus a polling sleep under launchd background scheduling. The listener now blocks on the socket and wakes through the exact owned socket during shutdown. A transactional report-only reinstall passed, followed by two 40-request bursts at concurrency 8 without error.

Mode acceptance exercised report-only → enforce → report-only → enforce. Each reload used a distinct launchd-owned PID, exact launchd/IPC PID and mode agreement, private permissions, and a completed first scan; old PIDs exited. Ordinary Chrome and the dedicated CfT process retained their identity summaries throughout. Final enforce stayed healthy across subsequent sweeps with zero incidents/history, a 0-byte log, no IP socket, and an idle point of 0.0% CPU/720 KiB RSS. The latest pre-commit source doctor inspected 386/386 processes with zero unreadable entries in 58 ms. This proves activation and operational mode rollback, not ambient eligible-incident cleanup or sustained acceptance.

Open gates: broader real supported-family corpus and chaos matrix, exit/wake/memory-pressure events, low-risk runtime-artifact cleanup, daemon restart during cooling, sleep/wake survival, sustained longevity/overhead, Intel/universal build, signing/notarization, versioned distribution rollback, multi-day ambient dogfood, and public-alpha evidence.

Before public visibility: confirm license/rights, add bilingual reader documentation and a publication-grade architecture diagram, complete field evidence, replace observational support statements with proven ranges, and repeat tracked/staged privacy and provenance scans. Private continuity, origin conversation, diagnostics, and live captures stay outside Git.
