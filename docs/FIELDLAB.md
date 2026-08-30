# Field Lab

The Field Lab must prove that Unlinger reconstructs complete supported incidents while preserving the nearest ordinary-browser and human-control counterexamples. Synthetic evidence and live activation evidence remain separate.

## Current corpus and harness evidence

The redacted corpus currently covers:

- abandoned agent-browser Chrome-for-Testing tree;
- active Playwright controller and browser tree;
- abandoned Playwright browser tree;
- Puppeteer with a persistent profile;
- ordinary Google Chrome with a standard profile;
- headed automation under human control;
- manual CDP without a framework anchor;
- PID reuse between observations.

Core/daemon tests additionally cover durable abandonment grace and reset, frozen-plan ordering, TERM-before-KILL per exact target, membership/identity change aborts, bounded revival attribution, persistence retention, IPC pause/resume, report-only signal suppression, and terminal receipts after a post-signal runtime failure. A macOS integration test sends TERM only to an isolated child created and owned by the test, after first proving that a changed birth identity is refused.

## Live read-only evidence

On 2026-08-30 a report-only smoke used an isolated temporary database/socket. Native doctor inspected 452 of 452 current-user processes with zero unreadable entries in 28 ms; no current Unlinger incidents were present. CLI status, empty history, pause, paused status, resume, and doctor completed through the real Unix socket.

This proves the current machine's read-only snapshot and local control path. It does not prove supported ghost detection, real cleanup precision, low overhead over time, LaunchAgent survival, or ambient enforcement safety.

## Live isolated enforcement evidence

On 2026-08-30 an owner-approved field run launched Chrome for Testing 151.0.7922.34 through macOS LaunchServices with a unique temporary Playwright-style profile. The browser root was reparented to PID 1, no controller remained, and a simultaneous ordinary Google Chrome root remained live.

The isolated daemon used a temporary SQLite database and Unix socket. The same Playwright incident remained stable through the full durable 90-second abandonment grace, reached CONFIRMED with eight exact members, and entered a frozen cleanup plan. Root TERM was delivered first. The same root survived its grace, so exact-identity KILL was delivered only after revalidation; the terminal receipt reported CLEARED, zero survivors, and both 15/60-second revival checks complete. The ordinary Chrome root was still live afterward. No profile or runtime artifact was deleted by Unlinger.

The first dry-run also exposed two detached `chrome_crashpad_handler` helpers as standalone PROTECTED noise because their enclosing app paths contained browser/framework markers. Browser-root matching was narrowed to the executable basename. A regression test failed before the change and passed afterward; a second real dry-run then produced exactly one main COOLING incident and no standalone crashpad incidents.

This is one isolated supported-family cleanup, not ambient activation, multi-day dogfood, a version-range claim, or zero-false-positive acceptance. Raw captures, local paths, the temporary database, and private continuity remain outside Git.

## Open exit evidence

The original seven-session/seventy-process-class real incident remains unavailable and therefore unverified. Parent/host exit variants beyond the first clean host-exit observation, controller hang, `setsid` escape, partial exit, supervisor revival, daemon restart, wake during cooling, broader simultaneous ordinary-browser cases, persistence corruption, chaos races, and overhead benchmarks remain open.

Headless Guard, reap, and representative hook/script solutions remain comparative subjects. No comparative result is claimed. Future runs must use the same cases and measure precision, counterexample protection, time to detection, complete-tree cleanup, revival and artifact behavior, idle overhead, and required user actions.
