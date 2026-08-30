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

## Open exit evidence

The original seven-session/seventy-process-class real incident and simultaneous ordinary Chrome counterexample remain unavailable and therefore unverified. Live isolated supported-family cleanup, parent/host exit variants, controller hang, `setsid` escape, partial exit, supervisor revival, daemon restart, wake during cooling, simultaneous ordinary Chrome, persistence corruption, chaos races, and overhead benchmarks remain open.

Headless Guard, reap, and representative hook/script solutions remain comparative subjects. No comparative result is claimed. Future runs must use the same cases and measure precision, counterexample protection, time to detection, complete-tree cleanup, revival and artifact behavior, idle overhead, and required user actions.
