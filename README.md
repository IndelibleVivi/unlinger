# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

Unlinger is a local, zero-touch runtime-hygiene utility for abandoned browser-automation process trees. Its target product surface is absence: stale automation should disappear before CPU, memory, swap, sockets, and old headless browser clusters become the user's problem.

## Current state

The repository contains a macOS pre-v0.1 source candidate spanning observation, deterministic cleanup, persistence, IPC, daemon, CLI, managed-service, and a native SwiftUI menu-bar App:

- native current-user process snapshots through `libproc` and `sysctl`;
- PID plus process-birth and executable-file identity;
- one shared deterministic sessionizer parameterized by embedded schema-v2 packs for agent-browser, Playwright, and Puppeteer;
- an exact automatic-eligibility point for Chrome for Testing `151.0.7922.34`, while unknown/mixed versions and every controller-bearing session fail closed as `PROTECTED`;
- hard protection gates, a non-evidentiary 60-second minimum age, two-observation stability, and a durable 90-second abandonment grace;
- frozen cleanup plans with durable PREPARED actions and fresh whole-incident revalidation before each exact signal stage;
- controller/root TERM, member TERM, exact-survivor KILL, post-action scans, bounded 15/60-second revival checks, and restart-safe delivery-unknown retry lockout;
- DAP-only runtime-artifact cleanup with a targeted Darwin pathname-reference query, a complete current-user argv pass, exact file/parent identity, an exclusive quarantine step, and a durable action journal; profiles and runtime directories are never deleted;
- independent process/artifact/overall cleanup outcomes, so a proved-gone tree remains visible as reclaimed when an explicitly refused artifact is safely retained; delivery uncertainty and unproved post-side-effect failures still fail closed;
- redacted SQLite v6 timelines, durable cleanup-policy revision, stable public event tokens, namespace-aware ordinary-mutation receipts, cooling/protection/retry/lifecycle state, terminal receipts stamped at completion, and bounded retention;
- a 0600 local newline-delimited JSON socket with frontend schema v3 and a schema-v1 CLI/service compatibility lane; v3 omits process/service identities, exposes exact readiness, roster freshness, typed outcomes, capabilities and durable mutation reconciliation, and cannot encode lifecycle commands; historical v2 is rejected rather than silently downgraded;
- native process-exit, wake, and memory-pressure scheduling hints with a periodic fallback; every trigger still begins with a fresh snapshot and pressure never lowers a gate;
- a report-only daemon default and generation/instance/epoch-bound signal authorization;
- a transactional per-user LaunchAgent lifecycle with sealed immutable generations, exact launchd/IPC/binary checks, an acceptance-scoped SQLite rollback lease, crash-replayable candidate rollback, schema-preserving cross-generation containment, explicit candidate accept/rollback, and same-generation fresh-epoch re-arm only after a signal-free first scan; machine-readable service output uses a public projection rather than exposing PIDs, instance IDs, local paths or raw errors;
- graceful SIGTERM handling that terminates the current cycle safely, preserves same-generation desired intent for launchd restart, and removes the exact owned socket; explicit service drain clears that intent.
- a SwiftPM native menu-bar and Dock client under `apps/UnlingerApp`, with a direct AppKit `@main`, process-lifetime delegate ownership, an AppKit-owned status item/popover and reusable ordinary window hosting the SwiftUI surface, a reliable Dock/window fallback when an external menu host cannot resolve the item, explicit back/window routing, strict v3 DTOs, bilingual copy, canonical-fixture previews/tests, crash-durable pre-send mutation journal, stale-safe refresh/detail state, capability-gated ordinary actions, bounded deduplicated local notifications, menu-client-only launch at login, and a private ad-hoc-signed `.app` bundler that rejects removable-volume resource and loader paths.

The current allowed claim is **pre-v0.1 installed report-only candidate**. Exact-head generation 12 and the ad-hoc-signed native App are installed for private dogfood; the daemon is healthy, schema-v3/v6 capable, report-only and unarmed. Its acceptance lease still retains generation 9's manifest/plist/v5 database. A real rollback restored generation 9, whose exact old CLI/daemon reopened v5 and returned healthy ReadyReportOnly, before the candidate was reinstalled. The candidate is deliberately not accepted yet, so rollback remains available throughout initial dogfood.

Historical field evidence remains narrower than the source surface: one owner-approved production-timing generation-9 harness passed on the exact admitted Chrome-for-Testing point, with one eight-member tree, nine exact signal actions, zero survivors, both revival checks, one exact `DevToolsActivePort` removal, restart without journal duplication, ordinary-Chrome preservation, and final report-only containment. This is one controlled point—not ambient or broad support. A crash after canonical-to-quarantine rename may still strand the exact quarantine entry, and the final revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU. Multi-day dogfood, an ambient real eligible incident, narrow enforce acceptance for this candidate, Intel/universal, signing/notarization/distribution and public alpha remain open.

## Build and inspect

Requirements: macOS 14 or later and Rust 1.98.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo run -p unlinger-cli -- doctor --source-only
cargo run -p unlinger-cli -- scan --dry-run
cargo run -p unlinger-cli -- scan --dry-run --json
```

The default `doctor` also requires a reachable, healthy, ready daemon. Use `--source-only` before installation or when intentionally validating source without claiming runtime readiness.

Build and test the fixture-first native frontend without touching the installed service:

```bash
cd apps/UnlingerApp
swift test
scripts/bundle.sh
scripts/pre-v0.1-smoke.sh
```

The smoke script creates and owns one temporary database/socket/lock, keeps the daemon report-only, verifies v3 reads and durable receipts across restart, checks private modes and no IP listener, and removes only that exact temporary root. It never touches the installed service. See [`apps/UnlingerApp/README.md`](apps/UnlingerApp/README.md).

Do not improvise an installed migration or bypass the service CLI. [`docs/INSTALLED_DOGFOOD.md`](docs/INSTALLED_DOGFOOD.md) remains the operator contract. Generation 12 is intentionally parked at `candidate_ready_report_only`; use its exact CLI for status or rollback, and do not run `accept-candidate`, install/uninstall or change mode until the owner accepts the observed dogfood result.

For source-only development, run the daemon in its safe default mode and use the local CLI from another terminal:

```bash
cargo run -p unlinger-daemon -- --report-only

cargo run -p unlinger-cli -- status
cargo run -p unlinger-cli -- history
cargo run -p unlinger-cli -- explain <incident-id>
cargo run -p unlinger-cli -- pause 2h
cargo run -p unlinger-cli -- resume
cargo run -p unlinger-cli -- retry <incident-id>
cargo run -p unlinger-cli -- protect <incident-id>
cargo run -p unlinger-cli -- unprotect <incident-id>
cargo run -p unlinger-cli -- export-diagnostics <incident-id>
```

`scan` always requires `--dry-run` and never sends signals. Direct `unlingerd` invocation defaults to report-only. Managed LaunchAgents receive only `--managed --activation-generation`; the desired/effective mode and signal authority live in exact durable lifecycle state, not in a plist `--enforce` flag.

IPC requests are single-shot. The v3 App durably journals a namespace token and mutation UUID before any request byte, then reconciles uncertain delivery with read-only `mutation_status`; it never automatically resends. A named cleanup retry clears only that incident's durable block and cooling candidate; it never signals immediately and must pass a fresh cooling window and every ordinary gate.

Full command lines, executable paths, and profile paths exist only in transient classification memory. SQLite, IPC, CLI output, and diagnostic exports use redacted typed records that omit signal targets and session identifiers.

## Documentation

- [Product & Technical Specification 0.1](docs/SPEC.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Safety model](docs/SAFETY.md)
- [Signature packs](docs/SIGNATURES.md)
- [Privacy](docs/PRIVACY.md)
- [Local IPC contract](docs/IPC.md)
- [Pre-v0.1 acceptance levels](docs/PRE_V0_1_ACCEPTANCE.md)
- [Installed report-only dogfood runbook](docs/INSTALLED_DOGFOOD.md)
- [Support truth](docs/SUPPORT.md)
- [Machine-readable support matrix](docs/support-matrix.v1.json)
- [Native frontend](apps/UnlingerApp/README.md)
- [Frontend contract and canonical fixtures](apps/UnlingerApp/Contract/README.md)
- [Field Lab](docs/FIELDLAB.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Current state](docs/current-state.md)

The repository is private. No public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a public-safety/privacy scan.
