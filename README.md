# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

Unlinger is a local, zero-touch runtime-hygiene utility for abandoned browser-automation process trees. Its target product surface is absence: stale automation should disappear before CPU, memory, swap, sockets, and old headless browser clusters become the user's problem.

## Current state

The repository now contains a macOS source candidate spanning observation, deterministic cleanup, persistence, IPC, daemon, CLI, managed-service, and a first native SwiftUI frontend:

- native current-user process snapshots through `libproc` and `sysctl`;
- PID plus process-birth and executable-file identity;
- one shared deterministic sessionizer parameterized by embedded schema-v2 packs for agent-browser, Playwright, and Puppeteer;
- an exact automatic-eligibility point for Chrome for Testing `151.0.7922.34`, while unknown/mixed versions and every controller-bearing session fail closed as `PROTECTED`;
- hard protection gates, a non-evidentiary 60-second minimum age, two-observation stability, and a durable 90-second abandonment grace;
- frozen cleanup plans with durable PREPARED actions and fresh whole-incident revalidation before each exact signal stage;
- controller/root TERM, member TERM, exact-survivor KILL, post-action scans, bounded 15/60-second revival checks, and restart-safe delivery-unknown retry lockout;
- DAP-only runtime-artifact cleanup with a targeted Darwin pathname-reference query, a complete current-user argv pass, exact file/parent identity, an exclusive quarantine step, and a durable action journal; profiles and runtime directories are never deleted;
- independent process/artifact/overall cleanup outcomes, so a proved-gone tree remains visible as reclaimed when an explicitly refused artifact is safely retained; delivery uncertainty and unproved post-side-effect failures still fail closed;
- redacted SQLite v5 timelines, cooling/protection/retry/lifecycle state, terminal receipts stamped at completion, and 14-day/10,000-event retention;
- a 0600 local newline-delimited JSON socket with a frontend-only public schema v2 and a schema-v1 CLI/service compatibility lane; v2 omits process/service identities, exposes explicit capabilities and a bounded read-only current-incident roster, and cannot encode lifecycle commands; clients use one 15-second attempt, each connection retains a 3-second I/O bound, and a bounded eight-worker server prevents one slow read from blocking later control traffic;
- native process-exit, wake, and memory-pressure scheduling hints with a periodic fallback; every trigger still begins with a fresh snapshot and pressure never lowers a gate;
- a report-only daemon default and generation/instance/epoch-bound signal authorization;
- a transactional per-user LaunchAgent lifecycle with sealed immutable generations, exact launchd/IPC/binary checks, SQLite backup, report-only rollback, and same-generation fresh-epoch re-arm only after a signal-free first scan;
- graceful SIGTERM handling that terminates the current cycle safely, preserves same-generation desired intent for launchd restart, and removes the exact owned socket; explicit service drain clears that intent.
- a SwiftPM native menu-bar client under `apps/UnlingerApp`, with bilingual copy, canonical-fixture previews/tests, capability-gated ordinary actions, delivery-uncertain readback without automatic resend, and an ad-hoc-signed private `.app` bundler.

This is **a private source candidate with generation 9 installed report-only; the native frontend is source/isolated-daemon verified but not installed or integrated with generation 9; nothing is a product release**. One owner-approved production-timing installed-generation harness has now passed end to end on the exact admitted Chrome-for-Testing point: one eight-member frozen tree produced nine journaled exact signal actions, zero survivors, both revival checks, an exact `DevToolsActivePort` removal, and roughly 90 MiB reclaimed. The same generation then restarted into a fresh daemon instance and enforcement epoch without duplicating the cleanup journal, preserved every pre-existing ordinary-Chrome root identity, and finished stably unarmed/report-only. This proves one controlled process/artifact/restart transaction on one host, not ambient or broad safety. Two artifact residuals remain known: a daemon crash after canonical-to-quarantine rename can strand the exact quarantined entry, and a same-UID swap remains possible between final pathname revalidation and `unlinkat`. Ambient operation has not encountered and reclaimed an ordinary real eligible incident, and no multi-day, public-alpha, universal-binary, signed, notarized, or distribution claim exists.

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
```

See [`apps/UnlingerApp/README.md`](apps/UnlingerApp/README.md) for the isolated source-daemon live smoke. The installed generation 9 is v1-only and is not a frontend integration target.

Build a private release candidate, install it in report-only mode, and inspect the exact launchd/IPC boundary:

```bash
cargo build --release --workspace
target/release/unlinger service install --mode report-only
target/release/unlinger service status
```

`service install` publishes both binaries as a sealed generation under the private Application Support tree, writes a generation-bound per-user LaunchAgent, bootstraps it, and returns only after launchd PID, IPC PID, generation, executable identity, desired/effective mode, private permissions, readiness, and a completed first reconciliation scan agree. `service set-mode` uses exact generation/instance lifecycle IPC; failed arming is recovered to a proven report-only floor. `service uninstall` unloads the agent and removes managed service definitions/generations while preserving local history and logs. Enforce mode is an explicit field/dogfood action, not part of ordinary source verification.

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

IPC requests are single-shot. The client does not automatically resend after a timeout: for a mutation, a missing response means delivery is uncertain and the caller must read back the relevant state before deciding what to do next. A named cleanup retry clears only that incident's durable block and cooling candidate; it never signals immediately and must pass a fresh cooling window and every ordinary gate.

Full command lines, executable paths, and profile paths exist only in transient classification memory. SQLite, IPC, CLI output, and diagnostic exports use redacted typed records that omit signal targets and session identifiers.

## Documentation

- [Product & Technical Specification 0.1](docs/SPEC.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Safety model](docs/SAFETY.md)
- [Signature packs](docs/SIGNATURES.md)
- [Privacy](docs/PRIVACY.md)
- [Local IPC contract](docs/IPC.md)
- [Native frontend](apps/UnlingerApp/README.md)
- [Frontend contract and canonical fixtures](apps/UnlingerApp/Contract/README.md)
- [Field Lab](docs/FIELDLAB.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Current state](docs/current-state.md)

The repository is private. No public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a public-safety/privacy scan.
