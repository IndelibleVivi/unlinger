# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

Unlinger is a local, zero-touch runtime-hygiene utility for abandoned browser-automation process trees. Its target product surface is absence: stale automation should disappear before CPU, memory, swap, sockets, and old headless browser clusters become the user's problem.

## Current state

The repository contains a macOS pre-v0.1 source candidate spanning observation, deterministic cleanup, persistence, IPC, daemon, CLI, managed-service, and a native SwiftUI menu-bar App:

- native current-user process snapshots through `libproc` and `sysctl`;
- PID plus process-birth and executable-file identity;
- one shared deterministic sessionizer parameterized by embedded schema-v2, source `0.4.0` agent-browser/Puppeteer packs and a `0.5.0` Playwright pack;
- an exact source automatic-eligibility allowlist for Chrome for Testing `151.0.7922.34` and `152.0.7977.42`, while every other, unknown, or mixed version and every unregistered or unverified controller-bearing session fails closed as `PROTECTED`;
- [`task run` and `task status`](docs/TASKS.md) for command-owned Playwright CLI sessions: durable registration before execution, exact command-owner lifetime, live-client protection, multiple workspaces and existing cleanup/impact authority;
- hard protection gates, a non-evidentiary 60-second minimum age, two-observation stability, and a durable 90-second abandonment grace;
- frozen cleanup plans with durable PREPARED actions and fresh whole-incident revalidation before each exact signal stage;
- controller/root TERM, member TERM, exact-survivor KILL, post-action scans, bounded 15/60-second revival checks, and restart-safe delivery-unknown retry lockout;
- an implemented but currently dormant DAP-only runtime-artifact path with targeted Darwin pathname-reference proof, complete current-user argv proof, exact file/parent identity, exclusive quarantine, and a durable action journal; all source packs disable artifact admission, so the process-only candidate cannot schedule or journal an artifact action;
- independent process/artifact/overall cleanup outcomes, so a proved-gone tree remains visible as reclaimed when an explicitly refused artifact is safely retained; delivery uncertainty and unproved post-side-effect failures still fail closed;
- redacted SQLite v8 storage with coalesced observation spans, independent cleanup-impact rows and lifetime aggregates, stable public event tokens, namespace-aware ordinary-mutation receipts, cooling/protection/retry/lifecycle state, terminal receipts stamped at completion, observation-bounded retention, and at least 14 days of cleanup detail;
- a 0600 local newline-delimited JSON socket with current frontend schema v5, transitional schema v4, legacy-compatible schema v3, and a schema-v1 CLI/service lane; v5 extends the atomic daemon-owned browser overview with durable impact and typed storage-residue observation, while v4 retains the earlier overview shape, v3 retains its existing commands, and no frontend schema can encode lifecycle commands; historical v2 is rejected rather than silently downgraded;
- an observe-only scanner for the exact current-user Chrome `code_sign_clone` temporary-residue family: it recognizes upstream `.app.bundle` and feature-disabled `.app` clone directories, traverses by directory descriptor without following framework symlinks, and records only count, regular-file logical size, shape/reference status, and reason IDs, never paths or file contents; reference proof remains incomplete and deletion is unavailable;
- native process-exit, wake, and memory-pressure scheduling hints with a periodic fallback; every trigger still begins with a fresh snapshot and pressure never lowers a gate;
- a report-only daemon default and generation/instance/epoch-bound signal authorization;
- a transactional per-user LaunchAgent lifecycle with sealed immutable generations, exact launchd/IPC/binary checks, an acceptance-scoped SQLite rollback lease, crash-replayable candidate rollback, schema-preserving cross-generation containment, explicit candidate accept/rollback, and same-generation fresh-epoch re-arm only after a signal-free first scan; machine-readable service output uses a public projection rather than exposing PIDs, instance IDs, local paths or raw errors;
- graceful SIGTERM handling that terminates the current cycle safely, preserves same-generation desired intent for launchd restart, and removes the exact owned socket; explicit service drain clears that intent.
- a SwiftPM native menu-bar and Dock client under `apps/UnlingerApp`, with a strict schema-v5 browser-first overview that consumes one atomic daemon-owned phase/session/compatibility/coverage/settlement/impact/residue snapshot; its history index now shows completed cleanup outcomes rather than periodic observation noise, while detail renders server-owned observation spans. The Swift presentation layer publishes stable mapped values once per state transition, and a direct AppKit `@main` gives the popover and reusable ordinary window independent navigation storage with one-way route handoff. Its square status item uses the system `circle.dashed` symbol and a stable AppKit autosave identity so macOS and compatible menu organizers address the same item across restarts. The client also retains reliable Dock/window fallback, crash-durable no-resend mutation handling, capability-gated ordinary actions, bounded local notifications, menu-client-only launch at login, bilingual VoiceOver copy, a fixture-only Accessibility/RSS regression gate, and a private ad-hoc-signed `.app` bundler that rejects removable-volume resource and loader paths.

The installed private deployment is accepted generation 19 from source head `f7f1857`, serving frontend schemas v5/v4/v3 over SQLite v7 and healthy `ReadyEnforce` under the original process-only `0.4.0` policy. The 2026-09-08 repair recovered generation 17 from a terminal SQLite I/O failure, corrected observation of real Chrome `.app.bundle` clones, passed exact-head CI, exercised a real candidate rollback, and installed the verified replacement. The existing schema-v5 App now displays the observed clone count and logical size; no clone was deleted and no cleanup eligibility was widened. The original I/O failure cause remains unknown. See [`docs/current-state.md`](docs/current-state.md) for current runtime and evidence details.

The installed App makes completed cleanup visible through independent impact authority, compresses repeated observations into spans, shows cleanup-only history, and reports the exact Chrome code-sign clone residue family as observe-only. A live UI check read the v5 home and cleanup-only empty-history routes without the prior repeated `PROTECTED` rows or 404 behavior. During the bounded deployment observation, the App remained alive for more than five minutes with sampled RSS no higher than 29,408 KiB. This supplements rather than replaces the earlier ten-minute/1,398-read memory acceptance of the repaired navigation architecture; neither result is multi-day App dogfood, packaged-notification proof, or universal external-menu-host/display evidence.

Historical field evidence remains narrower than the installed policy. The generation-15 managed full-timing run reclaimed one isolated Chrome-for-Testing `151.0.7922.34` tree: eight exact processes, nine ordered signal actions, zero survivors, two revival checks, same-generation fresh-epoch restart without journal duplication, and final harness containment. It admitted zero artifact candidates and wrote zero receipt/journal artifact actions. Generation 19 retains eligibility for exact controllerless CfT `151.0.7922.34` and `152.0.7977.42`, but controller-bearing sessions remain protected because task/controller lifetime authority is incomplete; generation 19 has no candidate-specific controlled cleanup or ambient eligible-cleanup result yet. Historical generation-9/13 DAP results do not authorize the active runtime to delete artifacts; both DAP P2s remain dormant. Multi-day dogfood, an ambient real eligible incident, broader version evidence, Intel/universal, signing/notarization/distribution and public alpha remain open.

## Run a browser task

```bash
unlinger task run -- ./browser-task.sh
unlinger task status <task-id> --json
```

The script must use the verified Playwright CLI and inherit its issued `PLAYWRIGHT_CLI_SESSION`. A released task still needs all cleanup gates; report-only mode only observes. See the [task guide](docs/TASKS.md) for exact versions, configuration, semantics and current host-integration limits. This source capability must be installed before the existing service can accept task commands.

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

The report-only smoke creates and owns one temporary database/socket/lock, verifies the v5 App and durable receipts across restart, and never touches the installed service. The Accessibility memory smoke separately launches an owned fixture-only App on a synthetic stress-history route, repeatedly requests its real macOS Accessibility tree, and enforces external RSS and retained-growth cutoffs. See [`apps/UnlingerApp/README.md`](apps/UnlingerApp/README.md).

Do not improvise an installed migration or bypass the service CLI. [`docs/INSTALLED_DOGFOOD.md`](docs/INSTALLED_DOGFOOD.md) records the completed report-only candidate/rollback lane; [`docs/current-state.md`](docs/current-state.md) owns current runtime truth. Generation 19 is accepted with no pending candidate rollback lease and is healthy `ReadyEnforce` after the 2026-09-08 repair deployment. Use the exact active-generation CLI for status and the transactional service CLI for authorized changes.

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

IPC requests are single-shot. The v5 App durably journals a namespace token and mutation UUID before any request byte, then reconciles uncertain delivery with read-only `mutation_status`; it never automatically resends. A named cleanup retry clears only that incident's durable block and cooling candidate; it never signals immediately and must pass a fresh cooling window and every ordinary gate.

Full command lines, executable paths, and profile paths exist only in transient classification memory. SQLite, IPC, CLI output, and diagnostic exports use redacted typed records that omit signal targets and private browser/profile identifiers. Task status exposes only its newly issued opaque selector and phase.

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

The repository is private and no public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, confirmation that the remaining tracked app-icon source and compiled icon may be redistributed, bilingual reader documentation, and a publication-grade architecture diagram. Current field evidence and the all-scope public-candidate scan are necessary inputs, not authority to publish.
