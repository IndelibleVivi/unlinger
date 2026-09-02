# UnlingerApp

Native macOS menu-bar and Dock frontend for Unlinger. It is a thin, local-only SwiftUI projection over frontend schema v4; contract authority lives in [`Contract/`](Contract/) and implementation boundaries in [`FRONTEND_BOUNDARY.md`](FRONTEND_BOUNDARY.md).

The App owns no classification, cleanup policy, signal authorization, daemon installation, daemon mode, or service lifecycle. It never falls back to schema v1. A timed-out or untrusted mutation response is reconciled from a durable pre-send journal and is never automatically resent.

## Current behavior

- strict schema-v4 status/history/browser-overview/detail/diagnostics DTOs, including exact readiness and observation freshness, with no silent v3 or v1 fallback;
- one atomic daemon-owned `BrowserOverviewSnapshot` containing the authoritative product phase, current sessions, typed compatibility/coverage, saved protections, recent settlement and a rule-generated support catalog;
- one pure `BrowserOverviewMapper` that only selects localized copy and display shapes from that snapshot; it does not rescan evidence, join history or recompute product state;
- a browser-first overview, current-session rows, typed coverage explanations, saved protections, exact recent settlement, history and browser-context detail; the old process-tree status/roster presentation has been retired;
- capability-gated pause/resume/retry/protect/unprotect with namespace-aware durable receipts;
- one global unresolved-mutation lock, crash/restart status-only reconciliation, and authority-loss truth;
- single-flight/coalesced refreshes, polling-session generations, stale snapshot retention, and typed incident-detail failures;
- bilingual browser copy, matching formatter locale, VoiceOver state/reason/mode/freshness labels, Settings/About and explicit “Quit Unlinger App” semantics—the daemon continues unchanged;
- a direct AppKit `@main` whose strong process-lifetime delegate owns the status item/popover and reusable ordinary window independently of any SwiftUI scene or window lifetime; the App remains a regular Dock app so the window route is available even when a third-party menu host cannot resolve the status item; both hosts project the same SwiftUI state/router, popover detail has an explicit Back control, and it can open the current route in the window;
- local notifications with `off`, `attention` (default), and `attention_and_reclaims`; first trusted refresh baselines retained events, suppressed events are still marked seen, and a mode change never replays backlog;
- duplicate-avoidance notification ledger: durable claim before one schedule attempt, stable request IDs, no sound, foreground quiet, and public-safe click routing through the reusable shared-router window;
- launch-at-login controls only this menu-bar client via `SMAppService.mainApp`. It never manages the daemon.

Notification delivery is a best-effort local projection over bounded status/history refreshes, not a gap-free event feed. The App reads current OS authorization every time; a denied prompt does not affect daemon behavior and is not repeatedly requested.

## Layout

- `Sources/UnlingerKit/IPC` — strict v4 envelope/DTOs and single-attempt cancellable Unix-socket transport;
- `Sources/UnlingerKit/Persistence` — owner-private `0700` directory / `0600` crash-durable atomic files;
- `Sources/UnlingerKit/State` — coalesced polling, presentation-only browser mapping, stale-snapshot handling, durable mutation reconciliation;
- `Sources/UnlingerKit/Notifications` — modes, ledger, coordinator and system scheduler;
- `Sources/UnlingerKit/Navigation` — shared notification/menu routing;
- `Sources/UnlingerKit/Settings` — preferences and menu-client login item;
- `Sources/UnlingerKit/UI` — AppKit popover/window hosts plus one shared browser-first SwiftUI home, history, detail, diagnostics and settings surfaces;
- `Sources/UnlingerKit/Copy` — English and Simplified Chinese copy;
- `Sources/UnlingerApp` — `@main`, packaged-live versus fixture/debug wiring;
- `Tests/UnlingerAppTests` — fixture, transport, mutation, concurrency, detail, notification, settings and opt-in live-socket coverage.

## Build and verify

```bash
swift build
swift test
scripts/bundle.sh
```

`scripts/bundle.sh` builds `build/Unlinger.app` in a temporary internal SwiftPM scratch path, copies the active v4 fixtures and transitional v3 fixtures, rejects packaged resource fallbacks or loader paths that still point at a removable volume, verifies both localizations and Info.plist, then applies a private ad-hoc signature. That is not Developer ID signing or notarization.

The repeatable pre-v0.1 integration gate owns a unique temporary database/socket/lock, remains report-only, runs the live Swift suite before and after daemon restart, checks private file modes and absence of IP listeners, and deletes only its own temp root:

```bash
scripts/pre-v0.1-smoke.sh
```

For manual source-only UI work, `scripts/demo-window.sh` runs an isolated report-only daemon. `UNLINGER_WINDOW=1` presents the ordinary window immediately. The packaged App remains regular and retains its Dock entry alongside the status item; Dock reopen, the popover's explicit window action, and notification routes all show the same reusable AppKit-owned ordinary window without changing daemon state.

Deterministic product-state QA can instead use `UNLINGER_FIXTURE=browser-clear|browser-active|browser-verifying|browser-confirmed-report-only|browser-reclaiming|browser-protected-unsupported|browser-attention|browser-recent-settlement` together with `UNLINGER_WINDOW=1`. These scenarios return typed v4 browser snapshots and never contact or mutate the installed service.

## Installed boundary

Accepted generation 15 and the matching schema-v4 ad-hoc-signed App are installed for private dogfood. The App consumes v4 and the daemon retains v3 only as a transition endpoint. Generation 15 is healthy `ReadyEnforce` under the `0.3.0` process-only policy after exact-head CI, real candidate-A rollback to generation 13, candidate-B reinstall/restart/accept, full-timing field acceptance, final containment and a stable post-arm sweep. [`../../docs/INSTALLED_DOGFOOD.md`](../../docs/INSTALLED_DOGFOOD.md) records the repeatable transactional lane.

The strongest claim is **private enforcement candidate for the exact admitted point**. Ambient process-only enforcement is active and no lease is pending, but one controlled run plus one stable sweep is not private-v0.1 or multi-day acceptance, a signed distribution candidate or a public release.
