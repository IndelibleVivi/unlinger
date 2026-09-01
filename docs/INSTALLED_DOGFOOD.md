# Installed report-only dogfood runbook

This runbook advances only acceptance level 3. It installs the exact source candidate and packaged App for private report-only observation. It never arms enforcement, runs a signal harness, or accepts the candidate on the owner's behalf.

## Preconditions

- The source commit is pushed and exact-head macOS CI is green.
- `cargo test --workspace`, the Swift suite, release build and bundle gate pass at that commit.
- The current installed service is healthy, quiescent, report-only and unarmed.
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

Install the verified ad-hoc bundle at the owner-local application target, preserving any prior bundle as a recoverable sibling until the new App has launched. The packaged executable and resource lookup must contain no source/build-volume path, and a clean launch must not request removable-volume access. Run the opt-in `LiveSocketTests` against the installed service socket, launch the packaged App, and verify that status/history/roster/detail/diagnostics and ordinary mutation reconciliation use schema v3. The regular Dock/window route must remain available independently of status-item discovery by any external menu host. Restart the daemon with:

```bash
<candidate-cli-path> service restart-report-only --json
```

Then recreate the App/client state and repeat the v3 read/reconciliation checks. Notification authorization and final visual behavior remain owner-observed macOS UI gates.

## Mandatory rollback proof

Before retaining the candidate for dogfood, exercise the lease rather than merely inspecting its files:

```bash
<candidate-cli-path> service rollback-candidate --json
<recorded-prior-cli-path> service status --json
```

The restored status must identify the prior generation, return healthy and quiescent `ReadyReportOnly`, remain unarmed, and report the prior SQLite schema. This exact prior CLI/daemon open is the required old-binary proof; a generic SQLite reader is not a substitute.

Reinstall the same exact-head candidate, repeat installed v3 and restart checks, and leave the second candidate at `candidate_ready_report_only` with rollback retained throughout initial dogfood.

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

Passing this runbook permits only **pre-v0.1 installed report-only candidate**. Multi-day dogfood, an ambient eligible incident, any current-candidate enforcement run, the two artifact P2 decisions, universal binaries, Developer ID signing/notarization, distribution and public release remain separate gates.
