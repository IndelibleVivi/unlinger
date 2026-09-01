# Current state

**Updated:** 2026-09-01

**Programme:** Unlinger 0.1

**Source:** private pre-v0.1 schema-v3 candidate; isolated report-only verified; acceptance rollback lease implemented locally

**Remote:** private origin `main`; the prior source tranche is green, while the rollback-lease diff is local and awaiting push/exact-head CI

**Installed runtime:** generation 9, v1-only, last verified healthy at a stable report-only floor

**Activation:** unarmed; owner authorized the installed report-only runbook, but no installation has occurred at this source checkpoint

**Highest claim currently permitted:** **pre-v0.1 source candidate — isolated report-only verified** (acceptance level 2)

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
- bounded local notifications: `off`, default `attention`, or `attention_and_reclaims`; first trusted refresh baselines retained tokens, suppressed events remain seen, and durable claim precedes one schedule attempt.

Notifications are best-effort local projections over bounded polling. They have no sound, are quiet in foreground, use public-safe route data, and never treat an App mutation response fault as a backend cleanup event. Runtime remains local-only with no account, telemetry, cloud sync or normal-operation network behavior.

The service source now extends its durable install transaction through `CandidateReadyReportOnly`, `AcceptanceInProgress` and `Accepted`. A ready candidate retains the prior manifest, report-only plist and SQLite snapshot; install, uninstall and mode changes remain blocked until explicit `accept-candidate` or `rollback-candidate`. `restart-report-only` restarts only the exact active generation at the report-only floor and preserves a pending lease. Focused CLI tests cover phase disposition, rollback-material retention, invalid backup projection, install-time enforce rejection and command parsing.

## Installed gate

Installed generation 9 uses schema-v1 IPC and a SQLite-v5-capable binary. Source v3 migrates the active database to v6, which generation 9 cannot reopen directly. The source rollback lease now preserves a SQLite-consistent v5 snapshot and exact prior generation through the post-ready App acceptance window.

Installed v3 integration remains unproved until the owner-authorized [`INSTALLED_DOGFOOD.md`](INSTALLED_DOGFOOD.md) lane passes exact-head CI, installs without arm, exercises the lease, proves the generation-9 CLI/daemon opens the restored v5 database and returns healthy ReadyReportOnly, then reinstalls the same candidate and completes packaged App/daemon restart reconciliation. The second rollback lease remains pending during initial dogfood.

## Safety and field truth

Automatic process eligibility remains limited to controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34`, plus every existing identity, isolation, durable abandonment, stability and protection gate. Agent-browser, Playwright and Puppeteer are recognized/classified families; only the exact controllerless CfT point is automatically eligible. Controller-bearing, headed/attached, standard/shared profile, wrong/mixed/unknown version and incomplete identity stay protected.

Automatic artifact cleanup remains limited to one exact admitted `DevToolsActivePort` file after the process tree and both revival checks are clear. Two P2 residuals remain: a crash after canonical-to-quarantine rename can strand the exact quarantine entry, and the final path revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU.

Historical generation-9 evidence proves one owner-approved managed full-timing Playwright-style CfT process/artifact/restart point: eight exact members, nine signal actions, zero survivors, both revival checks, one exact DAP removal, fresh-epoch restart without journal duplication, ordinary-Chrome preservation, and final stable report-only containment. It does not prove ambient eligibility, multi-day zero-false-positive dogfood, broader families/versions, or current-candidate enforcement.

## Verification truth

The level-2 source gate passed on 2026-09-01:

- `cargo fmt --all -- --check`, strict workspace clippy and the release workspace build passed;
- `cargo test --workspace` passed 272 tests; the two owner-only live CfT tests remained ignored;
- source-only doctor inspected 408/408 listed processes with zero unreadable, argument-unavailable or descriptor-unavailable entries, 17 executable-identity-unavailable entries, `healthy: true` and no errors;
- dry-run inspected the same 408/408 snapshot with the same complete readable/argument/descriptor coverage, 17 executable-identity-unavailable entries and no incident;
- `swift test` passed 62 tests in 11 suites;
- the release App bundle assembled, its active schema-v3 fixtures and localizations validated, and its ad-hoc signature verified;
- the isolated smoke passed six live-socket tests, restarted the source daemon over the same private temporary SQLite database, then passed the same six tests again. It verified effective report-only mode, durable receipt replay and owner-private temp/database/socket/lock modes, and removed only its owned temporary root; and
- the tracked candidate/diff scan found no Faye-specific absolute path, attached-audit filename or identifier, secret-shaped addition, whitespace error or generated build payload; generic `/Users/example` and `/Users/private` strings remain only in synthetic fixtures and redaction/path tests; and
- a separate read-only installed-status check found generation 9 healthy, ready, v1/v5, quiescent, report-only and unarmed with exact PID/generation/binary/permission agreement. It did not reload, migrate or otherwise mutate the installed service.

The private GitHub `backend` workflow also passed at the pushed exact head on macOS: formatting, strict clippy, workspace tests, release workspace build, native frontend tests and native frontend bundle.

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

Owner-only `cft_fieldlab` and `managed_cft_fieldlab` remained ignored and were not run. No source-v3 binary opened the active generation-9 database.

## Open gates

- exact-head acceptance-lease CI plus real generation-9 database rollback/open proof;
- installed v3 daemon/App integration and restart reconciliation without arm;
- multi-day report-only dogfood and an ambient real eligible incident;
- separately owner-authorized narrow enforcement acceptance for a future candidate;
- resolution or explicit product acceptance of both artifact P2 residuals;
- broader family/version/controller field evidence, chaos, sleep/wake and sustained-pressure evidence;
- Intel/universal build evidence;
- Developer ID signing, notarization, packaging/update/rollback and distribution;
- public-safe repository/license/rights decision, public alpha and public release.

See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md), [`SUPPORT.md`](SUPPORT.md), and [`support-matrix.v1.json`](support-matrix.v1.json) for the exact claim and support vocabulary.
