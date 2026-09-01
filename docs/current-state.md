# Current state

**Updated:** 2026-09-01

**Programme:** Unlinger 0.1

**Source:** private pre-v0.1 schema-v3 candidate; direct-AppKit Dock fallback and source-volume-independent packaging verified

**Remote:** Dock/package-hardening implementation head `0cc3beb85d4b757bcee58c5556ece38897202aca` is on private origin `main`; exact-head `backend` run `33520850444` is green. This current-state reconciliation follows as documentation-only truth.

**Installed runtime:** generation 12, schema-v3/v6 capable, healthy at a stable report-only floor; acceptance lease retains generation 9/v5

**Activation:** unarmed; generation 12 and the packaged App are active for private report-only dogfood

**Highest claim currently permitted:** **pre-v0.1 installed report-only candidate** (acceptance level 3)

## Source truth

The backend source now uses SQLite schema v6 and frontend schema v3 while preserving schema-v1 CLI/service compatibility. Schema v2 remains as historical fixtures but is rejected with typed `unsupported_schema`; the App emits v3 only and never falls back to v1.

V3 ordinary mutations carry a public-safe namespace token plus canonical UUID. The daemon serializes lifecycle state with a `BEGIN IMMEDIATE` store transaction, replays an exact stored receipt before current lifecycle policy, re-evaluates authoritative durable facts through one shared public-action policy, applies state, advances a durable cleanup-policy revision and stores a typed `applied | no_change | rejected` receipt atomically. Same-ID canonical replay is idempotent; conflicting reuse fails; old-namespace missing requests cannot apply. Receipt pruning rotates namespace atomically and retains a minimum 14-day reconciliation window.

SQLite v6 also assigns stable opaque public event tokens, retains storage-recovery identity across clean restarts, and commits each observation cycle's history as one batch before publishing the roster. The public roster distinguishes `never_observed`, `scan_in_progress`, `current` and `stale_after_failure`, with optional time/token when no real snapshot exists. Last scan uses cycle completion rather than cycle start.

The native App under `apps/UnlingerApp` now has:

- strict schema-v3 DTO/envelope decoding and a narrow exact v1 incompatibility parser;
- phase-aware, single-attempt, cancellable socket I/O;
- an owner-private crash-durable pending-mutation journal written before connect/send;
- one global unresolved mutation lock and startup/status-only reconciliation without resend;
- coalesced refresh generations, polling-session isolation, stale roster retention and typed incident-detail failures;
- local-view diagnostics results with required `document_schema_version` and semantic-lossless JSON export;
- stable event/action identity, including artifact-only action groups;
- bilingual Settings/About/Quit surfaces, shared notification routing and menu-client-only `SMAppService.mainApp` launch at login;
- a direct AppKit `@main` that strongly retains the single delegate for the blocking App run loop, with AppKit-owned `NSStatusItem`/`NSPopover` and reusable ordinary-window lifecycles around one shared SwiftUI state/router; the packaged App remains a regular foreground/Dock app, closing the last ordinary window does not terminate it, and Dock reopen plus popover detail retain explicit window/Back/current-route controls;
- a release bundler that builds in an internal temporary SwiftPM scratch path, removes removable-volume toolchain rpaths, and rejects packaged executables that retain removable-volume resource fallbacks or loader paths;
- bounded local notifications: `off`, default `attention`, or `attention_and_reclaims`; first trusted refresh baselines retained tokens, suppressed events remain seen, and durable claim precedes one schedule attempt.

Notifications are best-effort local projections over bounded polling. They have no sound, are quiet in foreground, use public-safe route data, and never treat an App mutation response fault as a backend cleanup event. Runtime remains local-only with no account, telemetry, cloud sync or normal-operation network behavior.

The service source extends its durable install transaction through `CandidateReadyReportOnly`, `AcceptanceInProgress`, `RollbackInProgress` and `Accepted`. `DatabaseBackedUp` is a conservative restore boundary because a crash may occur after either candidate selection file is published but before `CandidateSelected` is durable. Rollback validates the immutable backup's recorded schema and the sealed prior daemon/CLI/manifest, persists replay intent before physical mutation, accepts only transaction-owned candidate/prior/mixed selection cuts, restores the same snapshot and republishes the prior report-only selection until convergence. Offline containment directly updates the compatible managed-lifecycle row and verifies that the database `user_version` is unchanged; it no longer opens a restored prior database through the current migrating `HistoryStore::open` path.

`restart-report-only` still requires exact candidate selection, a report-only manifest and executable rollback material, but it no longer requires a stopped, PID-less or terminal-failed candidate to already be healthy before replacement. Acceptance remains stricter: exact healthy, quiescent ReadyReportOnly plus report-only desired mode and valid rollback material. Service-command JSON now serializes a dedicated schema-v1 public projection that retains lifecycle/lease truth while excluding PID, instance ID, absolute paths and raw errors.

## Installed evidence

Generation 12 from the previously verified source head remains the active installed candidate; the newer rollback-hardening source is not installed. The daemon exposes frontend schema v3 over the owner-private socket, uses SQLite v6, is healthy and quiescent ReadyReportOnly, has no armed generation or enforcement epoch, and retains `candidate_ready_report_only` rollback authority to generation 9. The current lease reports the prior generation and SQLite backup present with `rollback_available: true`; it has not been accepted. No daemon lifecycle or mode mutation occurred during this follow-up.

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

The level-2 source gate passed on 2026-09-01:

- `cargo fmt --all -- --check`, strict workspace clippy and the release workspace build passed;
- `cargo test --workspace` passed 276 tests; the two owner-only live CfT tests remained ignored;
- source-only doctor inspected 452/452 listed processes with zero unreadable, argument-unavailable or descriptor-unavailable entries, 15 executable-identity-unavailable entries, `healthy: true` and no errors;
- dry-run inspected 451/451 listed processes with the same complete readable/argument/descriptor coverage, 15 executable-identity-unavailable entries and no incident;
- `swift test` passed 66 tests in 14 suites, including explicit one-level Back, current-route-preserving window presentation, actionable status-item popover and reusable AppKit-window ownership;
- the release App bundle assembled from its internal scratch path, its active schema-v3 fixtures and localizations validated, its ad-hoc signature verified, and both source-volume resource fallback and removable-volume loader-path checks passed;
- the isolated smoke passed six live-socket tests, restarted the source daemon over the same private temporary SQLite database, then passed the same six tests again. It verified effective report-only mode, durable receipt replay and owner-private temp/database/socket/lock modes, and removed only its owned temporary root; and
- the tracked candidate/diff scan found no Faye-specific absolute path, attached-audit filename or identifier, secret-shaped addition, whitespace error or generated build payload; generic `/Users/example` and `/Users/private` strings remain only in synthetic fixtures and redaction/path tests; and
- a separate read-only installed-status check found generation 9 healthy, ready, v1/v5, quiescent, report-only and unarmed with exact PID/generation/binary/permission agreement. It did not reload, migrate or otherwise mutate the installed service.

The private GitHub `backend` workflow passed at rollback-lease head `ea7c5bd` (run `33482121339`), scan-race fix head `9d9d765` (run `33483324021`), the prior installed-state docs head `68085ba` (run `33484218771`), AppKit menu/window implementation head `4eb70eb` (run `33500090371`), rollback-hardening/direct-AppKit lifetime head `188e65a` (run `33513920375`), and Dock/package-hardening implementation head `0cc3beb` (run `33520850444`) on macOS: formatting, strict clippy, workspace tests, release workspace build, native frontend tests and native frontend bundle. The first AppKit run `33499759687` correctly failed because two host tests constructed `NSStatusItem`/`NSWindow` before initializing `NSApplication`; `4eb70eb` fixed that test precondition rather than skipping the hosts.

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

Owner-only `cft_fieldlab` and `managed_cft_fieldlab` remained ignored and were not run. The installed lane remained report-only throughout; no signal authority was created. The source-v3 binary opened the active database only inside the acceptance lease, and the real rollback restored the v5 copy before generation 9 reopened it.

## Open gates

- multi-day report-only dogfood and an ambient real eligible incident;
- owner acceptance of the installed Dock/window fallback and packaged notification behavior;
- Thaw/macOS hosted-menu compatibility for the still-unresolved status item; this external presentation gap does not authorize further private-default or identity workarounds in Unlinger;
- separately owner-authorized narrow enforcement acceptance for a future candidate;
- resolution or explicit product acceptance of both artifact P2 residuals;
- broader family/version/controller field evidence, chaos, sleep/wake and sustained-pressure evidence;
- Intel/universal build evidence;
- Developer ID signing, notarization, packaging/update/rollback and distribution;
- public-safe repository/license/rights decision, public alpha and public release.

See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md), [`SUPPORT.md`](SUPPORT.md), and [`support-matrix.v1.json`](support-matrix.v1.json) for the exact claim and support vocabulary.
