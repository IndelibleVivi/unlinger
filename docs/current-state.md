# Current state

**Updated:** 2026-09-02

**Programme:** Unlinger 0.1

**Source:** private pre-v0.1 schema-v4 candidate; BGX-2 daemon-owned atomic browser projection, typed compatibility/catalog, v4 App and CLI verified locally and at exact remote head

**Remote:** BGX-2 implementation head `19c58e5a4e486a13a1ba5af85f5c7f932af81023` passed exact-head private macOS `backend` run `33608082841`; documentation closure is pending on top.

**Installed runtime:** generation 12, schema-v3/v6 capable, healthy at a stable report-only floor; acceptance lease retains generation 9/v5

**Activation:** unarmed; generation 12 and its earlier schema-v3 packaged App payload remain active for private report-only dogfood. The BGX-2 schema-v4 source candidate is not installed or activated.

**Highest claim currently permitted:** **pre-v0.1 installed report-only candidate** (acceptance level 3)

## Source truth

The backend source now uses SQLite schema v6 and accepts frontend schemas v4 and 3 while preserving schema-v1 CLI/service compatibility. Schema v4 adds the daemon-owned atomic browser product projection; schema v3 remains a transition endpoint whose existing responses retain their schema and meaning but rejects `browser_overview` without downgrade. Schema v2 remains historical and is rejected with typed `unsupported_schema`. The source App emits v4 only and never falls back to v3 or v1.

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

## Installed evidence

Generation 12 from the previously verified source head remains the active installed candidate; this newer schema-v4 source and App are not installed. The last verified daemon state exposes frontend schema v3 over the owner-private socket, uses SQLite v6, is healthy and quiescent ReadyReportOnly, has no armed generation or enforcement epoch, and retains `candidate_ready_report_only` rollback authority to generation 9. The last verified lease reports the prior generation and SQLite backup present with `rollback_available: true`; it has not been accepted. No daemon lifecycle or mode mutation occurred during this source-only work.

The rollback lease was exercised rather than inspected. Generation 10 migrated the active copy to v6, passed installed v3 reads/mutations and daemon restart, then `rollback-candidate` restored generation 9 and the SQLite-v5 snapshot. The exact generation-9 CLI/daemon reopened that database and returned healthy, quiescent ReadyReportOnly with exact PID/generation/binary/permission agreement. Candidate v6 database state was preserved separately as failed-generation evidence.

The first candidate reinstall exposed an acceptance-only race: `restart-report-only` required two separate quiescent reads and could repeatedly collide with periodic scan start. That generation was never accepted and was rolled back. Commit `9d9d765` allows exact healthy report-only restart during an observation-only scan while still refusing cleanup, arm or enforcement authority. After exact-head CI passed, the candidate was reinstalled as generation 12. A controlled readback observed `scan_in_progress: true`, invoked the fixed command, and replaced PID 7399 with PID 8047; the replacement returned healthy, quiescent ReadyReportOnly with the lease intact.

The current ad-hoc-signed App is installed byte-for-byte from the final local bundle and runs as the only Unlinger client. Installed live-socket tests passed 6/6 before and after daemon replacement, and the UI bundle passed the same installed 6/6 gate. A SwiftUI `MenuBarExtra` shell rendered blank under the prior macOS/Thaw menu host and was retired: direct AppKit entry now owns one process-lifetime delegate, status item, popover and reusable ordinary window while the SwiftUI state/router remains canonical. Runtime introspection confirms the direct `UnlingerAppDelegate`; the owner exercised popover Back and ordinary-window routing successfully.

The later missing-icon symptom is an external menu-host compatibility failure rather than a missing Unlinger status item or missing Thaw permission. The AppKit item exists with a live status window and Accessibility child. The owner-observed macOS settings have both Thaw Screen Recording and Accessibility enabled, and the current official feed has no build newer than the installed Thaw 2.0.1-rc.1 build 54. An exact stable-build-53 rollback reproduced the failure. Thaw's logs and matching source show macOS 26 returning hosted Control Center windows without a usable `sourcePID`; on this single-display setup the first generic `Item-0` can therefore remain unresolved and parked offscreen. A status-item autosave name, preferred-position defaults and an exact Command-drag did not establish a visible item, so none is retained as an Unlinger workaround.

The packaged Unlinger App now remains a regular foreground/Dock app while retaining its status item. Its Dock/window path is therefore independent of Thaw discovery: a clean installed launch produced the ordinary window, closing/reopening kept the App alive, and the same shared router remained available. The rebuilt executable contains only `/usr/lib/swift` and `@loader_path` rpaths and no source-volume resource fallback; launching it from `/` no longer produced the unrelated removable-volume access prompt. Thaw build 54 was restored and restarted after diagnosis. The Thaw-hosted menu icon itself remains unresolved and must not be reported as fixed.

## Safety and field truth

Automatic process eligibility remains limited to controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34`, plus every existing identity, isolation, durable abandonment, stability and protection gate. Agent-browser, Playwright and Puppeteer are recognized/classified families; only the exact controllerless CfT point is automatically eligible. Controller-bearing, headed/attached, standard/shared profile, wrong/mixed/unknown version and incomplete identity stay protected.

Automatic artifact cleanup remains limited to one exact admitted `DevToolsActivePort` file after the process tree and both revival checks are clear. Two P2 residuals remain: a crash after canonical-to-quarantine rename can strand the exact quarantine entry, and the final path revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU.

Historical generation-9 evidence proves one owner-approved managed full-timing Playwright-style CfT process/artifact/restart point: eight exact members, nine signal actions, zero survivors, both revival checks, one exact DAP removal, fresh-epoch restart without journal duplication, ordinary-Chrome preservation, and final stable report-only containment. It does not prove ambient eligibility, multi-day zero-false-positive dogfood, broader families/versions, or current-candidate enforcement.

## Verification truth

The current BGX-2 source gate passed locally on 2026-09-02:

- `cargo fmt --all -- --check`, strict workspace clippy and the release workspace build passed;
- `cargo test --workspace` passed 287 tests; the two owner-only live CfT tests remained ignored;
- source-only doctor inspected 451/451 listed processes with zero unreadable, argument-unavailable or descriptor-unavailable entries, 18 executable-identity-unavailable entries, `healthy: true` and no errors;
- dry-run inspected 450/450 listed processes with the same complete readable/argument/descriptor coverage, 18 executable-identity-unavailable entries and three `PROTECTED` automation sessions; no eligible incident or signal action was produced;
- `swift test` passed 73 tests in 15 suites, including strict v4 fixture decoding, daemon-phase preservation, typed compatibility/unknown copy, direct settlement projection, single-overview refresh, stale-snapshot containment, all seven product phases, bilingual VoiceOver copy, direct AppKit hosts and existing transport/mutation/notification contracts;
- the release App bundle assembled from its internal scratch path, packaged explicit v4 and v3 fixture directories, validated localizations, passed ad-hoc signature verification, and rejected source-volume resource fallback and removable-volume loader paths;
- the isolated smoke passed seven live-socket suite tests, including the v4 atomic browser overview, restarted the source daemon over the same private temporary SQLite database, then passed the same seven tests again. It verified effective report-only mode, durable receipt replay and owner-private temp/database/socket/lock modes, and removed only its owned temporary root; and
- no new rendered QA was required because BGX-2 changes protocol/state ownership without changing the previously rendered 360-point layouts or localized copy. Prior BGX-1 English/Simplified Chinese and AX observations remain historical UI evidence, not a fresh BGX-2 runtime proof.

BGX-2 implementation head `19c58e5` passed private GitHub exact-head macOS `backend` run `33608082841`, including formatting, strict clippy, workspace tests, release workspace build, native frontend tests and native frontend bundle. BGX-1 documentation-closure head `8c9e47e` remains separately verified by run `33597135228`.

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

Owner-only `cft_fieldlab` and `managed_cft_fieldlab` remained ignored and were not run. The installed lane remained untouched and report-only throughout; no signal authority was created. Source validation used only read-only host snapshots plus isolated temporary database/socket state.

## Open gates

- multi-day report-only dogfood and an ambient real eligible incident;
- owner acceptance of the installed Dock/window fallback and packaged notification behavior;
- any later owner-approved installation and acceptance of the browser-first App payload;
- Thaw/macOS hosted-menu compatibility for the still-unresolved status item; this external presentation gap does not authorize further private-default or identity workarounds in Unlinger;
- separately owner-authorized narrow enforcement acceptance for a future candidate;
- resolution or explicit product acceptance of both artifact P2 residuals;
- broader family/version/controller field evidence, chaos, sleep/wake and sustained-pressure evidence;
- Intel/universal build evidence;
- Developer ID signing, notarization, packaging/update/rollback and distribution;
- public-safe repository/license/rights decision, public alpha and public release.

See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md), [`SUPPORT.md`](SUPPORT.md), and [`support-matrix.v1.json`](support-matrix.v1.json) for the exact claim and support vocabulary.
