# UnlingerApp

Native macOS menu-bar frontend for Unlinger. It is a thin, local-only SwiftUI projection over frontend schema v3; contract authority lives in [`Contract/`](Contract/) and implementation boundaries in [`FRONTEND_BOUNDARY.md`](FRONTEND_BOUNDARY.md).

The App owns no classification, cleanup policy, signal authorization, daemon installation, daemon mode, or service lifecycle. It never falls back to schema v1. A timed-out or untrusted mutation response is reconciled from a durable pre-send journal and is never automatically resent.

## Current behavior

- strict v3 status/history/roster/detail/diagnostics DTOs, including exact readiness and observation freshness;
- capability-gated pause/resume/retry/protect/unprotect with namespace-aware durable receipts;
- one global unresolved-mutation lock, crash/restart status-only reconciliation, and authority-loss truth;
- single-flight/coalesced refreshes, polling-session generations, stale roster retention, and typed incident-detail failures;
- bilingual menu, detail, Settings/About and explicit “Quit Unlinger App” semantics—the daemon continues unchanged;
- local notifications with `off`, `attention` (default), and `attention_and_reclaims`; first trusted refresh baselines retained events, suppressed events are still marked seen, and a mode change never replays backlog;
- duplicate-avoidance notification ledger: durable claim before one schedule attempt, stable request IDs, no sound, foreground quiet, and public-safe click routing through a compact shared-router window;
- launch-at-login controls only this menu-bar client via `SMAppService.mainApp`. It never manages the daemon.

Notification delivery is a best-effort local projection over bounded status/history refreshes, not a gap-free event feed. The App reads current OS authorization every time; a denied prompt does not affect daemon behavior and is not repeatedly requested.

## Layout

- `Sources/UnlingerKit/IPC` — strict v3 envelope/DTOs and single-attempt cancellable Unix-socket transport;
- `Sources/UnlingerKit/Persistence` — owner-private `0700` directory / `0600` crash-durable atomic files;
- `Sources/UnlingerKit/State` — coalesced polling, projection, durable mutation reconciliation;
- `Sources/UnlingerKit/Notifications` — modes, ledger, coordinator and system scheduler;
- `Sources/UnlingerKit/Navigation` — shared notification/menu routing;
- `Sources/UnlingerKit/Settings` — preferences and menu-client login item;
- `Sources/UnlingerKit/UI` — popover, roster, history, detail, diagnostics and settings surfaces;
- `Sources/UnlingerKit/Copy` — English and Simplified Chinese copy;
- `Sources/UnlingerApp` — `@main`, packaged-live versus fixture/debug wiring;
- `Tests/UnlingerAppTests` — fixture, transport, mutation, concurrency, detail, notification, settings and opt-in live-socket coverage.

## Build and verify

```bash
swift build
swift test
scripts/bundle.sh
```

`scripts/bundle.sh` builds `build/Unlinger.app`, copies active v3 fixtures only, verifies both localizations and Info.plist, then applies a private ad-hoc signature. That is not Developer ID signing or notarization.

The repeatable pre-v0.1 integration gate owns a unique temporary database/socket/lock, remains report-only, runs the live Swift suite before and after daemon restart, checks private file modes and absence of IP listeners, and deletes only its own temp root:

```bash
scripts/pre-v0.1-smoke.sh
```

For manual source-only UI work, `scripts/demo-window.sh` runs an isolated report-only daemon. `UNLINGER_WINDOW=1` makes the debug App a regular Dock app; packaged menu-bar mode stays an accessory app except for its compact notification destination window.

## Installed boundary

Installed generation 9 remains v1-only, report-only and unarmed until the owner-authorized installed runbook begins. Service source now retains generation 9's manifest/plist/v5 database through explicit candidate accept/rollback and provides an exact report-only restart. The App may enter the installed lane only through [`../../docs/INSTALLED_DOGFOOD.md`](../../docs/INSTALLED_DOGFOOD.md), including a real generation-9 rollback/open before the candidate is reinstalled for dogfood.

Until that runbook passes, the strongest claim remains **pre-v0.1 source candidate — isolated report-only verified**. It is not an ambient-enforcement acceptance or a public release.
