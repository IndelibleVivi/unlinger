# Unlinger repository contract

## Authority

- `docs/SPEC.md` is the current product and technical contract for the 0.1 programme. It remains labelled a working draft; changes to product meaning, safety thresholds, supported families, or programme order require the project owner's explicit decision.
- `docs/IMPLEMENTATION_PLAN.md` owns implementation coverage and tranche status. It may change technique, not product scope.
- `docs/current-state.md` owns volatile source, candidate, installed, activated, field-verified, and release truth.
- The origin conversation/export is private provenance, not repository authority. Never copy, stage, commit, publish, or quote it here.

## Current hard boundary

The ordinary daemon source default is report-only. The project owner explicitly authorized the first current-user LaunchAgent and ambient enforcement dogfood on 2026-08-30; the installed dogfood service is expected to remain loaded in enforce mode unless the owner changes that boundary.

- Do not treat this one-host authorization as permission to activate another host, widen eligibility, delete runtime artifacts, or claim multi-day/release acceptance.
- Inspect `unlinger service status` before replacing or reloading the active service. Use the transactional service CLI rather than manual binary/plist copying or ad hoc `launchctl` mutation unless diagnosing that lifecycle itself.
- Future uninstall, pause of dogfood, or mode changes still require an applicable owner request. Preserve the active service while doing source-only work.
- Tests may signal only a process they create and retain exact ownership of; the macOS integration test uses an isolated `/bin/sleep` child.
- `crates/unlinger-daemon/tests/cft_fieldlab.rs` is an ignored, explicit owner-approved live path. It requires a Chrome-for-Testing app bundle, refuses ordinary Chrome, scopes every signal to exact identities admitted from its unique profile tree, and retains its profile for inspection. Its fast timing profile proves mechanics only; set `UNLINGER_FIELDLAB_FULL_TIMING=1` for the production 90/15/60 timing contract.
- No current source path deletes browser profiles or runtime directories.
- A future activation must retain all deterministic gates, durable abandonment grace, frozen-plan revalidation, exact identity signals, terminal receipts, and bounded revival behavior.

## Canonical paths

- `crates/unlinger-core`: platform-independent identity, graph, incident states, frozen cleanup plans, and cleanup executor.
- `crates/unlinger-rules`: embedded signature packs, sessionization, protection rules, deterministic classification, and pre-signal/revival revalidation.
- `crates/unlinger-macos`: macOS `libproc`/`sysctl` snapshots and exact-identity signal adapter. `/bin/ps`, process-name kills, and broad PGID kills are not production paths.
- `crates/unlinger-daemon`: durable cooling grace, scheduler, SQLite history, retention, blocking local IPC, control state, reconciliation engine, graceful shutdown, and canonical per-user paths.
- `crates/unlinger-cli`: status, history, explain, doctor, pause/resume, dry-run scan, redacted diagnostic export, and transactional LaunchAgent install/status/set-mode/uninstall.
- `rules/*.toml`: canonical source for embedded signature packs. Every eligibility expansion requires a positive fixture and the nearest normal/manual counterexample.
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
cargo run -p unlinger-cli -- doctor --json
cargo run -p unlinger-cli -- scan --dry-run --json
```

Daemon/IPC tests must use an explicit temporary database and socket and must remain report-only except for exact owned-child signal tests. The default Library paths now belong to the active dogfood service; do not reuse or delete them for source smoke tests.

Do not treat a clean build, synthetic fixture suite, owned-child signal test, or short activation receipt as evidence that ambient auto-clean is multi-day field-safe, broadly dogfood-proven, signed, or released.

The live CfT harness is excluded from ordinary workspace tests. Run it only inside an explicit owner-approved field boundary:

```bash
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
  cargo test -p unlinger-daemon --test cft_fieldlab -- --ignored --nocapture --test-threads=1
```

## Documentation and publication triggers

Update `README.md` for user-visible commands, defaults, platform support, privacy boundaries, or activation claims. Update this file when canonical paths or hard gates move. Update `docs/current-state.md` whenever source, candidate, installed, activated, field-verified, remote, or release status changes. Update `docs/SAFETY.md`, `docs/SIGNATURES.md`, and `docs/PRIVACY.md` with their corresponding contracts.

The private repository may receive source-safe English working documentation. Before any public visibility change, complete an owner-approved license/rights decision, bilingual reader documentation, a publication-grade architecture diagram, field evidence, and a tracked-file privacy/provenance scan. No repository license may be added by habit.
