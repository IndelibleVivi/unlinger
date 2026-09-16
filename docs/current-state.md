# Current state

**Updated:** 2026-09-16. **Programme:** 0.1. **Reader posture:** experimental developer source preview; not a signed/notarized App release or multi-day reliability claim.

## Source-only Chrome clone automatic cleanup (2026-09-16)

Current source now treats the exact current-user Chrome `code_sign_clone`
family as a default enforce-mode cleanup target while preserving the ordinary
daemon default of report-only. It retains one previous exact candidate identity
set in memory, requires the same set at the next 15-minute storage observation,
and uses the canonical native process snapshot to block only candidate-level
references: a process executable or absolute argv path inside a candidate, or a
bundle-confirmed clone-cleanup Helper whose exact suffix matches that candidate.
Ordinary Chrome main, renderer, GPU and utility processes outside the candidate
set, and a valid unrelated cleanup suffix, do not block. Incomplete process
coverage, insufficient Chrome-looking facts, and missing/malformed/ambiguous
cleanup-helper suffixes still fail closed. Only a healthy, ready, unpaused, non-draining enforce daemon
may mutate. Deletion stays descriptor-relative beneath the scanner-validated
candidate, never follows symlinks, and is followed immediately by a rescan whose
actual result becomes the persisted/public typed observation. Raw root and
candidate paths remain absent from SQLite, IPC, diagnostics and logs.

Temp-fixture coverage proves report-only non-mutation, first-observation
cooling, second-stable-observation cleanup, candidate-change reset, ordinary
Chrome/helper non-blocking, exact suffix and candidate-path reference blocking,
unrelated-suffix non-blocking, incomplete-coverage blocking, symlink/unexpected-shape refusal,
and truthful failed-removal rescan. For this correction, the complete
`unlinger-macos` test package, focused daemon storage-residue tests, workspace
format check and strict `unlinger-macos`/`unlinger-daemon` all-target clippy pass
locally. Exact source commit `516d483` then passed
[CI 35106242999](https://github.com/IndelibleVivi/unlinger/actions/runs/35106242999).
The installed generation 27 still uses the earlier global-Chrome blocking gate:
with ordinary Chrome running it reports `chrome_process_active`, nine candidates
and 13,273,958,043 logical bytes rather than deleting. The candidate-reference
correction has not replaced or restarted that daemon, has no installed cleanup
receipt, and has not deleted a real Chrome clone. Installed verification remains
a separate gate.

## Installed App memory recurrence and source repair candidate (2026-09-16)

The owner observed the installed `628d822` App recur at about **19 GB**. A macOS
CPU-resource diagnostic for that exact process captured **65% CPU** and footprint
growth from **42.02 MiB to 2,023.98 MiB in 138 seconds** while the App was
non-frontmost. Its dominant main-thread stack runs through SwiftUI/AttributeGraph,
`PlatformViewRepresentableAdaptor.updateViewProvider`,
`AppKitPopUpAdaptor.PlatformView.updateNSView`, popup item Accessibility-property
application and attributed-text resolution/allocation. Unified logs show the
owner's Force Quit ended that process with SIGTERM; this was not a daemon crash
or jetsam. The generation-27 daemon remained small and healthy.

The exact trigger is still intermittent. The old source did not reproduce the
runaway in bounded fixture root/Settings lanes, accelerated polling lanes, or a
15-minute real-daemon packaged run; those passing samples do not erase the
captured failure. The confirmed failing boundary is repeated popup Accessibility
materialization on the polling SwiftUI graph, not yet a uniquely reproduced
whole-app root cause.

Current App source removes that captured private adaptor path from all three
popup controls. Language, pause and notification selection now use one native
`NSPopUpButton` representable whose item objects survive equal configuration;
equal daemon refreshes and notification-mode writes are not republished,
localization resources stay stable until an actual language change, and the
ordinary window releases its SwiftUI/Accessibility host on close before
rebuilding it on reopen with the persistent router. The old closed-host,
equal-refresh and equal-setting paths fail the new focused regressions. The
current candidate passes **91 Swift tests**, release bundling and **460** repeated
fixture Accessibility-tree probes with zero failures, 22,208 KiB final RSS and
101,744 KiB startup maximum. A separate packaged candidate then ran against the
real generation-27 daemon for 900 one-second RSS samples, ending at 18,112 KiB
with a 98,000 KiB maximum and never approaching its 384 MiB cutoff. Installation
and multi-day acceptance remain separate facts; the canonical installed App is
still `628d822` until the authorized recoverable replacement completes.

## Installed v10 baseline (2026-09-15)

Implementation `e26297b` based on `9da14b7` composes bounded FD-list sampling,
explicit no-intervention completion, action-attributed impact accounting and
honest empty-observation/connection-failure presentation with optional exact
host ownership of an existing ordinary Playwright CLI session. On 2026-09-15
the owner authorized direct private dogfood installation. That source was first
accepted as generation 25 with SQLite v10 and remains in the current
generation-27 lineage; frontend schemas v5/v4/v3 and operator v1 remain
version-compatible. Generation 23 / SQLite v8 / Playwright-0.5.0 evidence below
is historical rollback and controlled-cleanup evidence, not current runtime truth.

Schema-v9 migration archives the old cumulative impact row and leaves raw action
and event records intact. Proved-reclaim totals are rebuilt only from retained
cleared attempts with a durable delivered signal; pruned legacy contributions
are not presented as newly proved and force partial-history labeling. Future
pruning preserves the corrected cumulative totals. Schema v10 then adds private
session-owner lease/controller tables without changing that impact authority. A
pre-v10 binary needs its pre-upgrade database backup; do not point it at a v10
database.

The installed Playwright `0.6.0` pack retains task-owned CLI
`1.63.0-alpha-2026-08-31` and exactly allowlists ordinary host-owned CLI
`1.62.1`. The optional lane derives a path-free selector from the controller's
16-hex registry namespace plus unchanged session name, requires same-user peer
and exact-child owner activation, binds only the exact controller/version/UID
inside that lifetime window, and treats release only as abandonment evidence.
Active, missing, reused, unsupported or incomplete ownership remains protected.
The App maps the ordinary missing-owner reason to explicit user copy.

Local inspection of the evaluated `1.62.1` runtime found a detached controller
with one-shot clients and no idle timer. PPID 1, registry age and a temporarily
idle socket therefore cannot safely distinguish abandonment from a live task
that may call again. No supported Codex/other-host adapter currently drives the
new lifecycle automatically, and a fresh owner generation deliberately does not
adopt an older long-lived controller. The installed primitive is consequently
a foundation, not Phase-2 ambient completion or zero-touch daily dogfood
acceptance.

The 2026-09-15 daemon/App candidate regressions cover bounded descriptor saturation/error
handling, owned native FD/socket sampling, no-signal completion through
executor/store/IPC, transactional v8→v9→v10 migration, path-free ordinary
session discovery, lease generations, exact owner/controller/version/session
binding, unsupported-version and active-client protections, operator/CLI
framing, App copy and notification suppression. The full workspace format,
strict clippy, test and release-build gates pass; the source-only doctor is
healthy and the dry-run scan remained nonmutating. The native App passed 86
Swift tests, its bundle verifier's 9 regression tests, release bundling, two
isolated report-only socket passes (7 + 7), and a 467-probe Accessibility/RSS
run with zero probe failures and 30,000 KiB final RSS. Those remain source and
isolated-fixture results. The installed transaction and App/socket evidence is
recorded below; no live-browser cleanup, ordinary ambient eligible incident,
multi-day, installed Accessibility/RSS, packaged-notification, or release
acceptance is claimed.

## Source, remote and installed state

| Surface | Observed truth |
| --- | --- |
| Runtime implementation | `e26297b`: SQLite v10 / Playwright `0.6.0` composition described above; integrated on `main` by tree-identical ancestry merge `d2a685b` |
| Subsequent test correction | `f22e08eb3410050b380eaee7b84c4ccda2a3ad1a`: bounded post-release offline-lock tests; production locking/timeouts unchanged |
| Baseline remote verification | [CI 34163955192](https://github.com/IndelibleVivi/unlinger/actions/runs/34163955192) passed for `b70bc94`; [CI 34165331792](https://github.com/IndelibleVivi/unlinger/actions/runs/34165331792) passed for `f22e08e`, including default-parallel Rust tests, release build, Swift tests and App bundling |
| Current reader preparation | Published source-preview candidate `de1a9c3`: bilingual reader guides, licensed material scopes, current architecture and safe demo teardown; [exact CI 34170593229](https://github.com/IndelibleVivi/unlinger/actions/runs/34170593229) passed all steps |
| Current source verification | Chrome candidate-reference correction `516d483` passed [exact-head CI 35106242999](https://github.com/IndelibleVivi/unlinger/actions/runs/35106242999); the current App repair passes 91 Swift tests, release bundling, a 460-probe Accessibility/RSS lane and a 900-sample real-daemon packaged guard, with exact-head CI still pending |
| App memory repair candidate | Source removes the captured SwiftUI popup Accessibility adaptor path, suppresses equal publications and releases the closed-window host; the exact intermittent trigger and multi-day acceptance remain open |
| Maintainer's reference service | Accepted generation 27, healthy and quiescent `ReadyEnforce`, generation/epoch-bound with no pending candidate lease; it still uses the global-Chrome blocking clone gate and is one reference installation, not a generation number users should copy |
| Reference protocols/persistence | Operator v1, frontend v5/v4/v3, SQLite v10; historical v2 rejected |
| Reference App | Ad-hoc-signed schema-v5 App from `628d822`, installed after the owner Force Quit the 19-GB instance; neither Developer ID signed nor notarized; displaced `e26297b` and earlier `016ca58` bundles retained as recoverable local siblings; source repair is not installed |
| Policy | Playwright `0.6.0`, agent-browser/Puppeteer `0.4.0`; process-only; every artifact flag false |
| Publication | [Repository public](https://github.com/IndelibleVivi/unlinger); source-available under SUL-1.0 + CC BY-NC-SA 4.0; anonymous API and reader/license/diagram access verified; no GitHub Release |

The source daemon still defaults to report-only. The reference service's explicit
generation-27 activation is separate from that default, from building the source,
and from repository publication.

## Current installed transaction evidence

The generation-23 baseline was healthy, quiescent `ReadyEnforce` on SQLite v8.
Generation 24 installed the exact candidate at the report-only floor, migrated
the live database to v10, matched both installed binaries to the release build,
passed seven schema-v5 live-socket tests before and after an exact report-only
restart, and retained its rollback lease and database backup. The matching App
bundle replaced the canonical App only after the prior `016ca58` bundle was
preserved as a recoverable sibling; strict signature/plist/resource verification,
recursive bundle equality, exact launch from the canonical bundle and continued
process presence across the daemon restart passed.

The mandatory rollback then restored generation 23 to healthy, quiescent,
unarmed `ReadyReportOnly`; its old daemon reopened the restored SQLite v8
database and the matching App passed all seven live-socket tests against it. A
fresh generation 25 repeated the v10 migration, binary match, report-only
restart and seven-plus-seven App/socket checks. The owner then accepted the
candidate, retiring the rollback lease, and explicitly armed generation 25. At
that stage the owner's ordinary PATH symlink resolved to generation 25 and
exposed the new `session` surface; it now resolves to generation 27.

The later first App-only memory repair did not replace or restart generation 25.
After exact-head CI passed, the canonical App was replaced recoverably with the
`628d822` bundle and the `e26297b` App was preserved beside the existing
`016ca58` rollback bundle. Recursive bundle equality, strict signature/plist
checks and an exact canonical launch passed. The installed App then completed
180 menu-only and 120 visible-window RSS samples below the 384 MiB cutoff, and
one trusted Accessibility read exposed the live schema-v5 overview and controls.
The later 19-GB recurrence proves those bounded samples were not multi-day
acceptance.

The first Chrome-clone cleanup candidate was then installed as generation 26 at
the report-only floor, exercised through the transactional replacement lane,
and actually rolled back to healthy generation 25. Fresh generation 27 repeated
the replacement and was accepted and armed. Current status readback is healthy,
quiescent `ReadyEnforce` on SQLite v10 with exact PID/generation/binary identity,
event source healthy, zero recovered cleanup attempts and zero attention. Its
storage observation sees nine clone candidates and 13,273,958,043 logical bytes,
but the pre-`516d483` global gate reports `chrome_process_active` while ordinary
Chrome is open. This is installed safe refusal, not automatic-cleanup success.
Pixel-level visual QA, packaged-notification and multi-day App acceptance remain
unperformed.

## Current controlled evidence

Generation 22 installed report-only, passed the seven App socket tests before and after restart, then **actually rolled back** to generation 19 with its exact old CLI/daemon reopening SQLite v7. A fresh generation 23 repeated installation/restart/App checks, was accepted, and was explicitly armed. Earlier initial-source candidates were rolled back before final acceptance and do not supply final-binary acceptance.

Two deliberately created task-owned CfT `152.0.7977.42` sessions then reached terminal process-only receipts on generation 23:

| Result | Controlled task 1 | Controlled task 2 |
| --- | --- | --- |
| Delivered action | One controller TERM | One controller TERM |
| Processes | 8 → 0 | 8 → 0 |
| Estimated memory impact | 91,979,776 bytes | 164,691,968 bytes |
| Revival checks | 2 | 2 |
| Runtime-artifact actions | 0 | 0 |
| Independent unrelated-session check | Verifier stopped at an intermediate record; later terminal readback proved cleanup, not complete control-session acceptance | Separate ordinary session still evaluated `42`, then closed normally by the verifier |

Independent action/impact authority agrees on 16 reclaimed processes and 256,671,744 estimated memory bytes. Actual installed-App readback showed **2 sessions, 16 processes and 244.8 MB**; its memory display uses binary scaling despite the MB label. The separate 1.48-GB clone card is **observed logical disk size**, not freed storage. Private profiles, raw evidence and user paths remain outside Git.

Isolated full-timing task tests cover exact CfT `151.0.7922.34` and `152.0.7977.42`, including active-owner/client protection, terminal cleanup and an unrelated session still usable. These are controlled points, not ordinary ambient eligible cleanup or multi-day dogfood. The complete managed restart/containment Level-4 harness remains historical generation-15 evidence; it is not attributed to generation 23.


The publication candidate passed fresh local formatting, strict workspace clippy, **313 Rust tests** (three explicit live-browser cases ignored), **81 Swift tests**, App bundle checks, and both isolated report-only App/socket passes (**7 + 7**). The new demo supervisor passed its two owned-child regression checks. A clean checkout with matching staged source built the release workspace; its source-only doctor was healthy, dry-run stayed nonmutating, and the one-shot preview produced zero cleanup receipts. The interactive reader example waited for healthy readiness, ran `/usr/bin/true` once, reached `released` with exit 0, and produced no cleanup impact. This verifies the documented source preview, not first installation on a separate clean macOS account. The architecture SVG was rendered and visually inspected; reader links and bilingual command/version/license parity were checked. The unchanged Accessibility/RSS lane was not repeated for these reader/script changes.

## Known limits and verification gaps

- The original generation-17 terminal SQLite disk-I/O failure cause remains unproved. Exact-instance containment/recovery and later transactional replacement succeeded; a later healthy database check does not establish the original cause.
- The command wrapper does not integrate every Codex App host or browser tool automatically. The source v10 optional session-owner primitive also has no supported automatic Codex adapter yet. Exact CLI/browser compatibility, lifetime/client proof and all ordinary gates remain required; unregistered, active-owner, reused, unsupported or unverified controllers stay protected.
- Chrome clone observation now accepts the actual `.app.bundle` shape and no-follow framework links. Installed generation 27 observes nine candidates and 13,273,958,043 regular-file logical bytes but still blocks on any ordinary Chrome process. Source `516d483` narrows that gate to candidate-level references and is not yet installed or credited with a real cleanup.
- All artifact admission is disabled. The dormant DAP engine still has a quarantine-after-crash recovery gap and a final pathname-swap TOCTOU. Native pathname-reference tests also intermittently returned no reference for an owned open ordinary or `O_EVTONLY` descriptor under parallel execution; exact serial tests passed, and the cause is unresolved. The active process path does not use that query. [Safety](SAFETY.md) owns these boundaries.
- The old zero-deadline offline-lock test failed because a concurrent fork can inherit an `O_CLOEXEC` descriptor until exec. A deterministic owned-child probe established that cause; `f22e08e` retains held-lock denial and gives post-release acquisition its existing bounded wait. The final exact-head CI passed. A later local full workspace run reproduced the separate dormant native-query failures above; no assertions were weakened.
- The current source fixture passed 460 Accessibility-tree probes with zero read failures and an external 22,208 KiB final RSS sample. The installed `628d822` App's earlier 300-sample/one-tree acceptance was superseded by the later 19-GB recurrence. The current native-popup candidate removes the captured failing adaptor boundary, but neither source checks nor historical points establish the exact intermittent trigger, multi-day App behavior, packaged notifications or every menu organizer/display arrangement.
- No Intel/universal verification, signed/notarized distribution, automatic update path or public release is claimed. Recognition of agent-browser/Puppeteer is not controlled field acceptance.

## Publication preparation

The published candidate `de1a9c3` scan covered 216 text files in both working/index views and its exact committed tree, plus 786 reachable historical text blobs. Its scope-repeated findings were reviewed as synthetic test/fixture paths and notification event-token code. The automated scan skipped two current and six historical binary assets; they were separately inspected as generated source images and derived icons, including metadata and historical provenance. This is a bounded review, not a secrets-free certificate.

A historical frontend handoff was reviewed in full as a technical interface/implementation guide, without private chats, local personal paths or account data. It was removed from the current tree in `df161af`. No history rewrite or separate repository is needed on the inspected evidence. [Provenance](PROVENANCE.md) records material boundaries without copying private working notes.

The owner selected the license scope in [LICENSING.md](../LICENSING.md), confirmed rights authority and authorized public visibility after candidate checks. The existing independent repository was made public after exact-source CI passed; provider readback and unauthenticated API/raw-file requests confirmed access. No repository fork, history rewrite, release tag or binary upload was created. The owner-approved source/content licenses are present. Reader documentation and the architecture export do not imply a released App or completed binary-distribution review. See [acceptance levels](PRE_V0_1_ACCEPTANCE.md), [support](SUPPORT.md) and the [support matrix](support-matrix.v1.json) for claim vocabulary.
