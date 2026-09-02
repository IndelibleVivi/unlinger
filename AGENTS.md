# Unlinger repository contract

## Authority

- `docs/SPEC.md` is the current product and technical contract for the 0.1 programme. It remains labelled a working draft; changes to product meaning, safety thresholds, supported families, or programme order require the project owner's explicit decision.
- `docs/IMPLEMENTATION_PLAN.md` owns implementation coverage and tranche status. It may change technique, not product scope.
- `docs/current-state.md` owns volatile source, candidate, installed, activated, field-verified, and release truth.
- The origin conversation/export is private provenance, not repository authority. Never copy, stage, commit, publish, or quote it here.

## Current hard boundary

The repository contains an enforcement engine, but the ordinary daemon default and accepted installed generation 13 are currently report-only. Source and synthetic verification do not authorize ambient activation. The owner has authorized a process-only replacement lane, but it must complete source/CI, transactional rollback/reinstall, installed full-timing acceptance, and final exact readback before ambient arm.

- Inspect `unlinger service status` before replacing or reloading the active service. Use the transactional service CLI rather than manual binary/plist copying or ad hoc `launchctl` mutation unless diagnosing that lifecycle itself.
- Do not run `unlingerd --enforce` against ordinary machine state, enable ambient enforcement, uninstall the service, or change the installed mode without an applicable owner-approved field boundary. Preserve the active report-only service while doing source-only work.
- Tests may signal only a process they create and retain exact ownership of; the macOS integration test uses an isolated `/bin/sleep` child.
- `crates/unlinger-daemon/tests/cft_fieldlab.rs` is an ignored, explicit owner-approved live path. It requires a Chrome-for-Testing app bundle, refuses ordinary Chrome, scopes every signal to exact identities admitted from its unique profile tree, and retains its profile for inspection. Its fast timing profile proves mechanics only; set `UNLINGER_FIELDLAB_FULL_TIMING=1` for the production 90/15/60 timing contract.
- `crates/unlinger-daemon/tests/managed_cft_fieldlab.rs` is the installed-generation acceptance path. It additionally exercises exact managed lifecycle identity, fresh-epoch re-arm after a same-generation restart, retry suppression, final report-only containment, and the current process-only invariant: no runtime-artifact candidate and zero receipt/journal artifact actions. Run it only with its explicit acknowledgement environment and an exact active-generation CLI path.
- The DAP cleanup engine remains implemented, but every current policy-version `0.3.0` pack sets `devtools_active_port = false`. The active source policy therefore cannot admit, schedule, deliver, or journal runtime-artifact cleanup. No source path deletes browser profiles or runtime directories.
- The dormant artifact path still has two known P2 residuals: a crash after canonical-to-quarantine rename may strand the exact quarantine entry, and the final pathname revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU. Historical generation-9 and generation-13 controlled runs removed the admitted DAP; do not turn those point results into current artifact acceptance or re-enable any pack flag without a separate owner decision after the residuals are resolved or explicitly accepted.
- Managed `Failed` is terminal for that daemon instance. `Disarm` may durably remove any stale signal authority but must not rehabilitate it; replacement proceeds only through exact generation/instance validation, `BeginDrain`, captured-process bootout, and a fresh report-only restart. A ready report-only service acceptance also requires a stable quiescent projection: no scan or cleanup in progress.
- IPC clients make one bounded attempt. The ordinary/default and service clients use a 15-second I/O timeout; each accepted server connection uses 3 seconds and the daemon serves at most eight connections concurrently. Never automatically resend a timed-out mutation or lifecycle command, because its delivery may already have committed; read back exact state instead. The managed field harness is the narrow exception only for read-only exact-incident `Explain`: it polls on a separate worker and may retry `not_found`, unavailable, or transient local I/O within its overall deadline while native identity sampling continues.
- Installed generation 13 and its native App use frontend schema v4 with a strict v3 transition endpoint, operator schema v1, and SQLite v6. The App emits v4 only and requires the daemon-owned atomic `browser_overview`. Both frontend schemas expose only `unlinger-protocol` ordinary commands/public DTOs and contain no lifecycle command. Schema v2 is historical and must receive typed `unsupported_schema`; never silently downgrade the App to v3 or v1.
- Generation 13 is accepted, healthy `ReadyReportOnly`, and has no rollback lease. The next process-only candidate must be installed report-only with a fresh lease to generation 13, pass exact `restart-report-only`, actually execute `rollback-candidate` and prove generation 13 reopens healthy, then be reinstalled as a fresh generation and repeat readiness/restart checks before `accept-candidate`. `restart-report-only` requires exact candidate selection, valid executable rollback material and a report-only manifest; acceptance is stricter and requires a healthy, quiescent exact ReadyReportOnly projection with no arm/enforcement authority. Rollback persists `RollbackInProgress` before physical mutation and must replay only transaction-owned candidate/prior selection states until the prior report-only generation is healthy.
- Whole-plan `FAILED` and public `cleared_with_residue` may coexist. A known no-removal artifact disposition after exact tree absence/revival proof keeps an incident attention/retry block but does not by itself fail the managed daemon closed. Any process/artifact delivery uncertainty, open PREPARED action, or unproved failure after a delivered side effect still triggers global fail-close.
- The authorized next activation is process-only and must retain all deterministic gates, durable abandonment grace, frozen-plan revalidation, exact identity signals, terminal receipts, bounded revival behavior, durable process-action journaling, restart recovery, generation-bound arming, fresh enforcement-epoch cooling, and report-only rollback. Runtime-artifact admission remains disabled.

## Canonical paths

- `crates/unlinger-core`: platform-independent identity, graph, incident states, frozen cleanup plans, artifact plans, and cleanup executor.
- `crates/unlinger-protocol`: frontend schema-v4 commands and public DTOs, the transitional v3 response contract, mutation contexts/receipts/status, capabilities, outcome enums, response envelopes, and canonical fixture decoders. It contains no service lifecycle command.
- `crates/unlinger-rules`: embedded signature packs, sessionization, typed browser product/version compatibility, the generated public support catalog, protection rules, deterministic classification, and pre-signal/revival revalidation.
- `crates/unlinger-macos`: macOS `libproc`/`sysctl` snapshots, transient app-version facts, native process-exit/wake/pressure sources, exact-identity signal adapter, and exact runtime-artifact adapter. A PID confirmed as `SZOMB` through `KERN_PROC_PID` is gone for live-identity purposes rather than an unreadable live process. `/bin/ps`, process-name kills, and broad PGID kills are not production paths.
- `crates/unlinger-daemon`: durable cooling grace, native-hint/periodic scheduler, SQLite v6 history/action/mutation journals, stable public event/receipt authority, one atomic schema-v4 browser overview projection, shared public-action policy, retention, local IPC, managed lifecycle state, and reconciliation engine.
- `crates/unlinger-cli`: status, atomic `browser status`, history, explain, doctor, pause/resume, retry, protect/unprotect, dry-run scan, redacted diagnostic export, and transactional LaunchAgent lifecycle.
- `apps/UnlingerApp`: SwiftPM native menu-bar thin client. The daemon's schema-v4 `BrowserOverviewSnapshot` is the one browser-product truth; `Sources/UnlingerKit/State/BrowserOverviewMapper.swift` performs localization/presentation only and must not recompute phase, compatibility, coverage or settlement. UI files consume its public-safe presentation types and must not reconstruct product state independently. `Sources/UnlingerKit` also owns strict transport/DTOs, owner-private persistence, durable mutation state, notifications, navigation, settings, and the AppKit-owned status-item popover/reusable ordinary window hosts; `Sources/UnlingerApp` owns launch-mode wiring. `Tests/UnlingerAppTests` owns fixture/transport/mutation/concurrency/notification/window-host/live-socket coverage. `scripts/bundle.sh` assembles the private ad-hoc-signed app and `scripts/pre-v0.1-smoke.sh` owns the isolated report-only integration gate. `Contract/v4` is active for the browser product contract, `Contract/v3` is transitional compatibility, and `Contract/v2` is historical only.
- `rules/*.toml`: canonical source for embedded signature packs. The shared deterministic Rust sessionizer is parameterized by pack data; packs do not replace safety logic. Every eligibility expansion requires a positive fixture and the nearest normal/manual counterexample.
- `fixtures/macos`: synthetic or deliberately redacted topology only. Never add page contents, credentials, real usernames, repository paths, private profile identifiers, or raw private command lines.

## Safety and privacy

- A score may explain or rank; it never authorizes cleanup.
- Age, CPU, PPID, process name, or one launch flag never proves a ghost incident.
- Standard profiles, headed/manual sessions, attached CDP browsers, persistent/shared profiles, other users, root/system processes, Unlinger itself, and its ancestors are protected.
- Full arguments and executable paths may exist only in transient snapshot memory. Persisted/displayed records use typed redacted evidence; frozen signal targets and session fingerprints are not serialized into history.
- Runtime operation has no telemetry, account, cloud sync, or normal-operation network behavior.
- Private working continuity belongs outside the Git worktree. Do not name a machine-local continuity path in tracked files.
- Stage explicit public-safe paths only. Never use a catch-all stage command in this repository.

## Verification

Run the narrow relevant command first, then the workspace checks before a source-complete claim:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
cargo run -p unlinger-cli -- doctor --source-only --json
cargo run -p unlinger-cli -- scan --dry-run --json
cd apps/UnlingerApp && swift test
scripts/bundle.sh
scripts/pre-v0.1-smoke.sh
```

Daemon/IPC tests must use an explicit temporary database and socket and must remain report-only except for exact owned-child signal tests. The default Library paths now belong to the active dogfood service; do not reuse or delete them for source smoke tests.

Do not treat a clean build, synthetic fixture suite, owned-child signal test, or short activation receipt as evidence that ambient auto-clean is multi-day field-safe, broadly dogfood-proven, signed, or released.

The live CfT harnesses are excluded from ordinary workspace tests. Run one only inside its explicit owner-approved field boundary:

```bash
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
  cargo test -p unlinger-daemon --test cft_fieldlab -- --ignored --nocapture --test-threads=1

UNLINGER_FIELDLAB_MANAGED_ACK=I_ACCEPT_INSTALLED_CFT_SIGNALING \
UNLINGER_FIELDLAB_FULL_TIMING=1 \
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
UNLINGER_FIELDLAB_MANAGED_CLI="/path/to/active/generation/unlinger" \
  cargo test -p unlinger-daemon --test managed_cft_fieldlab -- --ignored --nocapture --test-threads=1
```

## Documentation and publication triggers

Update `README.md` for user-visible commands, defaults, platform support, privacy boundaries, or activation claims. Update this file when canonical paths or hard gates move. Update `docs/current-state.md` whenever source, candidate, installed, activated, field-verified, remote, or release status changes. Update `docs/IPC.md`, `docs/SAFETY.md`, `docs/SIGNATURES.md`, and `docs/PRIVACY.md` with their corresponding contracts.

The private repository may receive source-safe English working documentation. Before any public visibility change, complete an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a tracked-file privacy/provenance scan. No repository license may be added by habit.
