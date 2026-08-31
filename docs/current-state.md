# Current state

**Updated:** 2026-08-31
**Programme:** Unlinger 0.1
**Source:** private backend source candidate; not released
**Remote:** private origin configured
**Installed runtime:** generation 9, loaded and healthy at a stable report-only floor
**Activation:** unarmed after one complete managed full-timing acceptance run; ambient enforcement remains off

## What is true now

The current source spans native macOS observation, deterministic schema-v2 sessionization for the three scoped Chromium families, exact-version protection, durable cooling, frozen cleanup plans, PREPARED-before-delivery process/artifact journals, exact TERM/KILL stages, bounded revival checks, redacted SQLite v5 history, native scheduling hints with periodic fallback, local IPC/CLI, and transactional immutable service generations.

The macOS live-process boundary now treats only a PID confirmed `SZOMB` through `KERN_PROC_PID` as gone; every other incomplete read remains fail closed. DAP cleanup no longer walks every descriptor of every same-UID process. It uses Darwin's targeted `proc_listpidspath` query for the exact canonical or quarantine pathname, a complete current-user argv pass, and frozen parent/file identity checks before and after the query.

The managed lifecycle is generation/instance/epoch bound. First scan is always report-only. `Failed` is terminal for one daemon instance and cannot be rehabilitated by a later successful scan or `Disarm`; exact drain/bootout/fresh restart is required. Ready report-only installation acceptance additionally requires no scan or cleanup in progress. Arm checks both durable identity and the current volatile lifecycle phase. Same-generation carried enforce intent may resume only after fresh report-only recovery/first scan, a new epoch, fresh cooling, and absence of open-attempt or delivery-unknown blockers.

Ordinary and service IPC clients make one 15-second request attempt; accepted server connections retain a 3-second I/O bound and are served by a bounded eight-worker set, so one slow history/read peer no longer blocks later status or lifecycle traffic. A timed-out mutation is uncertain delivery and is never automatically resent. Pause/resume, exact protection changes, named retry, and lifecycle changes advance the cleanup-policy revision. A named retry clears only its incident's block and cooling candidate and must pass fresh cooling before any later signal.

## Verification truth

Exact-head verification passes: workspace fmt, strict workspace clippy, 237 ordinary tests with the two owner-only live harnesses ignored, release workspace build, a source-only doctor over 501/501 current-user processes with zero unreadable argv/descriptor coverage, and a dry-run scan whose four visible automation-shaped incidents were all `PROTECTED`. Transactional generation-9 installation and post-field installed runtime readbacks also pass.

Synthetic verification includes exact owned-child signal/process-exit tests, an owned-zombie lookup/capture fixture, targeted-path artifact reference and quarantine tests, durable journal/recovery tests, uncertain-delivery and named-retry policy-revision races, terminal Failed lifecycle tests, stable quiescent service predicates, and transactional service recovery tests. The two live CfT harnesses remain ignored by ordinary workspace tests and require their explicit owner acknowledgement/boundary.

## Field truth

Historical isolated Chrome-for-Testing runs at the single admitted version proved full and fast process mechanics while preserving simultaneous ordinary Chrome. Earlier short LaunchAgent operation proved report-only sweeps, exact service identity, private permissions, local IPC, protected-session coexistence, and historical mode-transition mechanics on one host. Those are point/short-duration results, not sustained acceptance.

A generation-5 managed full-timing attempt completed the exact process programme for one eight-member Chrome-for-Testing tree: journaled TERM/KILL delivery, zero survivors, both revival checks, and roughly 60 MiB reclaimed. The DAP action then returned `Unsafe` under the superseded global descriptor walk, so no artifact was removed and the harness as a whole did not pass. Its terminal failure also exposed a lifecycle projection race. The targeted-path artifact design, terminal Failed behavior, volatile Arm gate, stable quiescent predicate, and exact Failed drain/restart path were implemented afterward.

Generation 7 then exposed a valid empty-argv case in a long-lived LaunchServices process. The `KERN_PROCARGS2` parser had incorrectly skipped consecutive NULs after argv parsing began, so the complete reference scan failed closed. One canonical parser now preserves empty arguments, stops at exact `argc`, and excludes the environment tail. A read-only retained-profile scan subsequently completed without incomplete processes.

The first generation-8 run completed the cleanup itself but rejected the postflight because an unrelated ordinary Chrome root was opened during the test. The proof now preserves every pre-existing ordinary-Chrome root by exact identity, permits new roots and ordinary child churn, and independently requires every receipt/journal target to belong to the pre-arm frozen field tree. A later generation-8 rerun stopped on a read-only `history(1000)` IPC timeout. Exact receipt polling now uses incident-scoped `Explain` on a separate worker, so native absence sampling remains at its own cadence; only read-only transient outcomes retry.

Generation 9 then passed the complete owner-approved production-timing managed harness in 341.8 seconds. One unique eight-member CfT tree produced nine ordered exact signal actions, zero survivors, both revival checks, an exact DAP removal, and roughly 90 MiB reclaimed. Durable attempt/action/artifact rows matched the terminal receipt exactly. A same-generation restart exposed the required transition, produced a new daemon instance and enforcement epoch, and left the completed journal unchanged through the post-restart observation. Every pre-existing ordinary-Chrome root retained exact identity even though a new unrelated root appeared during the run. Final independent readback proved generation/binary/launchd/IPC agreement, healthy ReadyReportOnly, requested/effective report-only, no enforcement epoch, no scan or cleanup, and no cleanup block.

This is one controlled installed process/artifact/restart acceptance point, not ambient or broad family acceptance. It does not close two known artifact P2s: daemon crash after canonical-to-quarantine rename can strand the exact random entry, and the final pathname `fstatat`-to-`unlinkat` interval retains a same-UID swap TOCTOU.

## Open gates

- retain the generation-9 report-only floor while collecting sustained evidence;
- resolve or explicitly accept the two artifact P2s before any broad artifact-completeness claim;
- collect broader supported-family/version, controller-bearing, chaos, sleep/wake, sustained pressure, restart-during-cooling, longevity/overhead, and multi-day zero-false-positive evidence;
- build/test Intel or universal artifacts, then separately complete signing, notarization, packaging, versioned rollback/update, release, and public-alpha acceptance;
- before public visibility, make the owner-approved license/rights decision, add bilingual reader documentation and a publication-grade architecture diagram, and repeat tracked/staged privacy and provenance scans.

Private continuity, origin conversation, retained profiles, diagnostics, and raw field captures remain outside Git.
