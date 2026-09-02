# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

Unlinger is a local, zero-touch runtime-hygiene utility for abandoned browser-automation process trees. Its target product surface is absence: stale automation should disappear before CPU, memory, swap, sockets, and old headless browser clusters become the user's problem.

## Current state

The repository contains a macOS pre-v0.1 source candidate spanning observation, deterministic cleanup, persistence, IPC, daemon, CLI, managed-service, and a native SwiftUI menu-bar App:

- native current-user process snapshots through `libproc` and `sysctl`;
- PID plus process-birth and executable-file identity;
- one shared deterministic sessionizer parameterized by embedded schema-v2, policy-version `0.3.0` packs for agent-browser, Playwright, and Puppeteer;
- an exact automatic-eligibility point for Chrome for Testing `151.0.7922.34`, while unknown/mixed versions and every controller-bearing session fail closed as `PROTECTED`;
- hard protection gates, a non-evidentiary 60-second minimum age, two-observation stability, and a durable 90-second abandonment grace;
- frozen cleanup plans with durable PREPARED actions and fresh whole-incident revalidation before each exact signal stage;
- controller/root TERM, member TERM, exact-survivor KILL, post-action scans, bounded 15/60-second revival checks, and restart-safe delivery-unknown retry lockout;
- an implemented but currently dormant DAP-only runtime-artifact path with targeted Darwin pathname-reference proof, complete current-user argv proof, exact file/parent identity, exclusive quarantine, and a durable action journal; all current `0.3.0` packs disable artifact admission, so the process-only candidate cannot schedule or journal an artifact action;
- independent process/artifact/overall cleanup outcomes, so a proved-gone tree remains visible as reclaimed when an explicitly refused artifact is safely retained; delivery uncertainty and unproved post-side-effect failures still fail closed;
- redacted SQLite v6 timelines, durable cleanup-policy revision, stable public event tokens, namespace-aware ordinary-mutation receipts, cooling/protection/retry/lifecycle state, terminal receipts stamped at completion, and bounded retention;
- a 0600 local newline-delimited JSON socket with frontend schema v4, a transitional schema-v3 compatibility endpoint, and a schema-v1 CLI/service lane; v4 adds one atomic daemon-owned browser overview and rule-generated compatibility catalog, while both frontend schemas omit process/service identities, retain durable mutation reconciliation, and cannot encode lifecycle commands; historical v2 is rejected rather than silently downgraded;
- native process-exit, wake, and memory-pressure scheduling hints with a periodic fallback; every trigger still begins with a fresh snapshot and pressure never lowers a gate;
- a report-only daemon default and generation/instance/epoch-bound signal authorization;
- a transactional per-user LaunchAgent lifecycle with sealed immutable generations, exact launchd/IPC/binary checks, an acceptance-scoped SQLite rollback lease, crash-replayable candidate rollback, schema-preserving cross-generation containment, explicit candidate accept/rollback, and same-generation fresh-epoch re-arm only after a signal-free first scan; machine-readable service output uses a public projection rather than exposing PIDs, instance IDs, local paths or raw errors;
- graceful SIGTERM handling that terminates the current cycle safely, preserves same-generation desired intent for launchd restart, and removes the exact owned socket; explicit service drain clears that intent.
- a SwiftPM native menu-bar and Dock client under `apps/UnlingerApp`, with a strict schema-v4 browser-first overview that consumes one atomic daemon-owned phase/session/compatibility/coverage/settlement snapshot; the Swift mapper now owns localization and presentation only, while a direct AppKit `@main` owns the status item/popover and reusable ordinary window around that same SwiftUI root, with reliable Dock/window fallback, crash-durable no-resend mutation handling, capability-gated ordinary actions, bounded local notifications, menu-client-only launch at login, bilingual VoiceOver copy, and a private ad-hoc-signed `.app` bundler that rejects removable-volume resource and loader paths.

The current allowed claim is **private enforcement candidate for the exact admitted point**. BGX-2 generation 13 and the schema-v4 ad-hoc-signed native App are installed for private dogfood. Generation 13 was accepted and later proved exact `ReadyEnforce` plus real cleanup in a controlled run, but is now deliberately contained at healthy `ReadyReportOnly`. The current locally source-complete candidate is process-only: its `0.3.0` packs preserve exact TERM/KILL authorization while setting every runtime-artifact policy to false. That source candidate is not installed or active until exact-head CI and transactional rollback gates pass.

Field evidence remains narrower than the source surface. The historical generation-13 managed full-timing run reclaimed one isolated Chrome-for-Testing `151.0.7922.34` tree: eight exact processes, nine ordered signal actions, zero survivors, two revival checks, one exact `DevToolsActivePort` removal, same-generation fresh-epoch restart without journal duplication, and final harness containment. The later brief re-arm was also contained. The live v4 overview protects three observed CfT `152.0.7977.42` sessions as unsupported. The historical DAP result does not authorize the current source to delete artifacts: a crash after canonical-to-quarantine rename may strand the quarantine entry, and the final revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU. The next installed full-timing acceptance must prove the process cleanup again and require zero artifact candidates, receipt actions, and durable artifact rows. Multi-day dogfood, an ambient real eligible incident, broader version evidence, Intel/universal, signing/notarization/distribution and public alpha remain open.

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

The smoke script creates and owns one temporary database/socket/lock, keeps the daemon report-only, verifies the v4 App including its atomic browser overview plus the retained frontend commands and durable receipts across restart, checks private modes and no IP listener, and removes only that exact temporary root. It never touches the installed service. See [`apps/UnlingerApp/README.md`](apps/UnlingerApp/README.md).

Do not improvise an installed migration or bypass the service CLI. [`docs/INSTALLED_DOGFOOD.md`](docs/INSTALLED_DOGFOOD.md) records the completed report-only candidate/rollback lane; [`docs/current-state.md`](docs/current-state.md) owns current runtime truth. Generation 13 is accepted and temporarily contained report-only, so there is no pending candidate rollback lease. Use the exact active-generation CLI for status and require a new owner decision before install, uninstall or another mode change.

For source-only development, run the daemon in its safe default mode and use the local CLI from another terminal:

```bash
cargo run -p unlinger-daemon -- --report-only

cargo run -p unlinger-cli -- status
cargo run -p unlinger-cli -- browser status
cargo run -p unlinger-cli -- browser status --json
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

IPC requests are single-shot. The v4 App durably journals a namespace token and mutation UUID before any request byte, then reconciles uncertain delivery with read-only `mutation_status`; it never automatically resends. A named cleanup retry clears only that incident's durable block and cooling candidate; it never signals immediately and must pass a fresh cooling window and every ordinary gate.

Full command lines, executable paths, and profile paths exist only in transient classification memory. SQLite, IPC, CLI output, and diagnostic exports use redacted typed records that omit signal targets and session identifiers.

## Documentation

- [Product & Technical Specification 0.1](docs/SPEC.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Safety model](docs/SAFETY.md)
- [Signature packs](docs/SIGNATURES.md)
- [Privacy](docs/PRIVACY.md)
- [Local IPC contract](docs/IPC.md)
- [Pre-v0.1 acceptance levels](docs/PRE_V0_1_ACCEPTANCE.md)
- [Installed report-only dogfood runbook (completed historical lane)](docs/INSTALLED_DOGFOOD.md)
- [Support truth](docs/SUPPORT.md)
- [Machine-readable support matrix](docs/support-matrix.v1.json)
- [Native frontend](apps/UnlingerApp/README.md)
- [Frontend contract and canonical fixtures](apps/UnlingerApp/Contract/README.md)
- [Field Lab](docs/FIELDLAB.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Current state](docs/current-state.md)

The repository is private. No public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a public-safety/privacy scan.
