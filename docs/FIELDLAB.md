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

A subsequent repeatable harness run used the same CfT version with a fast timing profile. It crossed durable cooling in separate cycles, delivered only root TERM, ended CLEARED with zero survivors and two revival checks, and preserved one simultaneous ordinary Chrome root by exact identity. The run took 14.41 seconds. Its SQLite timeline recorded RECLAIMING and CLEARED 4.194 seconds apart, providing host evidence that terminal events now use completion time rather than cycle-start time.

These are isolated supported-family runs, not ambient activation, multi-day dogfood, a version-range claim, or zero-false-positive acceptance. Raw captures, local paths, temporary databases, and private continuity remain outside Git.

## Repeatable CfT harness

The live harness is compiled but ignored by ordinary `cargo test --workspace`. An explicit run must provide a Chrome-for-Testing app bundle:

```bash
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
  cargo test -p unlinger-daemon --test cft_fieldlab -- --ignored --nocapture --test-threads=1
```

The default fast profile shortens observation, grace, and revival windows to exercise the real snapshot/classifier/exact-signal path quickly. It does not prove production latency. Add `UNLINGER_FIELDLAB_FULL_TIMING=1` to retain the production 90-second abandonment grace, 15-second observations, 60-second sweep spacing, and 15/60-second revival checks.

The harness refuses ordinary Google Chrome, launches one unique ephemeral profile through LaunchServices, preflights for unrelated COOLING incidents, and wraps the native runtime with an exact owned-identity signal scope. Any out-of-profile signal is rejected and fails the run. When ordinary Chrome roots are present, their PID/birth/executable identities must remain exact after cleanup; a run with none present does not prove that simultaneous counterexample. The harness never deletes its profile or SQLite evidence and prints the retained path for deliberate inspection and recoverable cleanup.

## Open exit evidence

The original seven-session/seventy-process-class real incident remains unavailable and therefore unverified. Parent/host exit variants beyond the first clean host-exit observation, controller hang, `setsid` escape, partial exit, supervisor revival, daemon restart, wake during cooling, broader simultaneous ordinary-browser cases, persistence corruption, chaos races, and overhead benchmarks remain open.

Headless Guard, reap, and representative hook/script solutions remain comparative subjects. No comparative result is claimed. Future runs must use the same cases and measure precision, counterexample protection, time to detection, complete-tree cleanup, revival and artifact behavior, idle overhead, and required user actions.
