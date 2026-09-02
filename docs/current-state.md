# Current state

**Updated:** 2026-09-02

**Programme:** Unlinger 0.1

**Source:** private pre-v0.1 schema-v4 process-only source-complete candidate; policy-version `0.3.0` preserves exact process TERM/KILL eligibility and disables runtime-artifact admission in every embedded pack; focused red/green policy/harness tests and the full local Rust/Swift/release/bundle/isolated-smoke gate pass; exact-head remote CI remains pending

**Remote:** BGX-2 implementation head `19c58e5a4e486a13a1ba5af85f5c7f932af81023` passed exact-head private macOS `backend` run `33608082841`; documentation-closure head `a52ed6b6741900f81b593563c095c152752c4092` passed run `33608496345`; final pre-install state head `31309a41a2660ecb8771963a92de264bd338bdc7` passed run `33609191047`. The process-only policy/harness/documentation candidate does not yet have an exact-head CI result or installed claim.

**Installed runtime:** accepted generation 13, frontend schemas v4/v3 plus operator schema v1, SQLite v6, healthy `ReadyReportOnly`; no candidate rollback lease remains

**Activation:** generation 13 proved exact generation-bound arm and real process/artifact cleanup in the historical controlled full-timing harness, then was briefly re-armed and read back successfully. It is now deliberately contained report-only because process termination was authorized but the two artifact-removal P2 residuals were not separately accepted. The schema-v4 packaged App is installed and running. Process eligibility remains limited to exact admitted CfT `151.0.7922.34`; currently observed CfT `152.0.7977.42` sessions remain `PROTECTED`. The process-only source candidate is not active.

**Highest claim currently permitted:** **private enforcement candidate for the exact admitted point** (acceptance level 4). Ambient enforcement is currently contained and has not been accepted as multi-day or broad field evidence.

## Source truth

The backend source now uses SQLite schema v6 and accepts frontend schemas v4 and 3 while preserving schema-v1 CLI/service compatibility. Schema v4 adds the daemon-owned atomic browser product projection; schema v3 remains a transition endpoint whose existing responses retain their schema and meaning but rejects `browser_overview` without downgrade. Schema v2 remains historical and is rejected with typed `unsupported_schema`. The source App emits v4 only and never falls back to v3 or v1.

All embedded rule packs are now policy version `0.3.0`. Their exact process eligibility, protection gates, timing, revalidation, TERM/KILL sequence, revival handling and durable process-action journal are unchanged. Every pack sets `devtools_active_port = false`, so analysis produces no runtime-artifact candidate. The managed acceptance verifier requires both terminal receipt and durable attempt journal to contain zero artifact actions and records a separate process-only proof artifact. The existing DAP engine and its focused tests remain dormant for a future explicitly authorized residual-resolution lane.

Frontend ordinary mutations carry a public-safe namespace token plus canonical UUID. The daemon serializes lifecycle state with a `BEGIN IMMEDIATE` store transaction, replays an exact stored receipt before current lifecycle policy, re-evaluates authoritative durable facts through one shared public-action policy, applies state, advances a durable cleanup-policy revision and stores a typed `applied | no_change | rejected` receipt atomically. Same-ID canonical replay is idempotent; conflicting reuse fails; old-namespace missing requests cannot apply. Receipt pruning rotates namespace atomically and retains a minimum 14-day reconciliation window.

SQLite v6 also assigns stable opaque public event tokens, retains storage-recovery identity across clean restarts, and commits each observation cycle's history as one batch before publishing the roster. The public roster distinguishes `never_observed`, `scan_in_progress`, `current` and `stale_after_failure`, with optional time/token when no real snapshot exists. Last scan uses cycle completion rather than cycle start.

BGX-2 adds typed browser product/version compatibility to the transient `IncidentReport` only; serde skips it for persisted observations, so the database schema remains v6 and history does not retain app-bundle facts. `RuleSet` generates the public family/product/admitted-version/action-level catalog from embedded version policies with a readable support revision. `ControlPlane` captures status and roster under one in-memory boundary, then `public_ipc` produces the canonical phase, current session summaries, typed coverage, attention/protection and recent settlement. Untrusted readiness/freshness/time becomes `unknown`; trusted phase precedence is server-owned. Settlement joins exact cleanup event token and earlier event ID, including same-millisecond histories, rather than depending on a bounded App history page.

The native App under `apps/UnlingerApp` now has:

- strict schema-v4 DTO/envelope decoding, explicit v3 fixture compatibility and a narrow exact v1 incompatibility parser;
- phase-aware, single-attempt, cancellable socket I/O;
- an owner-private crash-durable pending-mutation journal written before connect/send;
- one global unresolved mutation lock and startup/status-only reconciliation without resend;
- one pure `BrowserOverviewMapper` that maps daemon-owned v4 product truth into localized copy/display shapes without recomputing phase, scanning evidence or joining history;
- one browser overview request per coalesced refresh; transport loss retains prior rows as stale context but forces an App-local unknown phase;
- browser-first overview phases, typed family/session compatibility, closed-set coverage explanations, saved protections, exact settlement and a rule-generated support catalog, without raw evidence IDs or unsupported global process/RSS totals;
- browser-context detail from a coherent current row, retained observation or exact joined settlement, while absent estimates remain absent;
- coalesced refresh generations, polling-session isolation, stale browser-snapshot retention and typed incident-detail failures;
- local-view diagnostics results with required `document_schema_version` and semantic-lossless JSON export;
- stable event/action identity, including artifact-only action groups;
- bilingual browser copy and VoiceOver labels with language-matched value formatting, plus Settings/About/Quit surfaces, shared notification routing and menu-client-only `SMAppService.mainApp` launch at login;
- a direct AppKit `@main` that strongly retains the single delegate for the blocking App run loop, with AppKit-owned `NSStatusItem`/`NSPopover` and reusable ordinary-window lifecycles around one shared SwiftUI state/router; the packaged App remains a regular foreground/Dock app, closing the last ordinary window does not terminate it, and Dock reopen plus popover detail retain explicit window/Back/current-route controls;
- a release bundler that builds in an internal temporary SwiftPM scratch path, removes removable-volume toolchain rpaths, and rejects packaged executables that retain removable-volume resource fallbacks or loader paths;
- bounded local notifications: `off`, default `attention`, or `attention_and_reclaims`; first trusted refresh baselines retained tokens, suppressed events remain seen, and durable claim precedes one schedule attempt.

Notifications are best-effort local projections over bounded polling. They have no sound, are quiet in foreground, use public-safe route data, and never treat an App mutation response fault as a backend cleanup event. Runtime remains local-only with no account, telemetry, cloud sync or normal-operation network behavior.

The service source extends its durable install transaction through `CandidateReadyReportOnly`, `AcceptanceInProgress`, `RollbackInProgress` and `Accepted`. `DatabaseBackedUp` is a conservative restore boundary because a crash may occur after either candidate selection file is published but before `CandidateSelected` is durable. Rollback validates the immutable backup's recorded schema and the sealed prior daemon/CLI/manifest, persists replay intent before physical mutation, accepts only transaction-owned candidate/prior/mixed selection cuts, restores the same snapshot and republishes the prior report-only selection until convergence. Offline containment directly updates the compatible managed-lifecycle row and verifies that the database `user_version` is unchanged; it no longer opens a restored prior database through the current migrating `HistoryStore::open` path.

`restart-report-only` still requires exact candidate selection, a report-only manifest and executable rollback material, but it no longer requires a stopped, PID-less or terminal-failed candidate to already be healthy before replacement. Acceptance remains stricter: exact healthy, quiescent ReadyReportOnly plus report-only desired mode and valid rollback material. Service-command JSON now serializes a dedicated schema-v1 public projection that retains lifecycle/lease truth while excluding PID, instance ID, absolute paths and raw errors.

The owner-only managed harness consumes that public lifecycle projection and follows each mutation with one separate read-only schema-v1 daemon-status call for instance, generation and enforcement-epoch proof. Machine-local database/socket paths come from `LocalPaths`; the harness does not restore private fields to service-command JSON and never resends a lifecycle mutation after uncertain delivery.

## Installed evidence

The level-3 rollback lease was exercised rather than merely inspected: an earlier candidate restored generation 9 and its SQLite-v5 snapshot, and the exact generation-9 CLI/daemon reopened that database healthy at `ReadyReportOnly`. Generation 12 was then reinstalled, its restart path passed, and the owner explicitly accepted it on 2026-09-02, durably retiring the generation-9 lease before another install.

The exact BGX-2 release binaries were installed transactionally as generation 13 in report-only mode. Readback proved healthy `ReadyReportOnly`, exact launchd/IPC/generation/binary/permission agreement and a new rollback lease to generation 12. The matching schema-v4 ad-hoc-signed App bundle passed strict signature, plist and byte-for-byte build-bundle comparison, replaced the prior App recoverably, and launched from the installed Applications location. The owner then accepted generation 13, retiring its generation-12 lease. That generation-13 lease was validated but was **not actually executed before acceptance**; the earlier generation-12→9 rollback does not substitute for a generation-13 candidate rollback. The next candidate must close this gap through install, restart, real rollback to generation 13, reinstall as a fresh generation, repeat readiness/restart, and only then accept.

The first generation-13 managed field attempt stopped before launching CfT because the harness still expected the pre-BGX-2 private service JSON. Service readback confirmed healthy report-only containment. The harness was corrected to join the public service projection with one independent read-only daemon status and locally derived service paths; its focused unit suite passed before the live retry.

The corrected full-timing run passed in 378.73 seconds. Incident `inc-e2cabdde69de66dd` contained eight exact CfT 151 processes; the durable receipt and journal agreed on eight TERM actions followed by one exact-root KILL, zero survivors, two revival checks, `52,838,400` estimated bytes reclaimed and one DAP `removed` disposition. Same-generation restart changed daemon instance and enforcement epoch without duplicating journal state. The 0700 profile and eight 0600 proof artifacts remain retained outside Git. The harness ended healthy report-only; the owner-authorized postflight then re-armed the accepted generation 13.

Post-arm stable readback showed generation 13 healthy `ReadyEnforce`, exact armed generation and a fresh epoch, no cleanup in progress, and zero attention items. The v4 atomic overview was `current/protected`: it retained the exact generation-13 settlement and protected all three observed CfT `152.0.7977.42` sessions with `protection.browser_version_unsupported`. After independent review identified that process authorization did not separately accept the two artifact-removal P2s, the exact same generation was contained without restart. Current readback is healthy, quiescent `ReadyReportOnly` with no armed generation or enforcement epoch. The installed App remains running, and the prior App bundle remains as a recoverable owner-local sibling.

The direct AppKit Dock/window fallback remains the supported reachability path if an external menu host cannot place the status item. The previously diagnosed Thaw/macOS hosted-menu compatibility gap remains unresolved and must not be reported as fixed.

## Safety and field truth

Automatic process eligibility remains limited to controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34`, plus every existing identity, isolation, durable abandonment, stability and protection gate. Agent-browser, Playwright and Puppeteer are recognized/classified families; only the exact controllerless CfT point is automatically eligible. Controller-bearing, headed/attached, standard/shared profile, wrong/mixed/unknown version and incomplete identity stay protected.

Current source automatic artifact cleanup is disabled: all `0.3.0` packs produce zero runtime-artifact candidates and actions. The dormant exact `DevToolsActivePort` path retains two P2 residuals: a crash after canonical-to-quarantine rename can strand the exact quarantine entry, and the final path revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU.

Generation 9 and accepted generation 13 each prove one historical owner-approved managed full-timing Playwright-style CfT process/artifact/restart point. The generation-13 point proves the installed schema-v4 candidate performed real exact cleanup and recovered with a fresh epoch before final harness containment. It does not prove the uninstalled process-only source candidate. The later explicit re-arm proves activation state only; it does not establish ambient eligibility, multi-day zero-false-positive dogfood, or broader families/versions.

## Verification truth

The current process-only source gate passed locally on 2026-09-02:

- `cargo fmt --all -- --check`, strict workspace clippy and the release workspace build passed;
- `cargo test --workspace` passed 287 tests; the two owner-only live CfT tests remained ignored;
- source-only doctor inspected 436/436 listed processes with zero unreadable, argument-unavailable or descriptor-unavailable entries, 18 executable-identity-unavailable entries, all three signature packs at `0.3.0`, `healthy: true` and no errors;
- dry-run inspected 435/435 listed processes with the same complete readable/argument/descriptor coverage, 18 executable-identity-unavailable entries and three `PROTECTED` CfT 152 automation sessions; no eligible incident, runtime-artifact candidate or signal action was produced;
- `swift test` passed 73 tests in 15 suites, including strict v4 fixture decoding, daemon-phase preservation, typed compatibility/unknown copy, direct settlement projection, single-overview refresh, stale-snapshot containment, all seven product phases, bilingual VoiceOver copy, direct AppKit hosts and existing transport/mutation/notification contracts;
- the release App bundle assembled from its internal scratch path, packaged explicit v4 and v3 fixture directories, validated localizations, passed ad-hoc signature verification, and rejected source-volume resource fallback and removable-volume loader paths;
- the isolated smoke passed seven live-socket suite tests, including the v4 atomic browser overview, restarted the source daemon over the same private temporary SQLite database, then passed the same seven tests again. It verified effective report-only mode, durable receipt replay and owner-private temp/database/socket/lock modes, and removed only its owned temporary root; and
- no new rendered QA was required because the process-only change touches rule policy, field verification and documentation without changing the App layout or localized copy. Prior BGX-1 English/Simplified Chinese and AX observations remain historical UI evidence, not a fresh process-only runtime proof.

BGX-2 implementation head `19c58e5` passed private GitHub exact-head macOS `backend` run `33608082841`, including formatting, strict clippy, workspace tests, release workspace build, native frontend tests and native frontend bundle. Its documentation-closure head `a52ed6b` passed the same workflow in run `33608496345`; BGX-1 documentation-closure head `8c9e47e` remains separately verified by run `33597135228`.

The reproducible commands were:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
cargo run -p unlinger-cli -- doctor --source-only --json
cargo run -p unlinger-cli -- scan --dry-run --json
cd apps/UnlingerApp
swift test
scripts/bundle.sh
scripts/pre-v0.1-smoke.sh
```

Ordinary workspace tests still ignore both owner-only CfT harnesses. Separately, the explicitly acknowledged generation-13 `managed_cft_fieldlab` ran at full production timing and passed the exact managed process/artifact/restart contract. The accepted installed generation was then re-armed and read back at stable `ReadyEnforce`; this operational evidence is not part of ordinary source validation.

For the current process-only working tree, strict red/green tests first failed on the old `0.2.0` pack/artifact expectation and the historical mandatory-artifact managed receipt contract. After the policy and verifier change, `unlinger-rules` passes 16/16, the nonignored `managed_cft_fieldlab` suite passes 8/8 with its live case still ignored, and the full gate above passes. This establishes local source completeness only; it does not establish remote CI, installation, candidate rollback, live process-only cleanup or activation.

## Open gates

- multi-day ambient dogfood and an ordinary real eligible incident;
- owner observation of the installed schema-v4 App's final visual and packaged notification behavior;
- Thaw/macOS hosted-menu compatibility for the still-unresolved status item; this external presentation gap does not authorize further private-default or identity workarounds in Unlinger;
- exact field evidence before admitting CfT `152.0.7977.42` or any other version/family expansion;
- push and exact-head CI for the locally source-complete `0.3.0` process-only candidate;
- transactional install with a fresh lease to generation 13, exact restart, real rollback to generation 13, fresh reinstall/restart, then acceptance;
- one managed full-timing process-only run proving exact process cleanup, restart semantics, and zero artifact candidates/receipt actions/journal rows before ambient arm;
- resolution or explicit product acceptance of both artifact P2 residuals before any future artifact re-enable;
- broader family/version/controller field evidence, chaos, sleep/wake and sustained-pressure evidence;
- Intel/universal build evidence;
- Developer ID signing, notarization, packaging/update/rollback and distribution;
- public-safe repository/license/rights decision, public alpha and public release.

See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md), [`SUPPORT.md`](SUPPORT.md), and [`support-matrix.v1.json`](support-matrix.v1.json) for the exact claim and support vocabulary.
