# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

Unlinger is a local, zero-touch runtime-hygiene utility for abandoned browser-automation process trees. Its target product surface is absence: stale automation should disappear before CPU, memory, swap, sockets, and old headless browser clusters become the user's problem.

## Current state

The repository now contains a macOS source candidate spanning the observation, deterministic cleanup, persistence, IPC, daemon, and CLI paths:

- native current-user process snapshots through `libproc` and `sysctl`;
- PID plus process-birth and executable-file identity;
- process-graph reconstruction and framework-specific incident grouping;
- embedded, versioned signature packs for agent-browser, Playwright, and Puppeteer;
- hard protection gates, two-observation stability, and a durable 90-second abandonment grace;
- frozen cleanup plans with fresh whole-incident revalidation before each exact signal stage;
- controller/root TERM, member TERM, exact-survivor KILL, post-action scans, and bounded 15/60-second revival checks;
- redacted SQLite timelines with terminal cleanup events stamped at completion and 14-day/10,000-event retention;
- a 0600 local Unix-domain socket for status, history, explain, pause/resume, and diagnostic export;
- a periodic daemon whose source default is report-only;
- a transactional per-user LaunchAgent lifecycle with same-directory per-file promotion, launchd/IPC PID matching, first-scan health verification, explicit mode reload, and activation-failure rollback;
- graceful SIGTERM handling that finishes an in-flight cleanup receipt, suppresses new cleanup after shutdown begins, and removes the exact owned socket.

This is **an installed private dogfood candidate, not a product release**. An owner-approved per-user LaunchAgent is loaded in enforce mode on the first dogfood Mac after persistent report-only verification and an enforce → report-only → enforce rollback exercise. Owner-approved isolated Chrome-for-Testing runs have also exercised both the full production-timing path and the repeatable scoped Field Lab harness. Ambient operation has not yet encountered and reclaimed a real supported incident, and no multi-day, public-alpha, universal-binary, signed, notarized, or distribution claim exists. Wake/memory-pressure/exit events, low-risk runtime-artifact cleanup, the broader chaos/family matrix, and sustained dogfood remain open.

## Build and inspect

Requirements: macOS 14 or later and Rust 1.98.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo run -p unlinger-cli -- doctor
cargo run -p unlinger-cli -- scan --dry-run
cargo run -p unlinger-cli -- scan --dry-run --json
```

Build a private release candidate, install it in report-only mode, and inspect the exact launchd/IPC boundary:

```bash
cargo build --release --workspace
target/release/unlinger service install --mode report-only
target/release/unlinger service status

target/release/unlinger service set-mode enforce
target/release/unlinger service set-mode report-only
```

`service install` copies both binaries into `~/Library/Application Support/Unlinger/bin/`, writes a private per-user LaunchAgent, bootstraps it, and returns only after launchd PID, IPC PID, declared mode, file permissions, and a completed first reconciliation scan agree. `service set-mode` reloads transactionally and restores the prior plist/service if activation fails. `service uninstall` unloads the agent and removes the managed plist/binaries while preserving local history and logs.

For source-only development, run the daemon in its safe default mode and use the local CLI from another terminal:

```bash
cargo run -p unlinger-daemon -- --report-only

cargo run -p unlinger-cli -- status
cargo run -p unlinger-cli -- history
cargo run -p unlinger-cli -- explain <incident-id>
cargo run -p unlinger-cli -- pause 2h
cargo run -p unlinger-cli -- resume
cargo run -p unlinger-cli -- export-diagnostics <incident-id>
```

`scan` always requires `--dry-run` and never sends signals. `unlingerd` still defaults to report-only when invoked directly. The currently installed dogfood LaunchAgent passes `--enforce` explicitly; this proves activation mechanics and protected-session coexistence, not multi-day false-positive acceptance or a released default for other machines.

Full command lines, executable paths, and profile paths exist only in transient classification memory. SQLite, IPC, CLI output, and diagnostic exports use redacted typed records that omit signal targets and session identifiers.

## Documentation

- [Product & Technical Specification 0.1](docs/SPEC.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Safety model](docs/SAFETY.md)
- [Signature packs](docs/SIGNATURES.md)
- [Privacy](docs/PRIVACY.md)
- [Field Lab](docs/FIELDLAB.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Current state](docs/current-state.md)

The repository is private. No public license has been selected. Before any visibility change, the project still requires an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a public-safety/privacy scan.
