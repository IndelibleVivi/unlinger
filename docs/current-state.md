# Current state

**Updated:** 2026-08-30
**Programme:** Unlinger 0.1
**Source tranche:** P0/P1 source candidate plus partial P2 daemon surface and first isolated enforcement evidence
**Remote:** private origin configured
**Activation:** not installed; not activated; one isolated enforcement run completed; ambient enforcement not enabled

The workspace now contains native macOS snapshots, process identity/graph/sessionization, embedded rules, deterministic gates, durable 90-second cooling history, frozen cleanup plans, fresh revalidation, exact TERM/KILL sequencing, post-scan/revival handling, completion-timed redacted SQLite receipts, 0600 Unix IPC, a periodic daemon, and the complete specified CLI command set. `unlingerd` defaults to report-only; `--enforce` has completed one owner-approved isolated Chrome-for-Testing field run and has not been installed or enabled against ambient machine state.

Current verification: `cargo fmt --all -- --check`, strict workspace clippy, and all 34 ordinary Rust tests pass; the explicit live CfT test remains ignored by default. The suite includes a strict red/green regression for detached crashpad helpers, deterministic terminal completion timestamps, and a pure test proving the live harness rejects identities not admitted from its field profile. The exact macOS signal integration still targets only an isolated child created by the test.

A live report-only smoke used a temporary database/socket: 452/452 current-user processes were inspected, zero were unreadable, the snapshot completed in 28 ms, no incident was present, and CLI status/history/pause/resume/doctor completed through IPC. The temporary daemon was stopped. This is read-only/local-control evidence only.

The first isolated enforcement used Chrome for Testing 151.0.7922.34 with a dedicated temporary profile and simultaneous ordinary Chrome. The supported incident survived the full 90-second durable grace, reached CONFIRMED with eight exact members, received root TERM followed by exact KILL only after revalidation, and ended CLEARED with zero survivors and two completed revival checks. Ordinary Chrome remained live. A second host-level dry-run verified that the crashpad-helper fix leaves one main COOLING incident and no standalone crashpad noise. All temporary field artifacts were moved to Trash after exact process checks.

An ignored, explicit CfT harness now creates a unique profile, refuses stable Chrome, preflights unrelated COOLING incidents, and rejects any signal not admitted from its exact profile tree. Its first fast run passed in 14.41 seconds: one ordinary Chrome root retained exact identity, the field root exited after delivered TERM, zero survivors remained, and two revival checks completed. The live SQLite terminal event was 4.194 seconds later than RECLAIMING, verifying the completion-time fix on the host. The retained profile/evidence was moved recoverably to Trash after process-reference checks.

After the harness and test-race fixes, a fresh native doctor inspected 388/388 current-user processes with zero unreadable entries in 64 ms, and the final report-only dry-run found no incident. No daemon or field browser remains active.

Open gates: broader real supported-family corpus and chaos matrix, exit/wake/memory-pressure events, low-risk runtime-artifact cleanup, LaunchAgent install/rollback, daemon longevity/overhead, Intel/universal build, signing/notarization, distribution, multi-day ambient dogfood, and public-alpha evidence.

Before public visibility: confirm license/rights, add bilingual reader documentation and a publication-grade architecture diagram, complete field evidence, replace observational support statements with proven ranges, and repeat tracked/staged privacy and provenance scans. Private continuity, origin conversation, diagnostics, and live captures stay outside Git.
