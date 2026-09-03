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
- a SwiftPM native menu-bar and Dock client under `apps/UnlingerApp`, with a strict schema-v4 browser-first overview that consumes one atomic daemon-owned phase/session/compatibility/coverage/settlement snapshot; the Swift presentation layer owns localization plus incident-centric history and publishes stable mapped values once per state transition, while a direct AppKit `@main` gives the popover and reusable ordinary window independent navigation storage with one-way route handoff. The client also retains reliable Dock/window fallback, crash-durable no-resend mutation handling, capability-gated ordinary actions, bounded local notifications, menu-client-only launch at login, bilingual VoiceOver copy, a fixture-only Accessibility/RSS regression gate, and a private ad-hoc-signed `.app` bundler that rejects removable-volume resource and loader paths.

The current allowed daemon claim is **private enforcement candidate for the exact admitted point**. Accepted generation 15 remains installed and actively enforces the process-only `0.3.0` policy, which preserves exact TERM/KILL authorization while disabling every runtime-artifact policy. The installed schema-v4 ad-hoc-signed native App is intentionally stopped after a second severe memory runaway during browser-history/Accessibility interaction. Source now removes the shared-navigation and repeated-history-projection feedback surfaces and has a fixture-only hard-cutoff regression gate, but that does not silently accept or relaunch the older installed bundle. The daemon installation followed exact-head CI, candidate-A restart and real rollback to generation 13, candidate-B reinstall/restart/accept, a full-timing managed field pass, final report-only containment, explicit arm, and a stable later sweep.

Field evidence remains narrower than the source surface. The generation-15 managed full-timing run reclaimed one isolated Chrome-for-Testing `151.0.7922.34` tree: eight exact processes, nine ordered signal actions, zero survivors, two revival checks, same-generation fresh-epoch restart without journal duplication, and final harness containment. It admitted zero artifact candidates, wrote zero receipt/journal artifact actions, left `DevToolsActivePort` present, and projected `artifact_outcome: not_applicable`. After activation, a later complete sweep remained healthy and protected all four observed CfT `152.0.7977.42` sessions as unsupported. Historical generation-9/13 DAP results do not authorize the current runtime to delete artifacts; both DAP P2s remain dormant. Multi-day dogfood, an ambient real eligible incident, broader version evidence, Intel/universal, signing/notarization/distribution and public alpha remain open.

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
scripts/accessibility-memory-smoke.sh
```

The report-only smoke creates and owns one temporary database/socket/lock, verifies the v4 App and durable receipts across restart, and never touches the installed service. The Accessibility memory smoke separately launches an owned fixture-only App on a synthetic stress-history route, repeatedly requests its real macOS Accessibility tree, and enforces external RSS and retained-growth cutoffs. See [`apps/UnlingerApp/README.md`](apps/UnlingerApp/README.md).

Do not improvise an installed migration or bypass the service CLI. [`docs/INSTALLED_DOGFOOD.md`](docs/INSTALLED_DOGFOOD.md) records the completed report-only candidate/rollback lane; [`docs/current-state.md`](docs/current-state.md) owns current runtime truth. Generation 15 is accepted and actively enforcing the process-only policy, with no pending candidate rollback lease. Use the exact active-generation CLI for status and require a new owner decision before install, uninstall or another mode change.

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

The repository is private and no public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, confirmation that the tracked binary icons may be redistributed, bilingual reader documentation, and a publication-grade architecture diagram. Current field evidence and the all-scope public-candidate scan are necessary inputs, not authority to publish.
