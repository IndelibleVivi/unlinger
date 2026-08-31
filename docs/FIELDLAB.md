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

Core/rules/daemon tests additionally cover the 60-second minimum-age protection, exact product/version allowlisting, controller-version fail-closed behavior, durable abandonment grace/reset, frozen-plan ordering, PREPARED-before-delivery journals, TERM-before-KILL per exact target, membership/identity change aborts, bounded revival attribution, persistence recovery/retention, exact-incident protect/unprotect, IPC controls, report-only signal suppression, and terminal receipts after post-delivery failures. The rule fixtures admit Chrome for Testing only at bundle identifier `com.google.chrome.for.testing` and version `151.0.7922.34`; missing, mixed, wrong, or controller-unverified version facts remain `PROTECTED`.

Native source tests keep live side effects exact and owned. The signal integration sends TERM only to an isolated child created by the test after proving that a changed birth identity is refused. The process-exit event test proves native delivery with a separate owned `/bin/sleep` child. A forked owned-zombie fixture proves that a PID confirmed `SZOMB` through `KERN_PROC_PID` is excluded from snapshots and exact lookup rather than projected as a live unreadable process. Wake and memory-pressure unit tests validate event mapping/coalescing and scheduler semantics without claiming live wake or pressure delivery. Event-source failure tests prove the periodic fallback remains active.

Runtime-artifact source tests cover DAP-only admission, targeted `proc_listpidspath` results including stale-errno zero, open and `O_EVTONLY` references, canonical-to-quarantine rename continuity, complete argv/reference failure, unsafe ownership/type/link cases, parent/file identity changes, symlink refusal, last-hop replacement races, exclusive same-directory quarantine, and durable PREPARED/disposition recovery. A later installed managed run adds one controlled live-DAP point, but two P2 residuals remain outside the proof: crash recovery for a canonical-to-quarantine rename, and the final same-UID pathname swap interval before `unlinkat`. No test authorizes profile, socket, PID-file, or directory deletion.

## Live read-only evidence

On 2026-08-30 a report-only smoke used an isolated temporary database/socket. Native doctor inspected 452 of 452 current-user processes with zero unreadable entries in 28 ms; no current Unlinger incidents were present. CLI status, empty history, pause, paused status, resume, and doctor completed through the real Unix socket.

This proves the current machine's read-only snapshot and local control path. It does not prove supported ghost detection, real cleanup precision, low overhead over time, LaunchAgent survival, or ambient enforcement safety.

## Live isolated enforcement evidence

On 2026-08-30 an owner-approved field run launched Chrome for Testing 151.0.7922.34 through macOS LaunchServices with a unique temporary Playwright-style profile. The browser root was reparented to PID 1, no controller remained, and a simultaneous ordinary Google Chrome root remained live.

The isolated daemon used a temporary SQLite database and Unix socket. The same Playwright incident remained stable through the full durable 90-second abandonment grace, reached CONFIRMED with eight exact members, and entered a frozen cleanup plan. Root TERM was delivered first. The same root survived its grace, so exact-identity KILL was delivered only after revalidation; the terminal receipt reported CLEARED, zero survivors, and both 15/60-second revival checks complete. The ordinary Chrome root was still live afterward. No profile or runtime artifact was deleted by Unlinger.

The first dry-run also exposed two detached `chrome_crashpad_handler` helpers as standalone PROTECTED noise because their enclosing app paths contained browser/framework markers. Browser-root matching was narrowed to the executable basename. A regression test failed before the change and passed afterward; a second real dry-run then produced exactly one main COOLING incident and no standalone crashpad incidents.

A subsequent repeatable harness run used the same CfT version with a fast timing profile. It crossed durable cooling in separate cycles, delivered only root TERM, ended CLEARED with zero survivors and two revival checks, and preserved one simultaneous ordinary Chrome root by exact identity. The run took 14.41 seconds. Its SQLite timeline recorded RECLAIMING and CLEARED 4.194 seconds apart, providing host evidence that terminal events now use completion time rather than cycle-start time.

These are historical isolated supported-family runs, not evidence for the newer schema-v2 controller/version/artifact path, managed-generation restart path, ambient activation, multi-day dogfood, a version range, or zero-false-positive acceptance. Raw captures, local paths, temporary databases, and private continuity remain outside Git.

## First ambient activation evidence

On 2026-08-30 the owner approved installation and early dogfood on the current Mac. A release build was installed as a private per-user LaunchAgent in report-only mode. launchd PID and IPC PID matched, all managed paths passed the 0700/0600 permission contract, the first scan completed, and the same daemon PID crossed subsequent 60-second sweeps with zero confirmed or ambiguous incidents. Successful-operation stderr remained empty.

An early sequential IPC check exposed intermittent three-second client timeouts while the background LaunchAgent used a nonblocking `accept` plus a 20 ms polling sleep. A process sample showed the IPC thread coalesced in that sleep. The listener was changed to blocking `accept`, with the exact owned socket used to wake shutdown. A transactional report-only reinstall then replaced the running candidate through graceful bootout/bootstrap. Two separate 40-request bursts at concurrency 8 completed without error, and history remained readable and empty.

The earlier lifecycle then exercised report-only → enforce → report-only → enforce. Every transition produced a distinct launchd-owned PID, exact launchd/IPC agreement, private permissions, and a completed first reconciliation scan; prior PIDs were gone. The ordinary Google Chrome root and a simultaneously active dedicated Chrome-for-Testing root retained the same PIDs, launch times, and executables throughout. No incident or cleanup receipt was created.

That short enforce process remained healthy across another full sweep, with no confirmed or ambiguous incident, an empty service log, no IP socket, and an idle point sample of 0.0% CPU and 720 KiB RSS. These remain point and short-duration measurements, not sustained overhead or multi-day safety acceptance.

After an independent activation review, the owner authorized containment while the durable journal and managed lifecycle were hardened. The service was paused with no cleanup in progress, transactionally moved from enforce to report-only, and resumed for ordinary report-only observation. Ambient enforcement has never encountered and reclaimed an ordinary real eligible incident.

## Managed full-timing attempts and current boundary

Successive owner-approved managed-harness attempts were deliberately retained as fail-closed evidence rather than relabelled as passes:

- a generation-3 attempt exposed an exited Chrome-for-Testing root that remained visible as a zombie; exact lookup treated it as unreadable instead of gone, so the run stopped. Source now confirms zombie state through `KERN_PROC_PID` and excludes confirmed `SZOMB` records from the live table while retaining fail-closed behavior for every other read failure;
- a generation-4 attempt exposed transport timing, not a process-signal failure: the three-second client deadline expired around a lifecycle mutation whose response committed but arrived late. The client/server contract now separates one 15-second client attempt from the accepted connection's three-second server I/O bound and forbids automatic mutation resend after uncertain delivery;
- generation 5 completed the production-timing process programme for one unique Chrome-for-Testing tree: eight exact members received only the journaled TERM/KILL sequence admitted by the harness, the terminal receipt had zero survivors, both revival checks completed, and roughly 60 MiB was reclaimed. The subsequent DAP action returned `Unsafe`, so no artifact was deleted and the harness as a whole did not pass;
- that artifact failure traced to the superseded all-process descriptor walk: unrelated same-UID Apple agents can deny descriptor metadata, and normal close/reuse churn makes an enumerated FD stale. The replacement uses a targeted exact-path Darwin query plus complete current-user argv scan and remains fail closed on any incomplete result;
- the same run exposed a lifecycle projection race in which a terminal receipt could become visible before the daemon completed its global fail-close. Source now keeps `Failed` terminal, rejects stale Arm state, requires stable no-scan/no-cleanup readiness, and permits only exact Disarm-to-Drain-to-restart recovery;
- generation 7 then completed the process programme but failed closed while scanning argv for DAP references. The failure was a parser defect, not a live reference: a valid LaunchServices process contained empty arguments, and the old parser incorrectly consumed them as padding. The canonical `KERN_PROCARGS2` parser now skips padding only after the executable path, preserves middle/trailing empty argv entries, stops at exact `argc`, and never treats environment strings as argv;
- the first generation-8 run completed process cleanup and DAP removal but rejected its ordinary-Chrome postflight because the user opened a new unrelated root during the several-minute run. The proof was narrowed to the correct invariant: all pre-existing ordinary roots must preserve exact identities, while every receipt and durable journal target must independently belong to the pre-arm frozen field tree. New ordinary roots and normal renderer/utility churn are permitted;
- a later generation-8 run stopped while its synchronous read-only `history(1000)` query exceeded the 15-second client deadline. The fail-safe guard disarmed and restarted at report-only, and no mutation was resent. The harness now polls exact `Explain { incident_id }` on a background worker; only `not_found`, remote unavailable, and transient local I/O retry within the receipt deadline, while the main thread continues native exact-tree sampling every 250 milliseconds. The daemon also serves up to eight connections concurrently so one slow read cannot head-of-line block later control traffic.

Generation 9 was then installed transactionally at the stable report-only floor and passed the complete managed harness described below. Private profiles, runtime paths, incident identifiers, and raw captures remain outside Git.

## Repeatable CfT harness

The live harness is compiled but ignored by ordinary `cargo test --workspace`. An explicit run must provide a Chrome-for-Testing app bundle:

```bash
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
  cargo test -p unlinger-daemon --test cft_fieldlab -- --ignored --nocapture --test-threads=1
```

The default fast profile shortens observation, grace, and revival windows to exercise the real snapshot/classifier/exact-signal path quickly. It does not prove production latency. Add `UNLINGER_FIELDLAB_FULL_TIMING=1` to retain the production 90-second abandonment grace, 15-second observations, 60-second sweep spacing, and 15/60-second revival checks.

The harness refuses ordinary Google Chrome, launches one unique ephemeral profile through LaunchServices, preflights for unrelated COOLING incidents, and wraps the native runtime with an exact owned-identity signal scope. Any out-of-profile signal is rejected and fails the run. When ordinary Chrome roots are present, their PID/birth/executable identities must remain exact after cleanup; a run with none present does not prove that simultaneous counterexample. The harness never deletes its profile or SQLite evidence and prints the retained path for deliberate inspection and recoverable cleanup.

## Managed full-timing acceptance harness

`managed_cft_fieldlab` is the ignored acceptance harness for the installed immutable-generation boundary. It requires an explicit acknowledgement, production timings, the exact active-generation CLI, and a Chrome-for-Testing app bundle:

```bash
UNLINGER_FIELDLAB_MANAGED_ACK=I_ACCEPT_INSTALLED_CFT_SIGNALING \
UNLINGER_FIELDLAB_FULL_TIMING=1 \
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
UNLINGER_FIELDLAB_MANAGED_CLI="/path/to/active-generation/unlinger" \
  cargo test -p unlinger-daemon --test managed_cft_fieldlab \
  -- --ignored --nocapture --test-threads=1
```

The harness refuses ordinary Chrome and fast timings. It first proves the installed service is ready report-only, launches one unique CfT profile, admits one exact detached candidate, and sends SIGSTOP only to that exact root. It then arms the same ready generation, requires one journaled full-timing terminal cleanup, and proves every signal target belongs to the pre-arm frozen field tree while every pre-existing ordinary-Chrome root preserves exact identity. Exact receipt polling is read-only and incident-scoped; it runs separately from the 250-millisecond native absence proof. Next the harness restarts launchd's same generation and requires a visible unavailable/recovery/report-only transition followed by a new daemon instance and fresh enforcement epoch, without resending or mutating the completed cleanup journal. Teardown durably disarms the same instance to report-only and retains the profile/evidence for inspection.

Passing this harness proves one installed managed-generation process/artifact/restart transaction on one host. It does not close the two known artifact P2s or prove broader versions, controller-bearing eligibility, sleep/wake survival, sustained pressure behavior, multi-day dogfood, or release readiness.

### Current controlled result

Generation 9 result: **passed on 2026-08-31 in 341.8 seconds**. One unique eight-member Chrome-for-Testing tree produced nine ordered exact signal actions, zero survivors, both revival checks, an exact DAP `removed` disposition, and roughly 90 MiB reclaimed. The durable attempt/action/artifact projection exactly matched the terminal receipt. Same-generation restart exposed the required transition, created a new instance and enforcement epoch, and left the terminal journal unchanged throughout the post-restart observation. Every ordinary-Chrome root present before the run retained its exact identity; a root opened during the run was allowed as unrelated new state and was never a target. Independent post-run status readback showed generation 9 healthy and ready with requested/effective report-only, no enforcement epoch, no scan or cleanup in progress, and no cleanup block. The isolated 0700 profile and its eight checkpoint/receipt files remain retained for private local inspection.

## Open exit evidence

The original seven-session/seventy-process-class real incident remains unavailable and therefore unverified. Parent/host exit variants beyond the first clean host-exit observation, controller hang, `setsid` escape, partial exit, supervisor revival, daemon restart during cooling, live wake during cooling, sustained real memory pressure, broader versions and simultaneous ordinary-browser cases, artifact crash/swap races, chaos races, sustained overhead, and multi-day dogfood remain open. Source-level corruption recovery, event delivery tests, and the one managed generation-9 result narrow those risks but do not close their broader installed field boundaries.

The specification's “ambiguous under serious sustained pressure” notification condition has no accepted numeric threshold. Until the owner defines one, pressure can accelerate a fresh scan only; Field Lab and future frontend work must not treat ambiguous count, CPU, RSS, or time alone as notification authorization.

Headless Guard, reap, and representative hook/script solutions remain comparative subjects. No comparative result is claimed. Future runs must use the same cases and measure precision, counterexample protection, time to detection, complete-tree cleanup, revival and artifact behavior, idle overhead, and required user actions.
