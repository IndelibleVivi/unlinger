# Unlinger repository contract

## Authority

- `docs/SPEC.md` is the current product and technical contract for the 0.1 programme. It remains labelled a working draft; changes to product meaning, safety thresholds, supported families, or programme order require the project owner's explicit decision.
- `docs/IMPLEMENTATION_PLAN.md` owns implementation coverage and tranche status. It may change technique, not product scope.
- `docs/current-state.md` owns volatile source, candidate, installed, activated, field-verified, and release truth.
- The origin conversation/export is private provenance, not repository authority. Never copy, stage, commit, publish, or quote it here.

## Current hard boundary

The repository contains an enforcement engine, but the ordinary daemon default and the installed dogfood service are report-only. Source and synthetic verification do not authorize ambient activation.

- Inspect `unlinger service status` before replacing or reloading the active service. Use the transactional service CLI rather than manual binary/plist copying or ad hoc `launchctl` mutation unless diagnosing that lifecycle itself.
- Do not run `unlingerd --enforce` against ordinary machine state, enable ambient enforcement, uninstall the service, or change the installed mode without an applicable owner-approved field boundary. Preserve the active report-only service while doing source-only work.
- Tests may signal only a process they create and retain exact ownership of; the macOS integration test uses an isolated `/bin/sleep` child.
- `crates/unlinger-daemon/tests/cft_fieldlab.rs` is an ignored, explicit owner-approved live path. It requires a Chrome-for-Testing app bundle, refuses ordinary Chrome, scopes every signal to exact identities admitted from its unique profile tree, and retains its profile for inspection. Its fast timing profile proves mechanics only; set `UNLINGER_FIELDLAB_FULL_TIMING=1` for the production 90/15/60 timing contract.
- `crates/unlinger-daemon/tests/managed_cft_fieldlab.rs` is the installed-generation acceptance path. It additionally exercises exact managed lifecycle identity, fresh-epoch re-arm after a same-generation restart, retry suppression, and final report-only containment. Run it only with its explicit acknowledgement environment and an exact active-generation CLI path.
- Automatic artifact cleanup is limited to an exact admitted `DevToolsActivePort` file. Live-reference proof uses Darwin's targeted `proc_listpidspath` query for the exact canonical/quarantine pathname plus a complete current-user argv pass; it must fail closed on any incomplete query or identity check. No source path deletes browser profiles or runtime directories.
- The artifact path still has two known P2 residuals: a crash after canonical-to-quarantine rename may strand the exact quarantine entry, and the final pathname revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU. One controlled managed field run removed the admitted DAP successfully; do not turn that point result into broad artifact acceptance or widen eligibility while these boundaries remain.
- Managed `Failed` is terminal for that daemon instance. `Disarm` may durably remove any stale signal authority but must not rehabilitate it; replacement proceeds only through exact generation/instance validation, `BeginDrain`, captured-process bootout, and a fresh report-only restart. A ready report-only service acceptance also requires a stable quiescent projection: no scan or cleanup in progress.
- IPC clients make one bounded attempt. The ordinary/default and service clients use a 15-second I/O timeout; each accepted server connection uses 3 seconds and the daemon serves at most eight connections concurrently. Never automatically resend a timed-out mutation or lifecycle command, because its delivery may already have committed; read back exact state instead. The managed field harness is the narrow exception only for read-only exact-incident `Explain`: it polls on a separate worker and may retry `not_found`, unavailable, or transient local I/O within its overall deadline while native identity sampling continues.
- Frontend schema v3 and the native SwiftUI client are source-complete but not installed or integrated with generation 9. V3 exposes only `unlinger-protocol` ordinary commands/public DTOs, namespace-aware mutation receipts/status, exact readiness, roster freshness, event tokens, diagnostics and bounded notifications; lifecycle commands do not exist in that enum. Schema v2 is historical and must receive typed `unsupported_schema`; never silently downgrade the App to v1 `DaemonStatus` or v1 mutations.
- Source SQLite schema v6 is incompatible with the installed generation-9 v5 binary. The current service transaction has no acceptance-scoped rollback lease after candidate-ready. Until that lease retains the prior manifest/plist/v5 database through explicit acceptance and a real generation-9 rollback/open test passes, do not install/reload this candidate, point it at the active database, or claim installed v3 integration.
- Whole-plan `FAILED` and public `cleared_with_residue` may coexist. A known no-removal artifact disposition after exact tree absence/revival proof keeps an incident attention/retry block but does not by itself fail the managed daemon closed. Any process/artifact delivery uncertainty, open PREPARED action, or unproved failure after a delivered side effect still triggers global fail-close.
- A future activation must retain all deterministic gates, durable abandonment grace, frozen-plan revalidation, exact identity signals, terminal receipts, bounded revival behavior, durable action journaling, restart recovery, generation-bound arming, fresh enforcement-epoch cooling, and report-only rollback.

## Canonical paths

- `crates/unlinger-core`: platform-independent identity, graph, incident states, frozen cleanup plans, artifact plans, and cleanup executor.
- `crates/unlinger-protocol`: frontend schema-v3 commands, public DTOs, mutation contexts/receipts/status, capabilities, outcome enums, response envelopes, and canonical fixture decoders. It contains no service lifecycle command.
- `crates/unlinger-rules`: embedded signature packs, sessionization, protection rules, deterministic classification, and pre-signal/revival revalidation.
- `crates/unlinger-macos`: macOS `libproc`/`sysctl` snapshots, transient app-version facts, native process-exit/wake/pressure sources, exact-identity signal adapter, and exact runtime-artifact adapter. A PID confirmed as `SZOMB` through `KERN_PROC_PID` is gone for live-identity purposes rather than an unreadable live process. `/bin/ps`, process-name kills, and broad PGID kills are not production paths.
- `crates/unlinger-daemon`: durable cooling grace, native-hint/periodic scheduler, SQLite v6 history/action/mutation journals, stable public event/receipt authority, shared public-action policy, retention, local IPC, managed lifecycle state, and reconciliation engine.
- `crates/unlinger-cli`: status, history, explain, doctor, pause/resume, retry, protect/unprotect, dry-run scan, redacted diagnostic export, and transactional LaunchAgent lifecycle.
- `apps/UnlingerApp`: SwiftPM native menu-bar thin client. `Sources/UnlingerKit` owns strict schema-v3 transport/DTOs, owner-private persistence, durable mutation state, notifications, navigation, settings and UI projection; `Sources/UnlingerApp` owns launch modes; `Tests/UnlingerAppTests` owns fixture/transport/mutation/concurrency/notification/live-socket coverage. `scripts/bundle.sh` assembles the private ad-hoc-signed app and `scripts/pre-v0.1-smoke.sh` owns the isolated report-only integration gate. `Contract/v3` is active; `Contract/v2` is historical only.
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
