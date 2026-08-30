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
- redacted SQLite timelines with 14-day/10,000-event retention;
- a 0600 local Unix-domain socket for status, history, explain, pause/resume, and diagnostic export;
- a periodic daemon whose default mode is report-only.

This is **source-complete for the implemented paths, not an activated product release**. No LaunchAgent is installed or loaded, ambient automatic cleanup has not been enabled on this Mac, and no live dogfood or public-alpha safety claim exists. Wake/memory-pressure/exit events, low-risk runtime-artifact cleanup, packaging, universal release, signing/notarization, comparative Field Lab evidence, and public distribution remain open.

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

Run the daemon in its safe default mode, then use the local CLI from another terminal:

```bash
cargo run -p unlinger-daemon -- --report-only

cargo run -p unlinger-cli -- status
cargo run -p unlinger-cli -- history
cargo run -p unlinger-cli -- explain <incident-id>
cargo run -p unlinger-cli -- pause 2h
cargo run -p unlinger-cli -- resume
cargo run -p unlinger-cli -- export-diagnostics <incident-id>
```

`scan` always requires `--dry-run` and never sends signals. `unlingerd --enforce` exists for isolated integration and later field acceptance, but it is not the default, has not been installed as a service, and must not be described as dogfood-ready from source/tests alone.

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
