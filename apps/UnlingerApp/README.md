# UnlingerApp

Native macOS menu-bar and Dock frontend for Unlinger. Current source is a thin, local-only SwiftUI projection over frontend schema v5; contract authority lives in [`Contract/`](Contract/) and implementation boundaries in [`FRONTEND_BOUNDARY.md`](FRONTEND_BOUNDARY.md).

The App owns no classification, cleanup policy, signal authorization, daemon installation, daemon mode, or service lifecycle. It never falls back to schema v1. A timed-out or untrusted mutation response is reconciled from a durable pre-send journal and is never automatically resent.

## Current behavior

- strict schema-v5 status/history/browser-overview/detail/diagnostics DTOs, including exact readiness, observation freshness, cleanup impact, typed storage residue and server-owned observation spans, with no silent v4, v3 or v1 fallback;
- one atomic daemon-owned `BrowserOverviewSnapshot` containing the authoritative product phase, current sessions, typed compatibility/coverage, saved protections, exact recent settlement, durable cleanup impact, typed storage-residue status/eligibility and a rule-generated support catalog;
- one pure `BrowserOverviewMapper` that only selects localized copy and display shapes from that snapshot; it does not rescan evidence, join history or recompute product state;
- a separate `BrowserHistoryMapper` that exposes only terminal cleanup outcomes in the history index and consumes daemon-owned observation spans in detail; observation noise no longer becomes a repetitive history row;
- a browser-first overview, product/version-aware current-session rows, typed coverage explanations, saved protections, exact recent settlement, durable lifetime cleanup impact, daemon-owned Chrome code-sign clone residue/eligibility, cleanup-centric history and readable browser-context detail with named safety checks; the App displays enforce/report-only meaning but owns no storage deletion command; the old process-tree status/roster presentation has been retired;
- capability-gated pause/resume/retry/protect/unprotect with namespace-aware durable receipts;
- one global unresolved-mutation lock, crash/restart status-only reconciliation, and authority-loss truth;
- single-flight/coalesced refreshes, polling-session generations, stale snapshot retention, and typed incident-detail failures;
- bilingual browser copy, matching formatter locale, VoiceOver state/reason/mode/freshness labels, Settings/About and explicit “Quit Unlinger App” semantics—the daemon continues unchanged;
- a direct AppKit `@main` whose strong process-lifetime delegate owns the status item/popover and reusable ordinary window independently of any SwiftUI scene or window lifetime; the square status item uses the system `circle.dashed` symbol and stable autosave identity `app.unlinger.menu.primary`, while the App remains a regular Dock app so the window route is available even when a third-party menu host cannot resolve the status item; both hosts share one `AppState` and one `340 × 420` content size but own independent `AppRouter` paths, each host creates its SwiftUI root only while presented and releases it on close, and opening a current popover route in the ordinary window copies it once before clearing the hidden popover path;
- stored browser overview/history presentation rebuilt only when its daemon-owned source facts change, equal refreshes and equal settings writes left unpublished, stable localization resources reused until the chosen language changes, and language/pause/notification popups backed by an idempotently configured native `NSPopUpButton` rather than SwiftUI's popup item adaptor; stable-identity SwiftUI iteration and one outer Accessibility element per history link keep Accessibility traversal from triggering history regrouping or competing navigation writes inside view evaluation;
- local notifications with `off`, `attention` (default), and `attention_and_reclaims`; first trusted refresh baselines retained events, suppressed events are still marked seen, and a mode change never replays backlog;
- duplicate-avoidance notification ledger: durable claim before one schedule attempt, stable request IDs, no sound, foreground quiet, and public-safe click routing through the reusable ordinary-window router;
- launch-at-login controls only this menu-bar client via `SMAppService.mainApp`. It never manages the daemon.

Notification delivery is a best-effort local projection over bounded status/history refreshes, not a gap-free event feed. The App reads current OS authorization every time; a denied prompt does not affect daemon behavior and is not repeatedly requested.

On macOS 26, the system-level **System Settings → Menu Bar → Allow in the Menu Bar → Unlinger** switch must be on before AppKit can place the status item. Third-party organizers still own their section policy: with Thaw, put the stable Unlinger item in **Visible**. The current installed bundle was verified with Thaw 2.0.1-rc.1 on one built-in display; that bounded result does not claim every organizer, version, display or auto-hidden-menu-bar arrangement.

## Layout

- `Sources/UnlingerKit/IPC` — strict current-v5 plus compatibility-v4/v3 envelope/DTOs and single-attempt cancellable Unix-socket transport;
- `Sources/UnlingerKit/Persistence` — owner-private `0700` directory / `0600` crash-durable atomic files;
- `Sources/UnlingerKit/State` — coalesced polling, separate browser-snapshot and bounded-history presentation mapping, stale-snapshot handling, durable mutation reconciliation;
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

`scripts/bundle.sh` builds `build/Unlinger.app` in a temporary internal SwiftPM scratch path, copies the current v5 fixtures plus compatibility v4/v3 fixtures, rejects packaged resource fallbacks or loader paths that still point at a removable volume, verifies both localizations and Info.plist, then applies a private ad-hoc signature. That is not Developer ID signing or notarization.

The repeatable pre-v0.1 integration gate owns a unique temporary database/socket/lock, remains report-only, runs the live Swift suite before and after daemon restart, checks private file modes and absence of IP listeners, and deletes only its own temp root:

```bash
scripts/pre-v0.1-smoke.sh
```

The history/Accessibility regression gate is fixture-only and uses the real macOS Accessibility tree through a separately compiled, two-second-timeout `AXUIElement` probe. It requires an unlocked console session, builds and launches an exact child App directly on a 50-incident synthetic history route, holds one exact window element while repeatedly traversing its child tree, samples App RSS externally for two minutes, fails at 384 MiB RSS, more than 96 MiB retained growth, or four consecutive child-tree read misses, and signals only the processes it created. A successful read resets the bounded miss count:

```bash
scripts/accessibility-memory-smoke.sh
```

Use `UNLINGER_AX_SMOKE_SECONDS`, `UNLINGER_AX_RSS_LIMIT_MIB`, and `UNLINGER_AX_GROWTH_LIMIT_MIB` only when deliberately changing the duration or cutoff. A passing fixture gate is source regression evidence, not installed-App acceptance.

For manual source-only UI work, `scripts/demo-window.sh` (Python 3 required) builds and runs a foreground report-only demo. It owns unique temporary daemon state and an App home directory, launches the unbundled App without packaged notifications, and stops only its own children when the demo App quits or you press Ctrl-C in that terminal. The old `stop` subcommand and name-based process termination are retired. Closing just the window leaves the demo running. `UNLINGER_WINDOW=1` presents the ordinary window immediately. The packaged App remains regular and retains its Dock entry alongside the status item; Dock reopen, the popover's explicit window action, and notification routes all show the same reusable AppKit-owned ordinary window without changing daemon state.

Deterministic product-state QA can instead use `UNLINGER_FIXTURE=browser-clear|browser-active|browser-verifying|browser-confirmed-report-only|browser-reclaiming|browser-protected-unsupported|browser-attention|browser-recent-settlement|browser-impact-residue|browser-history-stress` together with `UNLINGER_WINDOW=1`. The dedicated impact/residue scenario renders both new cards with partial-history and logical-size caveats. Set `UNLINGER_FIXTURE_ROUTE=history` to open the isolated window directly on its history route. These scenarios return typed v5 browser snapshots and never contact or mutate the installed service.

## Installed boundary

The schema-v5 ad-hoc-signed App from memory-repair commit `c17e60f` is installed against generation 30 / SQLite v10. Its predecessor `628d822` had recurred at about 19 GB after earlier bounded acceptance. A system CPU-resource report captured that old App's main SwiftUI/AttributeGraph thread growing the process from about 42 MiB to 2.0 GiB in 138 seconds while repeatedly updating popup Accessibility attributed text; the owner then Force Quit that exact process. The installed repair removes the captured popup adaptor path and adds idempotence/lifecycle regressions. The displaced `628d822`, `e26297b` and `016ca58` bundles remain recoverable local siblings. [current-state](../../docs/current-state.md) owns exact current totals, runtime and deployment truth.

The repair passes 91 Swift tests, release bundling, a 460-probe fixture Accessibility/RSS lane with zero read failures and 22,208 KiB final RSS, plus a separate 900-sample real-daemon packaged guard with 98,000 KiB maximum and 18,112 KiB final RSS. After recoverable installation, its exact canonical process passed 2,400 one-second RSS samples with 22,192 KiB maximum and 13,552 KiB final RSS and remained running. That is bounded installed evidence; the exact intermittent trigger and multi-day acceptance remain separate. The status-item repair still uses the system symbol and one stable AppKit/Thaw identity. The installed App emits v5 only; the daemon retains v4 and v3 compatibility endpoints without downgrade. [`../../docs/INSTALLED_DOGFOOD.md`](../../docs/INSTALLED_DOGFOOD.md) records the repeatable transactional lane.

The strongest claim is **bounded installed memory-repair evidence plus an accepted private enforcement service baseline**. Generation 30 is currently healthy `ReadyEnforce` with no lease; one controlled historical process-cleanup run, short stable sweeps and one 40-minute App guard are not private-v0.1 or multi-day acceptance, a signed distribution candidate or a public release.
