# Current state

**Updated:** 2026-09-17. **Programme:** 0.1. **Reader posture:** experimental developer source preview; not a signed/notarized App release or multi-day reliability claim.

## Installed SQLite v11 storage cleanup result authority (2026-09-17)

Current source raises the daemon database authority from SQLite v10 to v11 and
adds a durable result path for the existing automatic Chrome `code_sign_clone`
cleanup without widening deletion eligibility. A PREPARED attempt row is written
before any descriptor-relative directory deletion, and the deletion is refused
when that write fails. After the deletion and immediate rescan the terminal
result and the real latest residue observation are committed together in one
SQLite transaction, so a result can never disagree with the observation it
settled against. The row persists only an opaque internal attempt token,
timestamps, the planned candidate count, before/after candidate counts and
logical-byte sums, the proved removed count, the retained not-planned count and
one typed `complete | partial | failed | delivery_unknown` outcome; no pathname,
candidate name, argv, user name or raw candidate identity is stored, and logical
bytes are never presented as physical APFS reclaim. An attempt still PREPARED
after a crash or restart is finalized as `delivery_unknown` and is never
rehabilitated from a later lower directory count, which stays a separate
observation. In-process recovery is retried before every due storage cycle and a
cycle whose recovery cannot be made durable is skipped, so no new deletion can
start against uncertain delivery. Frontend schema v5 gains an additive optional
`storage_cleanup_result` summary carrying only the typed outcome, timestamps and
aggregate counts; an older v5 payload without it still decodes, and v4/v3 keep
their meaning and cannot acquire it. Current App source decodes that optional
summary and presents `complete | partial | failed | delivery_unknown`, aggregate
before/after facts, timestamps and the logical-size/APFS caveat in the storage
section without gaining a deletion command. The source v10→v11 migration creates the
new table transactionally and preserves the existing latest residue observation
and session-owner authority.

The final working source passes workspace Rust format, strict clippy, tests and
release build; source-only doctor and dry-run each inspected 414 current-user
processes with zero unreadable-process, argument, executable-identity or
descriptor gaps, and the dry-run remained nonmutating. The App passes **101
Swift tests in 17 suites**, release bundling with verified Info.plist, ad-hoc
signature and both localizations, plus two isolated report-only schema-v5
socket passes (**7 + 7**) across a daemon restart. The new history regression
proves that terminal cleanup controls row visibility and completion time, the
latest observation at or before that cleanup supplies explanation, and a later
observation cannot rewrite the settled row. The architecture SVG was
regenerated from its Mermaid source and visually inspected. The fixture
Accessibility/RSS lane remains **not freshly verified for this working source**:
it refused to start because the console session was locked, launched no fixture
App and did not contact the installed service.

Backend implementation `a9355fa` passed exact-head
[CI 35203895092](https://github.com/IndelibleVivi/unlinger/actions/runs/35203895092).
The startup-volume candidate CLI then installed generation 35 at the report-only
floor, migrated the live database from v10 to v11, passed seven schema-v5 App
socket checks before and after an exact report-only restart, and really rolled
back to generation 34. The restored generation-34 binary reopened the restored
v10 database and the new App passed all seven compatibility checks. A fresh
install of the same artifact became generation 36, repeated the v11 migration,
seven-plus-seven App/socket checks and report-only restart, and was accepted
before the owner-authorized transition back to enforce. It is now healthy,
quiescent `ReadyEnforce` with no rollback lease; the PATH CLI resolves to its
exact binary. The installed schema-v5 App includes the current cleanup-result
presentation and a later bilingual fix for the clear-state detail discovered by
rendered installed-App QA. No clone deletion has yet occurred under v11, so
durable result capture and presentation have installed migration/transport/UI
evidence but not a new live deletion result or multi-day acceptance. A pre-v11
binary must not open the migrated v11 database outside the transactional
rollback path that restores its v10 snapshot.

## Chrome clone per-candidate cleanup and `.app.bundle` correction (2026-09-17)

Current source treats the exact current-user Chrome `code_sign_clone` family as
an enforce-mode cleanup target while preserving the ordinary daemon default of
report-only. It retains prior exact candidate identities only in memory and
evaluates each candidate independently at the next 15-minute observation. A
candidate is eligible only when its device/inode/name identity is unchanged and
the complete canonical native snapshot finds no executable or absolute argv
path inside it and no bundle-confirmed clone-cleanup Helper naming its exact
suffix. A live candidate therefore remains protected without blocking stable
unreferenced siblings. Ordinary Chrome main, renderer, GPU and utility processes
outside candidates, and a valid unrelated cleanup suffix, do not block.
Incomplete process coverage, insufficient Chrome-looking facts and
missing/malformed/ambiguous cleanup-helper suffixes still fail closed for every
candidate. The one observed upstream exception is a live non-cleanup Chrome main
whose exact executable path is already proved inside a scanner-admitted
`.app.bundle` candidate but whose generic `.app` bundle fact is absent. Current
source treats that exact path as complete protection for only that candidate;
it grants no eligibility and does not apply to Helpers, cleanup-type arguments,
argv-only references, candidate-external processes or coverage gaps.

Only a healthy, ready, unpaused, non-draining enforce daemon may mutate. The
remover receives only the eligible exact subset, stays descriptor-relative
beneath scanner-validated candidates, never follows symlinks and immediately
rescans. A complete subset removal with protected survivors emits
`storage_residue.automatic_cleanup_partial`; full removal emits
`storage_residue.automatic_cleanup_completed`. Removal error, unavailable or
incomplete rescan, or any planned identity still present emits failure while
preserving the actual remaining observation. Public `reference_check` remains
aggregate, so `referenced` and `automatic_cleanup_eligible: true` may coexist
when different candidates supply those facts. Raw root and candidate paths
remain absent from SQLite, IPC, diagnostics and logs.

Focused temp fixtures prove full and subset deletion, a new candidate waiting
while an unchanged sibling is cleaned, mixed report-only projection, ordinary
Chrome/helper non-blocking, executable and absolute-argv references, exact,
unrelated and malformed helper suffixes, incomplete global proof, lifecycle
closure, symlink/unexpected-shape refusal, partial remover failure and
unavailable-rescan failure. The per-candidate implementation passed complete
local Rust/App/socket/source gates and exact-head CI as `c272942`. The later
`.app.bundle` correction passes all 18 focused storage-residue tests, complete
workspace format/strict-clippy/test/release-build gates, 91 Swift tests, App
bundling and two isolated seven-test v5 socket passes. Its source-only doctor
inspected 488 processes and dry-run 485, with zero unreadable processes or
identity/argument/descriptor gaps. The earlier broad workspace run also exposed
a test-only control-plane temp-directory collision; its clock-only nonce was
replaced by the established process-local atomic sequence pattern and the full
workspace rerun passed. A fresh Accessibility/RSS lane could not start because
the console session was locked; the prior `c17e60f` fixture and installed guards
remain the applicable memory evidence. Exact commit `a8aed45` passed
[CI 35142430097](https://github.com/IndelibleVivi/unlinger/actions/runs/35142430097).

Installed generation 32 contained the earlier per-candidate implementation from
`c272942`. With ordinary Chrome continuously open, its first and second stable
real observations both saw nine candidates and 13,273,958,043 logical bytes but
reported incomplete process observation and made no mutation. A bounded native
snapshot probe established the exact cause: the live main executable was inside
one scanner-admitted `Google Chrome.app.bundle` candidate, while the generic
bundle reader supplied no `app_bundle` fact for that suffix.

Generation 34 now contains the `a8aed45` correction. Its first production-
interval observation classified the family as `referenced` and waited for
stability. The second removed eight stable unreferenced candidates, immediately
rescanned and persisted the actual remainder: candidate count **9 → 1** and
logical bytes **13,273,958,043 → 1,474,884,227**, with
`storage_residue.automatic_cleanup_partial`. The continuously running ordinary
Chrome main stayed on the same PID and its one candidate remained protected by
`storage_residue.clone_candidate_path_referenced`; the App also stayed on its
same installed process. This is one bounded installed field result, not
multi-day evidence or a guarantee that APFS physically reclaimed the logical
byte difference.

## Source snapshot and rollback recovery correction (2026-09-17)

The first attempt to install the current daemon source as generation 28 reached
the transactional report-only floor but did not finish its first reconciliation
within the readiness window. The current and rollback databases both passed
SQLite `quick_check`; the apparent SQLite messages in the service log were from
an older unchanged log file and are not evidence for this incident. Repeated
native samples instead showed the main thread permanently blocked in a pathname
`open()` while `MacosSnapshotter` tried to derive executable dev/inode identity.
Debugger readback identified the exact target as the release CLI that was
simultaneously hosting the service transaction from a removable build volume.
That CLI waited for daemon readiness while the daemon waited in the executable
pathname open.

The rollback command first exposed a separate recovery defect: ordinary
quiescence validation rejected the exact transaction-owned candidate while it
was unhealthy and still in `FirstScanReportOnly`. Current source now gives only
transaction rollback a narrower validation policy: the exact generation and
instance must be disarmed, unhealthy, non-ready, non-draining and still in
`FirstScanReportOnly`; ordinary install/uninstall behavior is unchanged. The
focused lifecycle suite passes. Replaying rollback from a byte-identical CLI on
the startup volume then drained the stuck instance, restored the transaction
backup and returned generation 27 to healthy, quiescent `ReadyReportOnly` with
zero service-status problems and no pending lease.

Current macOS source removes the pathname-open dependency entirely. It reads
the executable vnode already mapped by the process through Darwin
`PROC_PIDREGIONPATHINFO`, requires that vnode path to equal `pidpath`, and uses
the mapped vnode's dev/inode/size/mtime as `ExecutableIdentity`; a missing
successful regular-file vnode stat, invalid size or mismatched evidence remains
incomplete and fail-closed. Regressions cover both a crafted full-size response
without a valid vnode stat and an owned executable whose pathname cannot be
read. The latter launches an owned executable, removes pathname read access
after launch, proves the old read-open is unavailable and still recovers the
original mapped identity. The complete Rust workspace format, strict clippy,
test and release-build gates pass, as do 91 Swift tests, App bundling and the
two-pass isolated v5 socket smoke. A fresh source-only doctor inspected 514
current-user processes in 110 ms and a separate dry-run inspected 513; both
reported zero unreadable processes, zero argument gaps, zero executable-identity
gaps and zero descriptor gaps. Exact commit `b2ada65` passed
[CI 35130438688](https://github.com/IndelibleVivi/unlinger/actions/runs/35130438688).
Generation 29 then completed report-only install/restart and App/socket checks,
really rolled back to healthy generation 27, and the exact artifact was freshly
installed, checked, accepted and explicitly armed as healthy `ReadyEnforce`
generation 30. The mapped-vnode/recovery correction remains installed in the
generation-34 lineage together with the later per-candidate and `.app.bundle`
corrections.

## Installed App memory recurrence and bounded installed repair (2026-09-16)

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

The installed `c17e60f` App removes that captured private adaptor path from all three
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
with a 98,000 KiB maximum and never approaching its 384 MiB cutoff. The prior
canonical App was then preserved as a recoverable sibling, strict recursive
bundle equality/signature/plist checks passed, and the exact canonical
`c17e60f` process completed **2,400 one-second installed samples** with 13,552
KiB final RSS and a 22,192 KiB maximum. It remained running after the guard.
That same installed process later remained alive through more than four hours
of installed runtime and the generation-33→32→34 daemon transaction, ending the
bounded observation at 12,864 KiB RSS without recurrence.
Before replacement on 2026-09-17, that same `c17e60f` process had remained alive
for 18 hours 45 minutes and was using 12,992 KiB RSS. The newly installed App
kept its Settings page and native notification popup visible while an external
guard sampled the exact process once per second for 180 seconds and Computer
Use repeatedly traversed the complete Accessibility tree. On the final canonical
bundle RSS started at 27,728 KiB, ended at 15,488 KiB and reached a 37,520 KiB
maximum, with no growth trend or tree failure. Rendered QA also exercised Home
→ Browser History → Back → Settings → Back, verified that the formerly raw
`browser.overview.clear.detail` key now resolves to bilingual reader copy, and
confirmed that returning to Home restores the `Unlinger` window title instead
of retaining the prior destination title. The displaced `c17e60f`,
pre-localization `a9355fa`, pre-root-title, `628d822`, `e26297b` and `016ca58`
bundles remain recoverable.
The exact intermittent trigger and multi-day acceptance remain separate facts.

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
| Current storage authority | Source and installed generation 36 use SQLite v11 with a durable auto-cleanup result path: PREPARED before any clone-directory deletion, one terminal result committed in the same transaction as the real latest residue observation, `delivery_unknown` recovery that never infers attribution from a later count, and an additive optional schema-v5 `storage_cleanup_result` with typed outcome, timestamps and aggregate counts/logical bytes only; no v11 live deletion result has occurred yet |
| Subsequent test correction | `f22e08eb3410050b380eaee7b84c4ccda2a3ad1a`: bounded post-release offline-lock tests; production locking/timeouts unchanged |
| Baseline remote verification | [CI 34163955192](https://github.com/IndelibleVivi/unlinger/actions/runs/34163955192) passed for `b70bc94`; [CI 34165331792](https://github.com/IndelibleVivi/unlinger/actions/runs/34165331792) passed for `f22e08e`, including default-parallel Rust tests, release build, Swift tests and App bundling |
| Current reader preparation | Published source-preview candidate `de1a9c3`: bilingual reader guides, licensed material scopes, current architecture and safe demo teardown; [exact CI 34170593229](https://github.com/IndelibleVivi/unlinger/actions/runs/34170593229) passed all steps |
| Current source verification | The SQLite-v11/result/App tranche at `a9355fa` passes workspace Rust format, strict clippy, tests and release build, healthy source-only doctor, nonmutating dry-run, 101 Swift tests / 17 suites, release bundle verification, two isolated report-only v5 socket passes (7 + 7) and [exact-head CI 35203895092](https://github.com/IndelibleVivi/unlinger/actions/runs/35203895092). The subsequent clear-detail localization and root-title repairs again pass 101 Swift tests / 17 suites and release bundling; their final exact-head CI is tracked separately from installed acceptance. Earlier exact-head evidence remains: Chrome aggregate candidate-reference correction `516d483` [CI 35106242999](https://github.com/IndelibleVivi/unlinger/actions/runs/35106242999), App repair `c17e60f` [CI 35118609063](https://github.com/IndelibleVivi/unlinger/actions/runs/35118609063), mapped-vnode/rollback correction `b2ada65` [CI 35130438688](https://github.com/IndelibleVivi/unlinger/actions/runs/35130438688), per-candidate cleanup `c272942` [CI 35138131207](https://github.com/IndelibleVivi/unlinger/actions/runs/35138131207), and installed `.app.bundle` correction `a8aed45` [CI 35142430097](https://github.com/IndelibleVivi/unlinger/actions/runs/35142430097) |
| App memory repair | The installed App retains `c17e60f`'s native-popup/equal-publication/closed-window repair and adds the v11 cleanup-result UI, rendered clear-detail localization fix and root navigation-title restoration. Its final canonical Settings/native-popup process passed 180 one-second installed samples at 27,728 KiB start, 15,488 KiB final and 37,520 KiB maximum during repeated Accessibility traversal; the earlier exact `c17e60f` process also passed 2,400 samples and later reached 18h45m at 12,992 KiB. The exact intermittent trigger and multi-day acceptance remain open |
| Maintainer's reference service | Accepted generation 36, healthy and quiescent `ReadyEnforce` with no pending candidate lease; generation 35 proved v10→v11 migration, restart and real rollback to generation 34 reopening v10 before the exact `a9355fa` backend artifact was freshly installed, accepted and armed as generation 36. The earlier generation-34 production observation removed eight stale clones and retained the one live Chrome candidate; no v11 deletion result exists yet |
| Reference protocols/persistence | Source and installed generation 36: operator v1, frontend v5/v4/v3 and SQLite v11 with optional storage-cleanup result; historical v2 rejected |
| Reference App | Current ad-hoc-signed schema-v5 App with cleanup-result presentation, native popup memory repair, bilingual clear-detail copy and root-title restoration; 101 Swift tests, strict bundle verification, rendered live UI navigation and the 180-sample installed Settings/Accessibility RSS guard passed. It is neither Developer ID signed nor notarized; displaced `c17e60f`, pre-localization `a9355fa`, pre-root-title, `628d822`, `e26297b` and `016ca58` bundles remain recoverable local siblings |
| Policy | Playwright `0.6.0`, agent-browser/Puppeteer `0.4.0`; process-only; every artifact flag false |
| Publication | [Repository public](https://github.com/IndelibleVivi/unlinger); source-available under SUL-1.0 + CC BY-NC-SA 4.0; anonymous API and reader/license/diagram access verified; no GitHub Release |

The source daemon still defaults to report-only. The reference generation 36 was
separately accepted and explicitly armed after its v11 transaction proof; its
current `ReadyEnforce` runtime remains distinct from the default and from
repository publication. The earlier generation-34 clone-cleanup result is not
a generation-36 durable-result field point, multi-day evidence or broad ambient
process-cleanup acceptance.

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
exposed the new `session` surface; it now resolves to generation 36.

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
the replacement and was accepted and armed. Before the later generation-28
attempt, status readback was healthy, quiescent `ReadyEnforce` on SQLite v10
with exact PID/generation/binary identity, event source healthy, zero recovered
cleanup attempts and zero attention. Its
storage observation sees nine clone candidates and 13,273,958,043 logical bytes,
but the pre-`516d483` global gate reports `chrome_process_active` while ordinary
Chrome is open. This is installed safe refusal, not automatic-cleanup success.
Pixel-level visual QA, packaged-notification and multi-day App acceptance remain
unperformed.

Generation 28 then installed at the report-only floor but failed first-scan
readiness because the old pathname-based snapshot blocked on the transaction
CLI executable. The initial rollback validator also rejected that exact
unhealthy pre-ready state. After the narrow transaction-only rollback correction
was built, a byte-identical recovery CLI staged on the startup volume drained
generation 28 and selected generation 27; the first retry from the removable
build volume reproduced the same executable-open deadlock in the restored old
daemon. Replaying from the startup-volume copy completed in the normal readiness
window and returned generation 27 to healthy `ReadyReportOnly`. Generation 28
was not accepted and supplies no cleanup or enforcement evidence.

After exact-head `b2ada65` CI passed, a byte-identical candidate CLI staged on
the startup volume installed generation 29 at the report-only floor. Generation
29 matched the release artifact, passed seven schema-v5 live-socket checks,
completed an exact report-only restart, and passed the seven checks again. The
mandatory rollback restored healthy generation 27 in report-only mode and the
matching App again passed all seven socket checks. A fresh install of the same
artifact became generation 30, repeated seven checks before and after restart,
and was accepted only after those checks. The owner-authorized mode transition
then armed generation 30. Its post-activation readback was healthy, quiescent
`ReadyEnforce`, with exact PID/generation/binary identity, zero problems and no
rollback lease. Its aggregate candidate-reference gate observed one real live
candidate reference and therefore retained all nine candidates; this is safe
installed refusal, not per-candidate cleanup acceptance.

After exact-head `c272942` CI passed, a startup-volume candidate CLI installed
generation 31 at the report-only floor. It matched the release artifact, passed
seven schema-v5 live-socket checks, completed an exact report-only restart and
passed the seven checks again. The mandatory rollback restored healthy
generation 30 in report-only mode, and the matching App again passed all seven
socket checks. A fresh install of the same artifact became generation 32,
repeated the seven checks before and after restart, and was accepted before the
owner-authorized transition back to enforce. Its post-activation readback was healthy
`ReadyEnforce`, with exact PID/generation/binary identity, zero problems and no
rollback lease. Its first two stable per-candidate clone observations retained
all nine candidates because the live `.app.bundle` main lacked a generic bundle
fact. This is installed fail-closed evidence, not cleanup acceptance.

After exact-head `a8aed45` CI passed, the startup-volume candidate CLI installed
generation 33 at the report-only floor. It matched the release artifact, passed
seven schema-v5 live-socket checks, completed an exact report-only restart and
passed the seven checks again. The mandatory rollback restored healthy
generation 32 in report-only mode, and the matching App again passed all seven
socket checks. A fresh install of the same artifact became generation 34,
repeated the seven checks before and after restart, and was accepted before the
owner-authorized transition back to enforce. Current readback is healthy and
quiescent `ReadyEnforce`, with exact PID/generation/binary identity, zero
problems and no rollback lease. The production-timing clone result is recorded
above; ordinary Chrome and the installed App kept their original PIDs throughout
the transaction and cleanup.

Backend implementation `a9355fa` then advanced the installed persistence boundary
to SQLite v11 without changing clone eligibility. A byte-identical CLI staged on
the startup volume installed generation 35 in report-only mode and migrated the
live database to v11. The new App passed seven live schema-v5 socket checks before
and after an exact generation-35 report-only restart. Mandatory rollback restored
healthy, quiescent generation 34 and its v10 database; the exact old CLI opened it
successfully and the new App passed the same seven compatibility checks. The same
unchanged candidate artifact was freshly installed as generation 36, repeated the
v11 migration, seven-plus-seven App/socket checks and report-only restart, and was
durably accepted before the prior enforce policy was restored. Final readback is
healthy, idle `ReadyEnforce`, with exact PID/generation/binary identity, SQLite
v11, zero service problems, no rollback lease and the PATH CLI resolving to the
generation-36 binary. A rendered installed-App pass exposed one omitted
`browser.overview.clear.detail` localization key and stale destination titles
after returning Home. Both localizations, key coverage and the root navigation
title were repaired; the 101-test Swift suite and bundle gate passed again, and
those App-only repairs were recoverably installed without replacing or
restarting generation 36. Home, History, Back and Settings rendered and
navigated successfully, and the 180-second Settings/native-popup RSS guard is
recorded above. No v11 clone deletion has happened yet, so this transaction is
installed migration/compatibility/runtime evidence rather than a live durable-
result deletion point.

## Codex zero-touch host-adapter feasibility (2026-09-17, investigated)

The optional session-owner lane retained in installed SQLite v11 is an exact
host-integration primitive, not an automatic Codex adapter. Source inspection
confirms that activation requires the
authenticated registrar's exact current child and then binds the admitted
ordinary Playwright session to an exact controller identity. An external hook or
observer invoked after Codex has already created its controller cannot satisfy
that parent/child ownership proof and cannot truthfully adopt the controller
into a fresh lease. Wrapping a Codex or browser command in `unlinger session
run` remains a supported explicit wrapper, but it is not install-and-forget
zero-touch behavior.

Turn-level `Stop` or `SessionEnd` notifications are not task/controller terminal
events. Thread archive/close is a stronger lifecycle hint but still lacks a
trusted thread-to-exact-controller/session binding and the lease capability
needed for immutable release. The smallest honest zero-touch integration
therefore requires Codex host support: the host or exact controller parent must
drive reserve/activate when the thread's controller is created, bind the stable
host task/thread identity to that exact controller/session, and drive immutable
release at the real terminal boundary. The daemon would continue to reconcile
missed release hints from exact owner exit and periodic/native observations.
Process name, age, workspace, UI title, log text, PPID after activation and
external wrappers remain non-authoritative heuristics.

No Codex hook, App configuration or host integration was installed by this
investigation. Zero-touch Codex ownership and ordinary owner-bound field cleanup
remain unimplemented and unverified; unbound ordinary sessions stay protected.

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

- The installed Home surface truthfully shows two independently retained proved
  reclaims and the most recent settlement, while Browser History currently says
  that no automatic-cleanup result is available. The history endpoint's bounded
  recent window now contains only protected observations; the older cleanup rows
  were pruned even though independent impact/recent-settlement authority remains.
  The App does not currently synthesize a history row from that separate
  authority. This is a real installed usability mismatch, not loss of the impact
  totals and not evidence of a new generation-36 cleanup.
- The original generation-17 terminal SQLite disk-I/O failure cause remains unproved. Exact-instance containment/recovery and later transactional replacement succeeded; a later healthy database check does not establish the original cause.
- The command wrapper does not integrate every Codex App host or browser tool automatically. The optional session-owner primitive introduced in v10 and retained by source v11 has no supported automatic Codex adapter. A bounded feasibility review found that later hooks/observers and turn-level lifecycle events cannot supply the current contract's exact parent/child activation plus thread-to-controller/session binding; a real adapter needs Codex host support. Exact CLI/browser compatibility, lifetime/client proof and all ordinary gates remain required; unregistered, active-owner, reused, unsupported or unverified controllers stay protected.
- Chrome clone observation accepts the actual `.app.bundle` shape and no-follow framework links. Historical installed generation 34 evaluated candidates independently across two production-interval observations, removed eight stable unreferenced clones and retained the one candidate containing the continuously running ordinary Chrome main. Generation 36 retains that gate and is healthy enforce, but no v11 deletion/result has occurred yet. The generation-34 point does not prove generation-36 result persistence, multi-day behavior, all future Chrome clone shapes or physical APFS reclaim equal to the logical byte reduction.
- All artifact admission is disabled. The dormant DAP engine still has a quarantine-after-crash recovery gap and a final pathname-swap TOCTOU. Native pathname-reference tests also intermittently returned no reference for an owned open ordinary or `O_EVTONLY` descriptor under parallel execution; exact serial tests passed, and the cause is unresolved. The active process path does not use that query. [Safety](SAFETY.md) owns these boundaries.
- The old zero-deadline offline-lock test failed because a concurrent fork can inherit an `O_CLOEXEC` descriptor until exec. A deterministic owned-child probe established that cause; `f22e08e` retains held-lock denial and gives post-release acquisition its existing bounded wait. The final exact-head CI passed. A later local full workspace run reproduced the separate dormant native-query failures above; no assertions were weakened.
- The source fixture passed 460 Accessibility-tree probes with zero read failures and an external 22,208 KiB final RSS sample. The installed `628d822` App's earlier 300-sample/one-tree acceptance was superseded by the later 19-GB recurrence. The installed `c17e60f` native-popup repair then passed 2,400 one-second RSS samples with a 22,192 KiB maximum and the same process later reached 18h45m at 12,992 KiB. The current installed cleanup-result/localization/root-title App passed rendered Home/History/Settings navigation and 180 one-second Settings/native-popup samples with a 37,520 KiB maximum during repeated Accessibility traversal. These bounded observations still do not establish the exact intermittent trigger, multi-day App behavior, packaged notifications or every menu organizer/display arrangement.
- No Intel/universal verification, signed/notarized distribution, automatic update path or public release is claimed. Recognition of agent-browser/Puppeteer is not controlled field acceptance.

## Publication preparation

The published candidate `de1a9c3` scan covered 216 text files in both working/index views and its exact committed tree, plus 786 reachable historical text blobs. Its scope-repeated findings were reviewed as synthetic test/fixture paths and notification event-token code. The automated scan skipped two current and six historical binary assets; they were separately inspected as generated source images and derived icons, including metadata and historical provenance. This is a bounded review, not a secrets-free certificate.

A historical frontend handoff was reviewed in full as a technical interface/implementation guide, without private chats, local personal paths or account data. It was removed from the current tree in `df161af`. No history rewrite or separate repository is needed on the inspected evidence. [Provenance](PROVENANCE.md) records material boundaries without copying private working notes.

The owner selected the license scope in [LICENSING.md](../LICENSING.md), confirmed rights authority and authorized public visibility after candidate checks. The existing independent repository was made public after exact-source CI passed; provider readback and unauthenticated API/raw-file requests confirmed access. No repository fork, history rewrite, release tag or binary upload was created. The owner-approved source/content licenses are present. Reader documentation and the architecture export do not imply a released App or completed binary-distribution review. See [acceptance levels](PRE_V0_1_ACCEPTANCE.md), [support](SUPPORT.md) and the [support matrix](support-matrix.v1.json) for claim vocabulary.
