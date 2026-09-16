# Current state

**Updated:** 2026-09-16. **Programme:** 0.1. **Reader posture:** experimental developer source preview; not a signed/notarized App release or multi-day reliability claim.

## Source-only Chrome clone automatic cleanup (2026-09-16)

Current source now treats the exact current-user Chrome `code_sign_clone`
family as a default enforce-mode cleanup target while preserving the ordinary
daemon default of report-only. It retains one previous exact candidate identity
set in memory, requires the same set at the next 15-minute storage observation,
and uses the canonical native process snapshot to block on any bundle-confirmed
Google Chrome or Chrome Helper process, including the clone-cleanup helper, or incomplete
process coverage. Only a healthy, ready, unpaused, non-draining enforce daemon
may mutate. Deletion stays descriptor-relative beneath the scanner-validated
candidate, never follows symlinks, and is followed immediately by a rescan whose
actual result becomes the persisted/public typed observation. Raw root and
candidate paths remain absent from SQLite, IPC, diagnostics and logs.

Temp-fixture coverage proves report-only non-mutation, first-observation
cooling, second-stable-observation cleanup, candidate-change reset, ordinary
Chrome/helper/incomplete-coverage blocking, symlink/unexpected-shape refusal,
and truthful failed-removal rescan. Focused macOS/daemon suites, strict workspace
clippy, the full Rust workspace suite and release build, all 86 Swift tests,
source App bundling, source-only doctor and dry-run scan pass. This is source
behavior only. It has not replaced or restarted the installed daemon or App,
has no installed cleanup
receipt, and has not deleted a real Chrome clone. Accepted generation 25 and
the installed `628d822` App therefore retain their prior observe-only clone
behavior and evidence below.

## Installed App memory hardening candidate (2026-09-16)

The owner observed a recurrent installed-App memory event above 20 GB. The exact
spike did not recur during bounded inspection, so its complete cause remains
unproved and the event is treated as a release blocker rather than explained
away by later low samples. The installed `e26297b` App stayed below 100 MiB in
several exact-child runs after the event; a later idle capture reported a 29 MiB
physical footprint, about 13 MiB of allocated malloc memory and a sub-megabyte
AttributeGraph allocation. One Accessibility transport failure coincided with a
later ScreenCaptureKit capture error and is not attributed to the App without
stronger evidence.

Commit `628d822` removes one concrete unnecessary retention surface:
`AppWindowController` still owns one reusable ordinary `NSWindow` from launch,
but does not construct its SwiftUI/Accessibility hosting tree until first
presentation. The first presentation constructs that tree exactly once. A
focused lifecycle regression fails the old eager-host behavior; all **86 Swift
tests** pass, release bundling passes, the history Accessibility/RSS gate passed
**457 probes with zero failures** and 17.8 MiB final RSS, and a separate
three-minute menu-only polling run passed **180 samples** with 61.4 MiB startup
maximum, 12.4 MiB final RSS and only 448 KiB growth from its minimum. Exact-head
[CI 35072613168](https://github.com/IndelibleVivi/unlinger/actions/runs/35072613168)
then passed every job.

The ad-hoc-signed bundle was installed at the canonical owner-local Applications
path after recursive equality and strict signature/plist checks. The displaced
`e26297b` bundle and earlier `016ca58` bundle remain separate recoverable
siblings. The installed App passed 180 normal menu-only RSS samples with a 73.3
MiB maximum and 11.6 MiB final value, then 120 visible-window samples between
10.8 and 38.0 MiB with a 29.1 MiB final value. A trusted Accessibility read
returned the live browser overview, cleanup-impact/residue cards and controls.
The App was still running at 19.8 MiB after seven and a half minutes. The
generation-25 daemon was not restarted or changed and remained healthy,
quiescent `ReadyEnforce`. These are bounded installed-candidate results, not
proof of the unique root cause, packaged-notification behavior or multi-day
reliability; recurrence still blocks a private-v0.1 reliability claim.

## Current installed candidate (2026-09-15)

Implementation `e26297b` based on `9da14b7` composes bounded FD-list sampling,
explicit no-intervention completion, action-attributed impact accounting and
honest empty-observation/connection-failure presentation with optional exact
host ownership of an existing ordinary Playwright CLI session. On 2026-09-15
the owner authorized direct private dogfood installation. The exact source
candidate is now accepted and active as generation 25 with SQLite v10 and
Playwright pack `0.6.0`; frontend schemas v5/v4/v3 and operator v1 remain
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
| Current source verification | Full local gates, [exact-source CI 34968538132](https://github.com/IndelibleVivi/unlinger/actions/runs/34968538132), [current-head CI 34968967500](https://github.com/IndelibleVivi/unlinger/actions/runs/34968967500), and App-memory exact-head [CI 35072613168](https://github.com/IndelibleVivi/unlinger/actions/runs/35072613168) passed |
| App memory hardening candidate | Installed `628d822` lazy ordinary-window host; 86 Swift tests, release bundle, 457-probe source Accessibility/RSS gate, 180 source menu-only samples, 180 installed menu-only samples, 120 installed visible-window samples and one trusted installed Accessibility read passed; exact 20-GB cause and multi-day acceptance remain open |
| Maintainer's reference service | Accepted generation 25 from `e26297b`, healthy and quiescent `ReadyEnforce`, generation/epoch-bound with no pending candidate lease; this is one reference installation, not a generation number users should copy |
| Reference protocols/persistence | Operator v1, frontend v5/v4/v3, SQLite v10; historical v2 rejected |
| Reference App | Ad-hoc-signed schema-v5 App from `628d822`, installed and running; neither Developer ID signed nor notarized; displaced `e26297b` and earlier `016ca58` bundles retained as recoverable local siblings |
| Policy | Playwright `0.6.0`, agent-browser/Puppeteer `0.4.0`; process-only; every artifact flag false |
| Publication | [Repository public](https://github.com/IndelibleVivi/unlinger); source-available under SUL-1.0 + CC BY-NC-SA 4.0; anonymous API and reader/license/diagram access verified; no GitHub Release |

The source daemon still defaults to report-only. The reference service's explicit
generation-25 activation is separate from that default, from building the source,
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
candidate, retiring the rollback lease, and explicitly armed generation 25. The
owner's ordinary PATH symlink now resolves to generation 25 and exposes the new
`session` surface.

The later App-only memory repair did not replace or restart that daemon. After
exact-head CI passed, the canonical App was replaced recoverably with the
`628d822` bundle and the `e26297b` App was preserved beside the existing
`016ca58` rollback bundle. Recursive bundle equality, strict signature/plist
checks and an exact canonical launch passed. The installed App then completed
180 menu-only and 120 visible-window RSS samples below the 384 MiB cutoff, and
one trusted Accessibility read exposed the live schema-v5 overview and controls.

A later full observation cycle remained healthy and quiescent `ReadyEnforce`
with SQLite v10, exact PID/generation/binary identity, armed generation 25,
event source healthy, zero recovered cleanup attempts, zero attention and no
scan or cleanup in progress. The installed App remained running. A fresh
browser overview saw two ordinary Playwright/Google Chrome `152.0.7977.83`
sessions and correctly protected both as `browser_product_unsupported`; this is
installed observation, not an eligible cleanup or host-adapter proof. Storage
residue remained observe-only with `automatic_cleanup_eligible = false`. The
later App-memory acceptance read those live impact/residue facts in the installed
UI through Accessibility. Pixel-level visual QA, packaged-notification and
multi-day App acceptance were not performed.

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
- Chrome clone observation now accepts the actual `.app.bundle` shape and no-follow framework links. A real observation matched one clone and 1,475,187,528 regular-file logical bytes; live references were observed and deletion remains unavailable.
- All artifact admission is disabled. The dormant DAP engine still has a quarantine-after-crash recovery gap and a final pathname-swap TOCTOU. Native pathname-reference tests also intermittently returned no reference for an owned open ordinary or `O_EVTONLY` descriptor under parallel execution; exact serial tests passed, and the cause is unresolved. The active process path does not use that query. [Safety](SAFETY.md) owns these boundaries.
- The old zero-deadline offline-lock test failed because a concurrent fork can inherit an `O_CLOEXEC` descriptor until exec. A deterministic owned-child probe established that cause; `f22e08e` retains held-lock denial and gives post-release acquisition its existing bounded wait. The final exact-head CI passed. A later local full workspace run reproduced the separate dormant native-query failures above; no assertions were weakened.
- The current source fixture passed 457 Accessibility-tree probes with zero read failures and an external 18,256 KiB final RSS sample. The installed `628d822` App separately passed 300 RSS samples across hidden-menu and visible-window states plus one trusted Accessibility-tree read. Historical installed-App evidence includes ten minutes/1,398 tree reads and an earlier bounded v5 deployment observation; neither current nor historical points establish multi-day App behavior, packaged notifications or every menu organizer/display arrangement.
- No Intel/universal verification, signed/notarized distribution, automatic update path or public release is claimed. Recognition of agent-browser/Puppeteer is not controlled field acceptance.

## Publication preparation

The published candidate `de1a9c3` scan covered 216 text files in both working/index views and its exact committed tree, plus 786 reachable historical text blobs. Its scope-repeated findings were reviewed as synthetic test/fixture paths and notification event-token code. The automated scan skipped two current and six historical binary assets; they were separately inspected as generated source images and derived icons, including metadata and historical provenance. This is a bounded review, not a secrets-free certificate.

A historical frontend handoff was reviewed in full as a technical interface/implementation guide, without private chats, local personal paths or account data. It was removed from the current tree in `df161af`. No history rewrite or separate repository is needed on the inspected evidence. [Provenance](PROVENANCE.md) records material boundaries without copying private working notes.

The owner selected the license scope in [LICENSING.md](../LICENSING.md), confirmed rights authority and authorized public visibility after candidate checks. The existing independent repository was made public after exact-source CI passed; provider readback and unauthenticated API/raw-file requests confirmed access. No repository fork, history rewrite, release tag or binary upload was created. The owner-approved source/content licenses are present. Reader documentation and the architecture export do not imply a released App or completed binary-distribution review. See [acceptance levels](PRE_V0_1_ACCEPTANCE.md), [support](SUPPORT.md) and the [support matrix](support-matrix.v1.json) for claim vocabulary.
