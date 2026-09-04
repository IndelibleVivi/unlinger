# Installed report-only dogfood runbook

This runbook advances only acceptance level 3. It installs the exact source candidate and packaged App for private report-only observation and proves that candidate's own rollback lease. It never arms enforcement, runs a signal harness, or accepts the candidate on the owner's behalf. The 2026-09-04 generation-16→15→17 execution completed this level-3 lane for schema v5/SQLite v7; the owner's later acceptance and arm were separate explicit actions.

## Preconditions

- The source commit is pushed and exact-head macOS CI is green.
- `cargo test --workspace`, the Swift suite, release build and bundle gate pass at that commit.
- The current installed service is healthy and quiescent. It may already be enforce-mode only when the owner has explicitly authorized replacement; candidate install must still use the service transaction, which first disarms/drains the exact prior instance and records a report-only rollback floor.
- Record the current `active_generation`, `cli_path`, database schema and service-status output before mutation.
- Build candidate binaries with `cargo build --release --workspace`; build the App with `apps/UnlingerApp/scripts/bundle.sh`.

## Candidate install

Run the candidate CLI itself so the copied CLI and daemon come from one release build:

```bash
target/release/unlinger service install \
  --daemon target/release/unlingerd \
  --mode report-only \
  --json
```

A successful install must report all of the following at the same readback:

- exact launchd, IPC, generation and executable identity;
- `ReadyReportOnly`, requested/effective `report_only`, no armed generation and no enforcement epoch;
- no scan or cleanup in progress;
- acceptance phase `candidate_ready_report_only`;
- `rollback_available: true`, with the prior generation named and the SQLite backup present.

While that lease is pending, ordinary install, uninstall and `set-mode` remain blocked. `service restart-report-only` is the only acceptance restart command; it keeps the lease and re-establishes an exact fresh report-only instance. Its preflight requires the exact selected candidate, a report-only active manifest and executable rollback material, so it can recover an unloaded, PID-less, scanning or terminal-failed candidate instead of requiring that the broken instance already be healthy. Acceptance remains separate and requires a healthy, quiescent exact ReadyReportOnly projection with no arm/enforcement authority.

## Installed App and restart checks

Install the verified ad-hoc bundle at the owner-local application target, preserving any prior bundle as a recoverable sibling until the new App has launched. The packaged executable and resource lookup must contain no source/build-volume path, and a clean launch must not request removable-volume access. Run the opt-in `LiveSocketTests` against the installed service socket, launch the packaged App, and verify that status/history/roster/detail/diagnostics and ordinary mutation reconciliation use the candidate's current frontend schema. For the current line this is schema v5, with v4 and v3 retained only as compatibility endpoints; the App must never silently downgrade. The regular Dock/window route must remain available independently of status-item discovery by any external menu host. Restart the daemon with:

```bash
<candidate-cli-path> service restart-report-only --json
```

Then recreate the App/client state and repeat the current-schema read/reconciliation checks. Notification authorization and final visual behavior remain owner-observed macOS UI gates.

## Mandatory rollback proof

Before retaining the candidate for dogfood, exercise the lease rather than merely inspecting its files:

```bash
<candidate-cli-path> service rollback-candidate --json
<recorded-prior-cli-path> service status --json
```

The restored status must identify the prior generation, return healthy and quiescent `ReadyReportOnly`, remain unarmed, and report the prior SQLite schema. This exact prior CLI/daemon open is the required old-binary proof; a generic SQLite reader is not a substitute. If the prior daemon supports only an older frontend schema, the preserved prior App bundle is the rollback client; the current App failing closed as incompatible is expected and must not be bypassed with a downgrade.

Reinstall the same exact-head candidate as a fresh generation and fresh acceptance transaction, repeat current-schema App and report-only restart checks, and leave the second candidate at `candidate_ready_report_only` until the owner separately accepts it. A prior candidate's rollback proof does not satisfy this fresh transaction.

## Acceptance and rollback

During dogfood:

```bash
<active-candidate-cli-path> service status --json
<active-candidate-cli-path> service rollback-candidate --json
```

Only after the owner accepts the observed candidate may the retained prior generation and database backup be retired:

```bash
<active-candidate-cli-path> service accept-candidate --json
```

`accept-candidate` is the durable linearization point. A crash before durable `accepted` restores the prior report-only generation; a crash after it finishes cleanup and must not reinterpret the candidate as rollback-eligible.

## Claim boundary

Passing this runbook permits only **pre-v0.1 installed report-only candidate**. It does not by itself authorize process enforcement or artifact cleanup. A later process-only field lane is the ordinary prerequisite for a Level-4 claim. On 2026-09-04 the owner separately directed generation 17 into live private enforcement after this Level-3 transaction without running the candidate-specific signal harness; that exceptional activation is recorded as active dogfood, not as generation-17 or CfT-152 Level-4 evidence. Multi-day dogfood, an ambient eligible incident, artifact re-enable, universal binaries, Developer ID signing/notarization, distribution and public release remain separate gates.
