# Current state

**Updated:** 2026-08-30
**Programme:** Unlinger 0.1
**Source tranche:** P0/P1 source candidate plus partial P2 daemon surface
**Remote:** private origin configured
**Activation:** not installed; not activated; ambient enforcement not run

The workspace now contains native macOS snapshots, process identity/graph/sessionization, embedded rules, deterministic gates, durable 90-second cooling history, frozen cleanup plans, fresh revalidation, exact TERM/KILL sequencing, post-scan/revival handling, redacted SQLite receipts, 0600 Unix IPC, a periodic daemon, and the complete specified CLI command set. `unlingerd` defaults to report-only; `--enforce` exists in source for isolated acceptance and has not been used against ambient machine state.

Current verification: `cargo fmt --check`, strict workspace clippy, and all 32 Rust tests pass. Tests cover counterexamples, PID reuse, durable grace/reset, TERM-before-KILL, revival bounds, post-signal failure receipts, persistence/retention, IPC, report-only suppression, and an exact macOS signal sent only to a child owned by the test.

A live report-only smoke used a temporary database/socket: 452/452 current-user processes were inspected, zero were unreadable, the snapshot completed in 28 ms, no incident was present, and CLI status/history/pause/resume/doctor completed through IPC. The temporary daemon was stopped. This is read-only/local-control evidence only.

Open gates: real supported-family incident corpus, isolated real cleanup and chaos matrix, exit/wake/memory-pressure events, low-risk runtime-artifact cleanup, LaunchAgent install/rollback, daemon longevity/overhead, Intel/universal build, signing/notarization, distribution, dogfood, and public-alpha evidence.

Before public visibility: confirm license/rights, add bilingual reader documentation and a publication-grade architecture diagram, complete field evidence, replace observational support statements with proven ranges, and repeat tracked/staged privacy and provenance scans. Private continuity, origin conversation, diagnostics, and live captures stay outside Git.
