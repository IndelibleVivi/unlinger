# UnlingerApp

Native macOS menu-bar frontend for Unlinger — a thin SwiftUI client over
frontend schema v2. Contract authority lives in [`Contract/`](Contract/);
implementation boundaries in [`FRONTEND_BOUNDARY.md`](FRONTEND_BOUNDARY.md).

The app projects backend truth only: it owns no classification, cleanup
policy, signal authorization, or service lifecycle control. Actions are gated
by backend capabilities; a timed-out mutation is never resent automatically,
and its uncertainty banner does not provide a shortcut around fresh capability
readback.

## Layout

- `Sources/UnlingerKit/IPC` — v2 envelope/DTOs, Unix-socket client (single
  attempt, 15 s I/O bound, 64 KiB/4 MiB limits), fixture client reading the
  canonical `Contract/v2` JSON in place.
- `Sources/UnlingerKit/State` — polling store, status→UI mapping, and the
  mutation state machine (confirmed / delivery-uncertain + readback).
- `Sources/UnlingerKit/UI` — popover, status/roster/attention/history/detail
  views. The roster (`incidents` command) is a read-only "watching right now"
  panel — observability only, no actions derive from it.
- `Sources/UnlingerKit/Copy` — bilingual (en / zh-Hans) `Localizable.strings`;
  copy states observable facts only and never renders raw backend identifiers.
- `Sources/UnlingerKit/Assets` — menu-bar template icons generated from
  `AssetsSource/` via `scripts/make-menubar-icon.swift`.
- `Sources/UnlingerApp` — the `@main` entry.
- `Tests/UnlingerAppTests` — fixture decoding, status mapping, mutation flow,
  localization, and an opt-in live socket smoke suite.

## Build and test

```bash
swift build
swift test
scripts/bundle.sh   # builds release and assembles build/Unlinger.app (ad-hoc signed)
```

## Live socket smoke

The installed generation 9 is v1-only and must stay untouched. For a
real-machine demo, `scripts/demo-window.sh` brings up an isolated source
report-only daemon (own temp database/socket/lock) and opens the windowed app
against it; `scripts/demo-window.sh stop` tears both down. To exercise the
socket from tests instead, run the daemon per `FRONTEND_BOUNDARY.md`, then:

```bash
UNLINGER_LIVE_SOCKET=/path/to/isolated/unlingerd.sock swift test --filter LiveSocketTests
UNLINGER_SOCKET_PATH=/path/to/isolated/unlingerd.sock build/Unlinger.app/Contents/MacOS/UnlingerApp
```

`UNLINGER_WINDOW=1` (windowed debug mode) additionally runs as a regular Dock
app, so a closed window comes back with a Dock-icon click. The shipped
menu-bar mode stays an accessory agent.

## Not yet done

- UserNotifications (private v0 categories: successful reclaim,
  daemon/service needs attention).
- Any integration with the installed generation — that requires a future
  v2-capable generation install, separately authorized.
