# Unlinger repository contract

## Authority

- `docs/SPEC.md` is the current product and technical contract for the 0.1 programme. It remains labelled a working draft; changes to product meaning, safety thresholds, supported families, or programme order require the project owner's explicit decision.
- `docs/IMPLEMENTATION_PLAN.md` owns implementation coverage and tranche status. It may change technique, not product scope.
- `docs/current-state.md` owns volatile source, candidate, installed, activated, field-verified, and release truth.
- The origin conversation/export is private provenance, not repository authority. Never copy, stage, commit, publish, or quote it here.

## Current hard boundary

The ordinary daemon default remains report-only. Accepted installed generation 19 runs the owner-authorized process-only `0.4.0` policy from source head `f7f1857`, after exact-head CI, a generation-18 report-only install/restart/real rollback to healthy generation 17 and SQLite v7, and a fresh generation-19 install/restart/App-protocol verification, acceptance and explicit arm. It fixes real Chrome clone observation (`.app.bundle`, descriptor-relative no-follow traversal) without adding deletion authority. Generation 17's SQLite-I/O terminal failure was recovered before replacement; the original I/O cause remains unknown. Generation 19 has installed-integration and live observation evidence, but no candidate-specific controlled signal run. Generation 15's CfT-151 Level-4 point remains historical and must not be borrowed as generation-19/CfT-152 field acceptance.

- Inspect `unlinger service status` before replacing or reloading the active service. Use the transactional service CLI rather than manual binary/plist copying or ad hoc `launchctl` mutation unless diagnosing that lifecycle itself.
- Apply the current user authorization to the live-service scope. A request to repair and restore the installed service authorizes the necessary transactional recovery/replacement and restoration of its existing policy; do not request the same permission again merely because this file records an older installed generation. A source-only task must preserve the active service. Do not run a second `unlingerd --enforce`, uninstall, widen eligibility or enable file deletion outside the authorized task and its demonstrated ownership boundary.
- The installed native App was owner-authorized for schema-v5 replacement and live deployment on 2026-09-04. The canonical Applications location contains only the ad-hoc-signed v5 candidate assembled from exact source/docs head `016ca58` (implementation `53142bd`); the superseded v4 bundle was moved to the owner's Trash after acceptance. Live UI readback covered the v5 home and cleanup-only empty-history routes, and a bounded post-install observation kept App RSS at or below 29,408 KiB for more than five minutes. The earlier repaired-navigation acceptance still provides the longer ten-minute/1,398-tree-read memory point. Neither point is multi-day App dogfood, packaged-notification delivery or every external menu host/display arrangement. Do not replace the App or repeat installed-App stress without a new applicable owner decision.
- Tests may signal only a process they create and retain exact ownership of; the macOS integration test uses an isolated `/bin/sleep` child.
- `crates/unlinger-daemon/tests/cft_fieldlab.rs` is an ignored, explicit owner-approved live path. It requires a Chrome-for-Testing app bundle, refuses ordinary Chrome, scopes every signal to exact identities admitted from its unique profile tree, and retains its profile for inspection. Its fast timing profile proves mechanics only; set `UNLINGER_FIELDLAB_FULL_TIMING=1` for the production 90/15/60 timing contract.
- `crates/unlinger-daemon/tests/managed_cft_fieldlab.rs` is the installed-generation acceptance path. It additionally exercises exact managed lifecycle identity, fresh-epoch re-arm after a same-generation restart, retry suppression, final report-only containment, and the current process-only invariant: no runtime-artifact candidate and zero receipt/journal artifact actions. Run it only with its explicit acknowledgement environment and an exact active-generation CLI path.
- The DAP cleanup engine remains implemented, but every source and installed policy-version `0.4.0` pack sets `devtools_active_port = false`. Neither source nor installed policy can admit, schedule, deliver, or journal runtime-artifact cleanup. No source path deletes browser profiles or runtime directories.
- The dormant artifact path still has two known P2 residuals: a crash after canonical-to-quarantine rename may strand the exact quarantine entry, and the final pathname revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU. Historical generation-9 and generation-13 controlled runs removed the admitted DAP; do not turn those point results into current artifact acceptance or re-enable any pack flag without a separate owner decision after the residuals are resolved or explicitly accepted.
- Managed `Failed` is terminal for that daemon instance. `Disarm` may durably remove any stale signal authority but must not rehabilitate it; replacement proceeds only through exact generation/instance validation, `BeginDrain`, captured-process bootout, and a fresh report-only restart. A ready report-only service acceptance also requires a stable quiescent projection: no scan or cleanup in progress.
- IPC clients make one bounded attempt. The ordinary/default and service clients use a 15-second I/O timeout; each accepted server connection uses 3 seconds and the daemon serves at most eight connections concurrently. Never automatically resend a timed-out mutation or lifecycle command, because its delivery may already have committed; read back exact state instead. The managed field harness is the narrow exception only for read-only exact-incident `Explain`: it polls on a separate worker and may retry `not_found`, unavailable, or transient local I/O within its overall deadline while native identity sampling continues.
- Installed generation 19 and the existing native App use frontend schema v5 with strict v4 and v3 compatibility endpoints, operator schema v1, and SQLite v7. The App emits v5 only and requires the daemon-owned atomic `browser_overview`, including impact/residue and observation-span facts. Frontend schemas expose only `unlinger-protocol` ordinary commands/public DTOs and contain no lifecycle command. Schema v2 is historical and must receive typed `unsupported_schema`; never silently downgrade the App to v4, v3 or v1.
- Installed/source `0.4.0` adds independent cleanup-impact authority, observation spans, exact CfT `152.0.7977.42` process eligibility, and observe-only Chrome code-sign clone residue reporting. These facts are installed and active for process-only dogfood after the 2026-09-08 recovery, but not candidate-specific controlled-cleanup evidence and never storage-deletion authority.
- Generation 19 is accepted, healthy `ReadyEnforce`, generation/epoch-bound, and has no rollback lease. Its candidate A generation 18 passed report-only restart and real rollback to generation 17/v7; candidate B generation 19 repeated readiness/restart/current-App socket checks and live residue readback before acceptance and arm. A successful scan alone must never rehabilitate a terminal Failed instance. Future replacements must perform their applicable transaction proof. Rollback persists `RollbackInProgress` before physical mutation and replays only transaction-owned selection states until the prior report-only generation is healthy.
- Whole-plan `FAILED` and public `cleared_with_residue` may coexist. A known no-removal artifact disposition after exact tree absence/revival proof keeps an incident attention/retry block but does not by itself fail the managed daemon closed. Any process/artifact delivery uncertainty, open PREPARED action, or unproved failure after a delivered side effect still triggers global fail-close.
- The installed process-only policy retains all deterministic gates, durable abandonment grace, frozen-plan revalidation, exact identity signals, terminal receipts, bounded revival behavior, durable process-action journaling, restart recovery, generation-bound arming, fresh enforcement-epoch cooling, and report-only rollback. Runtime-artifact admission remains disabled. Do not describe one controlled run plus one stable ambient sweep as multi-day dogfood or an ordinary ambient eligible cleanup.

## Canonical paths

- `crates/unlinger-core`: platform-independent identity, graph, incident states, frozen cleanup plans, artifact plans, and cleanup executor.
- `crates/unlinger-protocol`: current frontend schema-v5 commands/DTOs, transitional v4 browser-overview responses, legacy-compatible v3 responses, mutation contexts/receipts/status, impact/residue/span DTOs, capabilities, outcome enums, response envelopes, and canonical fixture decoders. It contains no service lifecycle command.
- `crates/unlinger-rules`: embedded signature packs, sessionization, typed browser product/version compatibility, the generated public support catalog, protection rules, deterministic classification, and pre-signal/revival revalidation.
- `crates/unlinger-macos`: macOS `libproc`/`sysctl` snapshots, transient app-version facts, native process-exit/wake/pressure sources, exact-identity signal adapter, and exact runtime-artifact adapter. A PID confirmed as `SZOMB` through `KERN_PROC_PID` is gone for live-identity purposes rather than an unreadable live process. `/bin/ps`, process-name kills, and broad PGID kills are not production paths.
- `crates/unlinger-daemon`: durable cooling grace, native-hint/periodic scheduler, SQLite v7 observation spans plus independent impact/action/mutation authority, stable public event/receipt authority, one atomic schema-v5 browser overview projection with v4 compatibility, observe-only storage-residue persistence, shared public-action policy, retention, local IPC, managed lifecycle state, and reconciliation engine.
- `crates/unlinger-cli`: status, atomic `browser status`, history, explain, doctor, pause/resume, retry, protect/unprotect, dry-run scan, redacted diagnostic export, and transactional LaunchAgent lifecycle.
- `apps/UnlingerApp`: SwiftPM native menu-bar thin client. The daemon's schema-v5 `BrowserOverviewSnapshot` is the current browser-product/impact/residue truth; `Sources/UnlingerKit/State/BrowserOverviewMapper.swift` performs snapshot localization/presentation only and must not recompute phase, compatibility, coverage, settlement, impact, or cleanup authority. `Sources/UnlingerKit/State/BrowserHistoryMapper.swift` publishes only terminal cleanup outcomes in the history index and consumes server-owned observation spans in detail; `AppState` invokes both mappers once per state transition and publishes stored presentation values only when they change. SwiftUI bodies must not regroup or sort history during AttributeGraph evaluation. The AppKit popover and ordinary window own independent `AppRouter` instances; menu-to-window navigation is a one-way copy that clears the hidden popover route, and no two long-lived `NavigationStack` hosts may bind one path. `MenuBarPopoverController` owns the square status item, uses the system `circle.dashed` symbol, and assigns stable autosave identity `app.unlinger.menu.primary`; do not restore a parallel packaged menu-icon pipeline. `Sources/UnlingerKit` also owns strict transport/DTOs, owner-private persistence, durable mutation state, notifications, navigation, settings, and the AppKit-owned status-item popover/reusable ordinary window hosts; `Sources/UnlingerApp` owns launch-mode wiring. `Tests/UnlingerAppTests` owns fixture/transport/mutation/concurrency/notification/window-host/live-socket coverage. `scripts/bundle.sh` assembles the private ad-hoc-signed app, `scripts/pre-v0.1-smoke.sh` owns the isolated report-only integration gate, and `scripts/accessibility-memory-smoke.sh` owns the fixture-only history/Accessibility RSS regression gate with an external hard cutoff. `Contract/v5` is current, `Contract/v4` is transitional, `Contract/v3` is legacy-compatible, and `Contract/v2` is historical only.
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
swift test --package-path apps/UnlingerApp
apps/UnlingerApp/scripts/bundle.sh
apps/UnlingerApp/scripts/pre-v0.1-smoke.sh
apps/UnlingerApp/scripts/accessibility-memory-smoke.sh
```

Daemon/IPC tests must use an explicit temporary database and socket and must remain report-only except for exact owned-child signal tests. The default Library paths now belong to the active dogfood service; do not reuse or delete them for source smoke tests.

The Accessibility memory smoke requires an unlocked console session, launches only its own fixture App, opens the synthetic stress-history route, and holds one exact native `AXUIElement` window reference while repeatedly traversing its child tree. The shell samples App RSS independently and terminates only the exact children it created. The probe tolerates up to three consecutive child-tree read misses and fails on the fourth; each successful read resets that count. It does not contact the installed daemon or authorize installed-App relaunch.

The 2026-09-04 installed-App acceptance was a separately owner-authorized private field operation, not an ordinary source-test command. Its shell RSS guard retained exact ownership of the App process and would signal only that App at the hard cutoff; the generation-15 daemon was never restarted, replaced, disarmed or signaled. A temporary standalone native AX helper could not reliably acquire `AXWindows` under its changing helper identity and was recorded as unavailable rather than passed; the accepted full-tree counts came from the stable trusted Computer Use Accessibility transport while the independent RSS guard remained active.

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
