# Current state

**Updated:** 2026-10-25. **Programme:** 0.1. **Reader posture:** experimental developer source preview; not a signed/notarized App release or multi-day reliability claim.

## Active repair candidate 52 (2026-10-25)

Source [`f45778e`](https://github.com/IndelibleVivi/unlinger/commit/f45778e46008c106fc78fd94e0977b272dd7faac)
passes [exact-source macOS CI40174916193](https://github.com/IndelibleVivi/unlinger/actions/runs/40174916193)
and is active as **generation 52 / SQLite v14 / frontend v5 / operator v1**.
It is healthy, quiescent and `ReadyReportOnly`, unarmed and without an enforcement
epoch. **It is not accepted:** the candidate rollback lease and prior generation
49 material remain intact. The PATH shortcut still selects the accepted 49 CLI;
current candidate operations must select the exact generation-52 CLI explicitly.

- Generation 51 passed **7 socket checks + report-only restart + 7**, with fresh
  mutation UUIDs and serial functional checks while the retained App kept running.
- The exact candidate then really rolled back to generation 49. The old daemon
  and CLI reopened the restored v14 healthy, quiescent, report-only and unarmed.
- The unchanged artifact freshly installed as 52 and repeated **7 + restart + 7**.
  These are 28 executed checks across both generations, not old receipt replay.
- Both uv and Node discovery now report **available** on the real installation.
  Report-only grants no maintenance authority; their last successful results
  retain their earlier generations/dates. No new cache removal is claimed.
- A bounded observer completed **20 readbacks spanning 3m11s**, all the same
  healthy ready report-only instance, zero read errors/failures and every sampled cleanup flag false. Database/WAL/SHM identities stayed stable. The minimum observed
  startup free space was about **1.08 GiB**; the earlier four-minute window held
  about 1.16 GiB. This does not resolve the recurrent storage-pressure cause.

The installed App is still the preceding schema-v5 artifact. The new verified
single-v5 bundle is prepared but has not replaced it. A fresh ordinary-window
navigation check is blocked by the **locked console**, not credited as passed.
The retained App's one-point RSS sample was 15,744 KiB; it is not a stress result.
Final packaged-App navigation, acceptance, production-timing managed cleanup and
restoration of the original enforce policy remain incomplete. Keep the candidate
report-only and retain its transaction material until those gates are completed
or the real rollback is selected. Earlier generation-49 field failures below
must not become a successful generation-52 receipt or a multi-day claim.

## Implementation and failure evidence (2026-10-25)

**Historical pre-repair failure:** generation 49 became terminal Failed and
effective report-only before the 51→49→52 transaction above. After a real
candidate-50 rollback restored it healthy,
the startup volume filled: Swift's linker returned ENOSPC/28 and generation 49
recorded SQLite `DiskFull`/13. Its first-failure time is 2026-10-25 15:46:23 +08,
reported at `store.rs:1655`. This is a directly observed disk-full event, distinct
from the earlier BUSY attempt and unproved SHMSIZE causes. Further production field work requires adequate host write capacity; source
verification continues on the development volume. No personal files or live caches were
manually deleted to obtain a pass.

Candidate [`26b400d`](https://github.com/IndelibleVivi/unlinger/commit/26b400d862bb7de46572e867dd81d201f62d2ba1)
passed [exact-source CI40173314896](https://github.com/IndelibleVivi/unlinger/actions/runs/40173314896)
and installed as generation 50, healthy/quiescent/report-only. Its first
concurrent installed socket run had three uncertain deliveries; read-only
reconciliation found historical committed receipts for the fixed test IDs,
with no new action. Seven subsequent read-only parallel requests completed in
2.6–6.1 ms. The original timeout cause remains unproved. Generation 50 was not
accepted; the actual rollback restored 49 and its old daemon reopened v14.

The follow-up fixes a confirmed uv version-parser error: real `uv --version`
contains parenthesized build metadata after `0.11.20`. Both that released form
and the version-only form now admit the same exact pin; malformed suffixes stay
unavailable. The functional socket suite uses new UUIDs per run and runs serially,
so acceptance exercises fresh mutations while ordinary App polling remains active.
It never automatically resends a timed-out mutation. Final follow-up checks and
remote CI remain separate from the earlier candidate evidence.

The owner retired unused migration/client compatibility for the sole dogfood
installation. The current source candidate creates new SQLite v14 directly and
reopens valid v14 without rewriting existing data or historical impact provenance.
Nonempty older/newer/unversioned stores are refused unchanged. Only frontend v5
and operator v1 remain active; retired frontend v2/v3/v4 receive typed
`unsupported_schema` before command dispatch. Existing scenarios now live under
one v5 fixture/resource path. The historical migration ladder, legacy response
projections and schema-selecting client constructors have been removed.

The generation-49 production-timing managed field attempt exposed a distinct
`SQLITE_BUSY`/5 failure at process-action PREPARED `BEGIN IMMEDIATE`. Its durable
attempt ended FAILED with **zero process actions and zero artifact actions**;
there is no successful cleanup or reclaim claim. The harness completed its
transactional fail-safe recovery into a fresh healthy, quiescent report-only
instance. Generation 49 remains accepted with no rollback lease and PATH still
selects its exact CLI. The healthy recovery described here precedes the later
DiskFull failure above. **Current policy is report-only** while repair and host
readiness are validated; its earlier enforce observations below are dated evidence.
The existing enforce policy may be restored only after the authorized repair
and healthy/quiescent verification. This failure is separate from both older
SHMSIZE and kernel-confirmed ENOSPC events.

The repair gives all clones of one `HistoryStore` a single active mutex-owned
SQLite connection, with independent short transactions. Native observation,
signals and grace/revival waits never hold that guard. Per-operation private
component checks, original Unix database device/inode binding, WAL/FULL durability,
external-writer timeout, PREPARED and terminal fail-close remain required. It
neither retries mutations nor rehabilitates Failed. Focused checks cover clone
serialization beyond the old timeout, bounded refusal of an external writer,
and refusal of a replaced database path. Source, installation and a repeat of
the failed production field lane are separate gates; the first repair candidate was installed only for testing and rolled back;
no repair candidate is currently accepted or active.

A native owned-child regression also exercises an executable that is unlinked
while still running. A missing executable pathname remains incomplete and
cannot acquire signal authority from an arbitrary mapped region. The current
ambient path gaps therefore remain a real coverage limit. Startup-volume space
is still variable and constrained; no personal files were removed to obtain a
passing run. The supplied CfT workload uses the exact admitted official artifact
and its own profile; no ordinary Chrome or runtime-artifact admission is involved.

### Follow-up verification after the installed probe

The uv metadata/fresh-ID follow-up passes strict workspace clippy, all **455 Rust
tests** (16 explicit opt-in ignored), release workspace build, source-only doctor
and dry-run, and **131 Swift tests / 19 suites**. These final checks used the
development volume after startup-volume ENOSPC interrupted an earlier test log
and a Swift manifest/link attempt. The runtime App/resources are unchanged from
the verified bundle above. A fresh installed acceptance and the production-timing
cleanup repeat remain incomplete; no earlier test, receipt or CI is substituted
for them. The functional gate no longer reuses historical mutation IDs.

### Current source verification

The candidate passes fresh formatting, strict workspace clippy, **455 Rust tests**
(16 explicit opt-in tests ignored), release workspace build, source-only doctor
and nonmutating dry-run. The native doctor inspected 679 processes; 14 lacked
executable path identity, with no missing-errno or mapped-vnode failures. Those
path gaps remain incomplete rather than being promoted into cleanup authority.
The App passes **131 Swift tests / 19 suites**, 11 bundle-verifier tests, two demo
ownership tests and relocated release bundling with only v5 packaged fixtures.
The isolated report-only service passes **7 socket checks + restart + 7**, retaining
independent uv/Node observations and zero maintenance attempts. The unchanged
120-second synthetic history Accessibility gate passes **464 traversals, zero
read failures**, initial/maximum **77,424 KiB**, final **17,728 KiB**. Later fixture
accounting/resource consolidation passed the Swift and bundle checks again;
the history rendering code and AX fixture were unchanged. This is source and
isolated evidence; the original artifact passed exact-source CI; the follow-up CI and installed
repeat remain separate gates.

### Previous installed checkpoint: observation/execution candidate


Candidate [`5fc355b`](https://github.com/IndelibleVivi/unlinger/commit/5fc355b26ed470c28cbd2de6eb2f5b1de5137d1e)
is pushed, source-complete, isolated report-only verified and now installed as
accepted generation 49 / SQLite v14. Its
[exact-source macOS CI](https://github.com/IndelibleVivi/unlinger/actions/runs/40158626156)
passed. The backend was transactionally replaced; the unchanged schema-v5 App
was retained. No controlled browser or manual cache operation was started.
The preceding WAL-lifetime
candidate [`3fdb694`](https://github.com/IndelibleVivi/unlinger/commit/3fdb6941188fa956593872780271943c0180d29e)
and its [exact-source CI](https://github.com/IndelibleVivi/unlinger/actions/runs/40147673014)
remain evidence for that earlier artifact rather than this new implementation.

The product remains ambient maintenance of abandoned local automation, with
integration-independent core cleanup and optional stronger lifetime evidence.
Current source connects process classification/execution/receipts/impact,
Chrome clone maintenance and independent uv/Node maintenance. The common
controller-bearing browser case still needs exact owner lifetime evidence;
the optional primitive is not an automatic Codex adapter. Browser/controller
compatibility remains narrow, and Phase-2 ordinary ambient/multi-day exit remains
open. The [programme](IMPLEMENTATION_PLAN.md) continues reliability, native
coverage, admitted controllerless use and real ordinary-use evidence together.

### Installed transaction and current acceptance boundary

The previous accepted generation 47 was found terminal Failed with the distinct
October-21 18:40:20 +08 SQLite SHMSIZE/4874 first-failure record. Its cause remains
unproved; it must not inherit the earlier 11:48 kernel-confirmed ENOSPC cause.
The existing exact-generation CLI transactionally recovered that instance in
report-only mode after preserving first-failure evidence and a SQLite-consistent
v14 backup. A startup-volume, byte-identical and strictly code-signed copy of
the pushed candidate then performed the complete installation transaction:

- Generation 48 passed **7 schema-v5 socket checks + report-only restart + 7**.
- Its real rollback restored generation 47; the old generation's own daemon and
  CLI reopened the restored v14 database healthy/quiescent, report-only and
  unarmed. A generic SQLite reader was not used as the old-binary proof.
- The unchanged candidate freshly installed as generation 49 and repeated
  **7 + restart + 7**. Two contiguous healthy/quiescent report-only readbacks
  preceded acceptance. Earlier busy preflight refusals did not bypass that gate.
- Generation 49 is durably accepted with no rollback lease. PATH selects its
  exact installed CLI. The pre-existing enforce policy was separately restored
  after another pair of quiescent readbacks: healthy `ReadyEnforce`, zero service
  problems and a fresh generation-bound epoch.

A bounded post-activation observer completed **35 readbacks spanning 9m41s**,
all the same healthy, ready, enforce instance, with zero read errors and no
first-failure record. Final service/operator readback was quiescent. The same
App process remained present in all 35 sparse samples: maximum RSS **26,736 KiB**
and maximum sampled CPU 12.4%. These are bounded runtime samples, not GUI
navigation, Accessibility stress, an eligible cleanup or multi-day acceptance.

Browser impact and the latest storage-cleanup result were unchanged across that
window. uv/Node retained their earlier terminal maintenance results. Node
availability changed from available to unsupported without a new attempt; a
subsequent read-only builtin query still identified exact Node v26.7.0/arm64 in
117 ms. The daemon discovery failure is unexplained. Neither that query nor
the historical successful result proves current automatic-maintenance readiness.

The installed store's WAL/SHM identities remained stable across repeated reads.
This is bounded installed evidence for the retained-connection lifetime, not a
diagnosis of old SQLite failures or a guarantee under disk exhaustion.

The retained App passes strict signature and packaged-resource verification,
and its ordinary Home was observed against generation 49. Fresh History and
Settings navigation remains **unverified**: the UI automation transport returned
empty/no-window states. A bounded native sample showed the main thread waiting
in AppKit and a background overview decode; it did not prove an App hang or a
repeat of the historical memory runaway. Installed socket tests establish the
v5 client/read/mutation/restart path, not complete GUI acceptance.

The production-timing managed CfT lane has **not started**. Startup-volume free
space fell from about 4.5 GiB to 704 MiB and then 291 MiB during this transaction,
with 10 GiB of swap allocated; the later observer reached a **150 MiB** minimum
before space returned to roughly 1.6 GiB. No new profile or signal harness was launched
under that changing constraint. Later spare capacity does not explain the
pressure or establish a stable test window. The field lane needs a stable host
before its exact-owned browser/profile workload is added; no version, timing,
quiescence or cleanup-proof gate was relaxed. The prior controlled receipts,
Node/uv results and clone results retain their original generations and dates.

### Continuous observation and exact process activity

The long-running daemon now runs process cleanup on one single-flight worker.
TERM/KILL grace and revival waits no longer stop ordinary owner reconciliation,
native observation and fresh roster publication. Each dispatch still starts
from a fresh confirmed plan and durable PREPARED attempt; there is no queue of
old plans or second process attempt. The existing two-observation gap remains
within the observation cycle. `--once` and direct engine/field-harness callers
retain synchronous behavior through the same execute/settle routine.

The exact worker owns process activity through terminal settlement and unwind;
a replacement scan or empty roster cannot clear it. Every signal retains the
frozen epoch/revision gate. Pause followed by immediate Resume cannot restore
that plan. Worker completion wakes a fresh cycle through the bounded scheduler
wait even when native event hints are unavailable. Shutdown cancels and joins;
Drop never detaches. A panic or error closes authority, and surviving PREPARED
recovery happens only after the exact worker has joined.

The integration exposed an existing unmanaged-lifecycle gap: successful
observation completion could overwrite terminal Failed. `ControlPlane` now
rejects that transition under the same status lock that publishes readiness;
managed durable fail-close retry is preserved. Tests cover observation during a
held cleanup wait, one open attempt, no dispatch of an intervening stale plan,
exactly-once receipt delivery, sticky cancellation, observer failure,
unknown/panicked delivery, shutdown/join and v5/v4/v3 Reclaiming projection.
Independent static review found no actionable issue, including observation /
retry-block / prune transactions concurrent with terminal receipt and impact.

### Native coverage remains conservative and is now diagnosable

Native path reads now retain the API return/errno distinction instead of losing
it inside the old string-error wrapper. The decoder checks the native byte count,
complete absolute UTF-8 pathname and mapped-vnode match. Additive serde-defaulted
coverage counters distinguish unavailable paths, zero-return-without-errno and
unproved mapped vnodes in doctor/dry-run output. No path, argv or PID is added to
the diagnostic counters; identity/classification/clone gates are unchanged.
Unpathed processes remain identity-incomplete, and no region-walk fallback or
alternative path authority was introduced.

The fresh main-session source doctor is healthy and takes **149 ms for 513
inspected processes**: 15 executable identities are incomplete, all 15 due to
unavailable native paths; mapped-vnode-only failures and zero-without-errno
counts are both zero. A separate earlier worker probe reported zero-without-
errno, while the preceding C probe recorded ENOENT. Those observations cannot
be combined into a kernel-cause claim. The pathname omission remains unresolved;
this source change makes the boundary visible rather than declaring coverage
complete. Fresh dry-run finds five protected Playwright incidents and performs
no cleanup.

### Source and isolated verification

Formatting, strict workspace clippy, **457 Rust tests** (16 explicit opt-in
cases ignored), release workspace build, source doctor and dry-run pass.
The full main-session macOS suite passes, including the native clock and
loopback cases unavailable in the worker environment. App validation passes
**131 Swift tests /19 suites**, 11 bundle-verifier tests, two owned-demo tests
and relocated release bundling. The isolated v5 report-only socket lane passes
**7 + restart + 7**, with independent uv/Node observations and no maintenance
attempt or IP listener.

The unchanged default history Accessibility/RSS gate completes its full
**120 seconds with 468 probes and zero transient failures**: 84,720 KiB
initial/maximum RSS and 20,368 KiB final RSS. The earlier retained-window
`AXError -25202` after 129 successful probes remains an unexplained failed
observation; a passing source fixture does not retroactively establish its
cause, installed-App health or multi-day acceptance. No fixture, duration,
threshold or assertion was weakened.

The preceding source cycle encountered startup-volume ENOSPC and then an
external temporary-root Unix-socket path-length limit. This cycle uses the
already established short source-test temporary root; App bundle validation
and the exact staged socket-smoke daemon retain their startup-volume boundary.
Host storage pressure has not been solved by the source changes.

### Retained store and installed boundary

The source retains the prior shared idle WAL connection lifetime for a
`HistoryStore` and its clones. Operation transactions, FULL durability,
per-operation path validation, available checkpointing and SQLite-consistent
backup remain independent. Connection setup captures SQLite's nonzero handle
OS errno when available. This reduces last-close/sidecar churn; it is not a
proven repair of the historical SHMSIZE/SHMOPEN causes. SQLite v14 and all public
protocol/admission/artifact boundaries are unchanged. The support matrix still
describes the existing Chrome clone enforce capability and required proof,
rather than stale observe-only or runtime-completeness constants.

The last verified producer versions remain uv 0.11.20 and Node 26.7.0. The
earlier transient uv `unsupported` observation has no proved cause and must not
be attributed to a version upgrade.

The October-25 recovery/48→47→49 transaction above supersedes the earlier
generation-47 live-health record. Installation and restoration of the existing
enforcement policy are verified; current-generation controlled/ambient cleanup
and multi-day acceptance are not claimed. Older healthy/enforce records below
retain their dated scope.

## Normal Node result and disk-full recovery (2026-10-21)

Generation 47 completed normal automatic Node compile-cache maintenance at
2026-10-21 01:05:30 +08: **3,833 successful final-file unlinks / 26,612,788
validated logical bytes** (about 25.4 MiB). The SQLite-v14 terminal result and
CLI/browser projection agree, and the PREPARED token is settled. The native
temporary entry dated October 19 remains present after that result. No manual
prune, timer override or live-cache injection was used. This closes the
previous checkpoint's missing current-generation normal Node result; logical
bytes are not physical APFS reclaim or browser impact.

At 11:48:56 +08, that same daemon instance entered terminal `Failed` with
`SystemIoFailure`, extended code 4618 (`SQLITE_IOERR_SHMOPEN`), at the connection's
`journal_mode=WAL` setup. The first-failure record and retained stderr identify
this event. At 11:48:50, the macOS kernel explicitly recorded `unlingerd: ENOSPC`
and `No space left on device (28)` while trying to write the same SQLite `-shm`
sidecar. Other processes also received ENOSPC in that window. This is direct
host evidence of storage exhaustion for this incident; it does not establish
the causes of earlier generation-45 or generation-36 errors. A consistent
read-only backup still passes `quick_check`.

The exact Failed generation-47 instance was transactionally replaced in
report-only mode, checked healthy/quiescent, then restored to its existing
enforce policy. It is again healthy `ReadyEnforce`, armed to generation 47 with
a fresh instance/epoch, zero service problems and no rollback lease. uv and Node
are available again, and the successful Node result is retained. The source
artifact remains `e8faf7a`; no store, retry, WAL-lifecycle or schema change was
needed. Startup-volume free space remains small and variable, so recovery is
not a claim that storage pressure has been resolved. Unknown cleanup delivery
and terminal fail-close retain their existing safety meaning.

## Historical reliability-repair checkpoint (2026-10-20; installed and activated)

Fresh service readback found generation 45 loaded with exact generation/binary
identity but terminal `Failed`, unhealthy/not ready and effective report-only.
Its retained primary error was SQLite `CannotOpen`, extended code 14. A consistent
read-only backup and `quick_check` returned `ok`; the first failure time and cause
remain unproved. The installed transactional CLI replaced that exact Failed
instance in report-only mode, verified fresh healthy/quiescent readiness and
restored the existing enforce policy. Recovered generation 45 subsequently
completed a normally scheduled native uv attempt with zero reported removals;
zero counters mean completion, not proof of no effects. Node on that unchanged
binary remained blocked by its temporary entry before the upgrade. A test-owned six-lane workload
completed 1,200 history/cache
reads and writes without reproducing that error. Source therefore retains the
existing store architecture and terminal fail-close policy. It adds path-free
SQLite source basename/line and an instance-sticky first-failure record, logged
with time/generation/instance before the durable fail-close attempt.

The owner approved independent maintenance of frozen validated Node final files
while safe native temporary entries remain untouched. The scanner replaces its
old whole-bucket temporary-writer refusal directly; no additional cache schema,
retry mechanism or parallel cleanup path is added. Unknown/unsafe contents still
block the bucket, and newly published final files invalidate the frozen plan.
Sixteen focused Node tests and the explicit isolated real-producer continued
import/offline-regeneration test pass. The current installed bucket's temporary
entry has not been manually removed or changed. The repaired source App also keeps daemon order for equal-millisecond history
events and anchors observation context before the exact terminal event. It
replaces random-token comparison rather than retaining another ordering path.
Home and the existing status item use live readiness for persistent Failed
visibility without a new service state machine or polling loop. Fresh Swift
validation passes 131 tests in 19 suites; Rust formatting, strict workspace
clippy, 438 workspace tests (16 explicit opt-in ignored), release build and
source-only doctor/dry-run pass. One local run reproduced the previously dated
dormant `O_EVTONLY` reference-query miss; exact serial and final full workspace
runs pass without changing its assertion. The final relocated bundle/resource gate and 11 verifier tests pass. The
isolated report-only v5 socket gate passes 7 + restart + 7 with no cache attempt.
The final default history Accessibility/RSS lane passes 461 traversals with zero
misses and maximum RSS 80,288 KiB; the existing owned-child gate with the
service-failed Home fixture passes 457 traversals with zero misses and maximum
RSS 85,296 KiB. These are fixture results. Artifact `e8faf7a` is pushed and passes
[exact-head macOS CI](https://github.com/IndelibleVivi/unlinger/actions/runs/39448729783).

Generation 46 passed 7 + report-only restart + 7 schema-v5 socket checks, then
actually rolled back to generation 45. Its exact old CLI/daemon reopened SQLite
v14 healthy, quiescent, unarmed and report-only. The unchanged candidate freshly
installed as generation 47 and repeated those checks before acceptance and
separate restoration of the existing enforce policy. Generation 47 is healthy
`ReadyEnforce`, armed to its exact generation with a fresh epoch, zero service
problems and no rollback lease. Both installed binaries match the startup-volume
candidate; PATH resolves to the exact generation-47 CLI. Both uv and Node were
available in the candidate/restart acceptance readbacks. The following normal
observation returned `unsupported` for both producer-discovery lanes, with no new
maintenance attempt. At that checkpoint the daemon remained healthy, but a
successful generation-47 normal Node maintenance result had not yet been
observed. No manual cache operation, timer override or live-cache injection was
used; the native temporary entry was retained.

A bounded observer on 2026-10-20/21 completed 76 readbacks across 19m47s:
all retained the same exact healthy/ready/enforce generation-47 instance, with
zero read errors. The same exact App process remained present in all 76 sparse
identity/resource samples, maximum RSS 42,496 KiB and sampled CPU 10.0%. These
are bounded polling measurements, not a repeated AX stress gate or multi-day
acceptance. The safe temporary entry retained its identity and metadata; no new
Node automatic deletion result was observed during that bounded window. Its
transient producer-discovery refusal remains unexplained; it is distinct from
the later normal completion and must not itself be reported as successful
maintenance.

The matching App is installed, with strict signature/resource verification and
the displaced bundle preserved. Ordinary-window Home/History/Settings, current
protected-session roster/detail and the native notification popup were read and
exercised across the installed boundary. The App also exported a valid schema-v5
diagnostic document to an owner-private 0600 file without private path content.
The current History empty state matches read-only SQLite: no cleanup detail or
cleanup-impact rows remain within the 14-day detail retention window; lifetime
impact separately retains the historical two proved reclaims / 16 processes /
256,671,744 estimated memory bytes. Missing terminal details are not recreated
from that aggregate. Notifications remain denied, and the ad-hoc login item is
off/unregistered. No new browser-process or Chrome-clone removal is claimed at
this checkpoint. Historical installed results below keep their own dates.

## Historical Node compile-cache extension (2026-10-09; installed and activated)

The owner approved the exact Node exception to producer-native maintenance,
including source/App/CLI integration, transactional installation and restoration
of the existing automatic enforce policy. Source SQLite v14 adds independent
Node PREPARED/latest-result authority to the unchanged uv and retired-npm
namespaces. Only Node 26.7.0 on Apple Silicon and the current-user Darwin default
version/V8-tag bucket are admitted; frozen format-validated regular files can be
unlinked, while directories, custom locations, other buckets and unknown/user
content stay protected. The shared cache worker retains independent weekly
clocks and sticky token/epoch cancellation; uncertain delivery fails closed.

The accepted reference is generation 45 / SQLite v14, built from `346daa9` with
[exact-head macOS CI](https://github.com/IndelibleVivi/unlinger/actions/runs/37890561746).
Generation 44 passed v5 socket checks before and after report-only restart
(7 + 7), then actually rolled back to generation 41: its exact old CLI/daemon
reopened the restored SQLite-v13 database healthy, quiescent, unarmed and
report-only. The unchanged candidate freshly installed as generation 45,
repeated 7 + restart + 7 checks, was accepted and then separately restored to
healthy `ReadyEnforce`. Installed binaries match the staged candidate; PATH
resolves to the exact generation-45 CLI. No rollback lease remains. Node and uv
discovery are available.

Normal scheduling completed generation 45's first Node maintenance at
2026-10-09 14:19:44 +08: **26,090 successful file unlinks / 343,549,504 validated
logical bytes** (about 327.6 MiB). No manual prune, timer override or live-cache
injection was used. SQLite-v14 `attempt_json`, the actual v5 response and CLI
overview agree exactly; the PREPARED token is settled and read-only SQLite
`quick_check` returns `ok`. All 88 readbacks across 17m31s were healthy, ready
and enforce, continuing for more than 90 seconds after completion. The normal
LaunchServices App remained the same exact process in all 88 sparse samples,
maximum RSS 14,112 KiB and sampled CPU 10.5%; these are polling/lifetime samples,
not visual/AX or multi-day acceptance. Node's own counts do not become uv/native
or browser impact, and logical bytes do not prove physical APFS reclaim.

Focused Node tests pass 14 cases, with the real-producer case separately opt-in:
its test-owned cache was removed while the retained Node child continued a
subsequent import, then a fresh offline child regenerated cache. Fresh source
checks pass format, strict workspace clippy, 434 Rust tests (16 explicit opt-in
ignored), release workspace build and source-only doctor/dry-run. Doctor observed
512 processes and dry-run 514, each with four unavailable executable identities;
this is not complete ambient coverage. Swift checks pass 110 tests in 19 suites.
Release bundling includes relocated resource verification. The isolated SQLite-v14
report-only socket gate passes 7 + restart + 7, with independent uv/Node
observations and no maintenance attempt. The unlocked-console history/Accessibility
gate passes 463 traversals with zero misses, maximum RSS 104,704 KiB and final
RSS 28,144 KiB. This is fixture evidence, not installed or Node Home interaction.
The existing architecture source/export was regenerated and visually checked.
The optional v5 Node field keeps its own counters separate from uv/native
accounting and browser impact; old v5 and v4/v3 behavior is retained.
The first candidate `48d9b72` passed macOS CI and generation 42 passed
installed socket checks before and after restart (7 + 7), but its shared producer
queries returned unsupported/unavailable despite exact local versions. Real
rollback restored generation 41 and SQLite v13. Restoring the established
inherited-cwd query path in `3378e14` passed local checks and
[exact-head CI](https://github.com/IndelibleVivi/unlinger/actions/runs/37887891913).
Installed generation 43 then restored uv discovery to available and passed
socket/restart checks (7 + 7), followed by another actual rollback to 41/v13.

Node producer discovery also worked, but the implementation's extra 64 MiB
per-file cutoff blocked the whole bucket on one 106,039,836-byte native file.
A read-only check confirmed that file's exact magic, payload length and CRC.
The approved family has no separate size-based deletion meaning; source now
uses the existing 2 GiB whole-scan budget and fixed 64 KiB CRC buffer for all
validated files, retaining the 120-second deadline. Tests cover a valid file
larger than 64 MiB, incorrect CRC beyond the first buffer and aggregate-budget
refusal. This final artifact passes the source checks, installed transaction and
bounded normal automatic result above; sustained reliability remains unverified.

During the first rollback, generation 41 re-entered terminal `Failed` with
`SystemIoFailure` / extended code 4874 (WAL shared-memory sizing). The startup
volume had about 227 MiB available; a temporary database on that volume completed
32 transactions without error, so the cause is not established. The following
43-to-41 rollback again reopened v13 healthy/report-only; that is a point result,
not a root-cause repair. The new packaged App is installed with the prior bundle
preserved and its LaunchServices process remains alive. Desktop interaction is
not yet accepted despite an unlocked console: the desktop transport cannot
acquire a current window from the menu-only process. A source workspace run hit
the previously observed owned-inode
kernel query failure; the exact test passed alone, then full workspace/release
passed without weakening assertions. [Safety](SAFETY.md#node-compile-cache-exception)
owns the exact boundary.

## Recovery and packaged App resource repair (2026-10-09; generation-41 checkpoint)

The installed generation-39 daemon was found terminal `Failed`, loaded but
unhealthy and report-only, with `history SQLite failed: unable to open database
file`. Its retained log also contained disk I/O failures. A read-only check of
the live SQLite-v13 database returned `ok`; this does not explain the earlier
failure. No reliable first-failure timestamp is available. Under the owner's
repair authorization, the exact generation CLI's transactional `set-mode
report-only` replaced the failed instance, then `set-mode enforce` restored the
existing policy with a fresh instance/epoch. That recovery preceded the upgrade
below; the recovery checkpoint is accepted generation 41.

Normal scheduling then completed a new native uv maintenance attempt at
2026-10-09 10:25:37 +08: 20,267 entries and 399,402,598 native logical bytes.
Fifty readbacks remained healthy/ready/enforce through the attempt and its
following observation. No manual prune, timer override or cache injection was
used. Logical accounting does not prove physical APFS reclaim. There is no new
browser-process or Chrome-clone deletion claim in this recovery.

The installed App independently reproduced an immediate resource-accessor
crash. Its localization bundle existed under `Contents/Resources`, while the
generated SwiftPM accessor searched the App root and an obsolete temporary
build path. Current source gives `UnlingerResources` sole ownership of runtime
resource selection: packaged Apps use their own `Contents/Resources` and never
fall back to a development copy; unbundled development/test executables retain
SwiftPM lookup. Packaged fixtures likewise use only the shipped resources.
The bundle gate relocates the assembled App and runs its resource-only entry
before AppKit, IPC, preferences or notifications. It checks real English/Chinese
copy and v3/v4/v5 fixtures rather than only checking copied files.

Fresh App checks pass 107 Swift tests in 19 suites, 11 Python verifier tests and
release bundling, including relocated executable resource verification. The
isolated socket gate initially timed out waiting for a daemon executed from the
removable checkout volume. A byte-identical startup-volume copy became ready,
and the canonical smoke now stages that owned copy before its two report-only
passes; both passes executed all seven socket tests and proved no cache attempt.
This does not attribute the earlier live SQLite fault to that launch boundary.

Fresh backend checks pass format, strict workspace clippy, 417 Rust tests
(15 opt-in ignored), release workspace build, source-only doctor and nonmutating
dry-run. Doctor inspected 507 processes and dry-run inspected 527; both retained
four unavailable executable identities, so ambient coverage is not complete.
The resource-repair commit `356b703` also passes
[exact-head macOS CI](https://github.com/IndelibleVivi/unlinger/actions/runs/37875581886).
The final source candidate `c867da7` passes
[exact-head macOS CI](https://github.com/IndelibleVivi/unlinger/actions/runs/37876551588).

Generation 40 passed installed schema-v5 socket checks before and after restart
(7 + 7), then actually rolled back to generation 39. The old generation-39
binary reopened the restored SQLite-v13 database healthy, ready, report-only
and unarmed. The unchanged artifact was freshly installed as generation 41,
which repeated restart/socket checks (7 + 7), was accepted and restored to the
existing enforce policy with a fresh epoch. It is healthy `ReadyEnforce`, has no
rollback lease and has zero service problems. Both installed binaries match the
staged release artifacts; PATH resolves to the exact generation-41 CLI. A
read-only SQLite-v13 quick check returns `ok`. No generation-41 cleanup result
is inferred from the earlier same-day generation-39 maintenance result.

The resource-repaired schema-v5 App is installed; previous App bundles remain
recoverable. The installed resource-only check passes and its exact owned
process stayed running through 300 one-second samples, maximum RSS 89,552 KiB
and maximum sampled CPU 29.9%. This is a bounded startup/polling observation.
That directly spawned guard child later exited with system-recorded status 0
and no new crash report. A normal LaunchServices launch remained running after
the separate observer exited; this checks desktop launch lifetime without
turning the guard's child lifetime into an App crash claim.
The console remained locked, so Accessibility, visual interaction, notification
delivery and login-item behavior are not accepted by this run. Fresh App ledger
authorization remains `denied`. The intermittent historical layout trigger is
still unresolved, though the eager Home path is now installed.

Current Chrome clone observation retains three candidates / 4,498,870,591
logical bytes with incomplete references and no automatic eligibility. No
browser-process or clone deletion occurred in this upgrade. Broader cache
families require individual admission: the owner subsequently approved the
Node exception recorded above. Whole-target Cargo clean does not establish
narrow incremental retention authority.

An isolated store investigation did not reproduce the SQLite failure under
normal WAL/SHM permissions, read-only hot-WAL preflight, held writer locks or
concurrent reads/writes. No speculative store retry/refactor was added. The
exact cause remains unknown. The installed path-free primary/extended
SQLite diagnostics now support the next discriminating recurrence observation; terminal
fail-close and uncertain-delivery protection remain unchanged.

## Source-only safety and responsiveness repair (2026-09-25; historical source verification)

At this dated source-only checkpoint, the repairs addressed bounded authority,
responsiveness, diagnostics and App layout without widening eligibility or
touching generation 39. They are now installed in generation 41; the following
verification and unresolved field observations retain their September dates.

The uv path now binds native spawn to the exact durable PREPARED attempt. The
parent writes its attempt token, enforcement epoch, proved cache-root
device/inode and producer binding to a private inherited capability pipe, and
passes the already-open no-follow root directory descriptor to the hidden
supervisor. The supervisor rejects the old raw hidden argv form, revalidates the
root descriptor and producer binding, and refuses to open uv if the parent pipe
is already at EOF. The native process changes directory through the inherited
descriptor and uses relative `--cache-dir .`, so replacing the root pathname
cannot redirect this run. The producer pathname receives one final binding
check immediately before spawn. This is a structural accidental-bypass and
path-replacement boundary for the ordinary same-user product contract; it is
not a hostile same-UID authentication or filesystem sandbox.

Chrome clone storage maintenance now runs on one single-flight background
worker, so status/Pause and ordinary process reconciliation do not wait for a
recursive directory walk. PREPARED and an exact in-memory token/epoch owner are
published under the lifecycle lock before deletion; filesystem work and the
immediate rescan run after that lock is released. Pause, Disarm, BeginDrain and
fail-close latch cancellation on the exact owner, and Resume cannot resurrect
it. The descriptor-relative remover checks that lease before each new candidate
and recursive entry. Cancellation after a partial effect therefore stops later
segments, rescans the real remainder and settles a typed `failed` result with
truthful counts. Exact-owner RAII releases the volatile activity even if the
worker unwinds; durable recovery then records `delivery_unknown` and the daemon
fails closed. Focused tests prove prompt Pause, sticky cancellation, old-token
isolation, blocked-worker main-lane progress, panic recovery and both zero- and
one-candidate partial cancellation.

SQLite errors now include path-free primary and extended SQLite codes in
addition to the library detail. This improves future incident evidence; it does
not identify or repair the unresolved generation-36 `unable to open database
file` root cause.

The App's small bounded Home section list is now an eager `VStack`, removing the
exact Home `LazyVStack`/`LazySubviewPlacements` path captured in the earlier
installed hang. The Accessibility smoke can select an existing fixture/route
without changing its exact-child ownership or limits. The default
history-stress lane passed 462 traversals with zero transient misses, maximum
RSS 74,432 KiB and final RSS 23,376 KiB. A separate 120-second Home lane stayed
below 51,392 KiB through 292 successful tree reads, then failed on the required
fourth consecutive Accessibility child-tree miss. It is not a passing Home
result or installed acceptance, and the intermittent installed trigger remains
unverified.

Final source verification passes Rust format, strict workspace clippy, 428 Rust
tests with 15 explicit opt-in lanes excluded, release workspace build, the
source-only doctor and nonmutating dry-run, and all 12 separately invoked native
uv tests. Focused results include 16 daemon-binary tests, 23 storage-residue
tests, eight ControlPlane tests and the SQLite diagnostic regression. Doctor and
dry-run each inspected 469 current-user processes with zero unreadable,
argument or descriptor gaps and three unavailable executable identities, so
they do not claim complete ambient coverage. The App passes 104 Swift tests / 18
suites, release bundling, two isolated report-only schema-v5 socket passes (7 +
7), and the default Accessibility lane described above. None of this source work
installed, restarted, paused, disarmed, drained or otherwise mutated generation
39, the installed App or the real user uv cache.

## Installed uv maintenance (2026-09-21)

The reference service is accepted generation 39 / SQLite v13, healthy
`ReadyEnforce`, with no rollback lease; PATH resolves to its exact CLI. The
installed daemon and schema-v5 App come from `89001f5`, whose
[exact-head CI](https://github.com/IndelibleVivi/unlinger/actions/runs/35523671670)
passed. uv and uvx were updated through the native updater from 0.11.19 to exact
0.11.20. This owner-authorized upgrade includes ordinary native cache maintenance;
it is not a release or multi-day reliability claim.

Generation 38 passed report-only App/socket checks before and after restart
(7 + 7), actually rolled back to generation 36 reopening its restored SQLite v11
database, and was freshly installed as generation 39. Generation 39 repeated
report-only socket and restart checks (7 + 7), then was accepted and armed. Both
installed binaries were compared with the staged release artifacts. Previous App
bundles and a private pre-upgrade database backup remain recoverable.

The first `d383667` candidate (generation 37) exposed a real admission defect:
uv's normal 0666 `.lock` was refused. A focused regression first reproduced that
failure, then passed after admitting this native mode while retaining owner,
regular-file, single-link and non-writable-directory protections. The native
availability test now initializes a test-owned cache through real uv. Generation
37 also completed restart and real v13→v11 rollback; it performed no cache
mutation. Generation 39's first live-cache attempt ran at the normal 15-minute
opportunity and settled `completed` at 2026-09-21 01:08:06 +08. PREPARED and
completion were 890 ms apart. Native accounting reported 708 removed entries
and 32,610,713 rounded logical bytes; SQLite and fresh frontend readback agree.
The independently observed archive bucket changed from eight entries to six,
and the cache-environment bucket from one to zero. Those observations do not
replace native accounting or establish physical APFS reclaim. No manual prune,
timer override or synthetic injection into the live cache was used.

Across 27 readbacks spanning 792 seconds, the daemon remained healthy enforce.
The final App process's sampled RSS peaked at 36,272 KiB and sampled CPU at 6.7%
(most samples were 0%). This is a bounded sparse observation, not a continuous
resource guard, proof of active-task preservation in this field attempt or
multi-day acceptance. Active-holder safety remains covered by the isolated
native lock/interoperability tests. No fresh generation-39 browser-process or
Chrome-clone deletion is claimed.

Fresh validation for `89001f5`: format, strict workspace clippy, 405 Rust tests
(14 explicit opt-in ignored), all 11 uv native isolated tests, release build,
source-only doctor/nonmutating dry-run, 104 Swift tests, bundle verification and
isolated socket checks (7 + 7). Doctor still reports five unavailable executable
identities, so it is not complete ambient process coverage. The rebuilt fixture
App passed 448 Accessibility traversals without transient misses, max RSS
96,608 KiB and final 10,752 KiB. Unknown browser-coverage copy now resolves in
both languages instead of exposing an internal key.

Two field issues remain causally unresolved. Before upgrade, generation 36 was
terminal `Failed`, disarmed to report-only, with `unable to open database file`.
The live v11 database passed a read-only quick check; its new rollback instance
was healthy. Neither observation explains the earlier failure. During the
upgrade, the `d383667` App process became unresponsive at about one CPU core;
a sample showed SwiftUI lazy-layout/AttributeGraph work and 588.8 MiB physical
footprint. That exact process was terminated and the final App relaunched. Its
subsequent observations are separate evidence; the fixture history gate does
not prove that the live Home-window trigger is fixed.

## uv maintenance source verification (2026-09-21, before installation)

The owner admitted uv-owned rebuildable cached execution environments under the
native producer lock. Source replaces npm verify with exact uv `0.11.20 cache
prune` at the default cache, runs cache work on one owned background worker,
and separates activity from process observation. Proved pre-mutation lock
refusal is deferred for 15 minutes; ordinary outcomes keep the weekly clock.
SQLite v13 preserves v12 npm evidence separately and starts fresh uv authority.
Frontend v5 adds optional uv facts without reusing the old npm field.

The old npm admission is withdrawn: an exact npm `11.19.0` / cacache `20.0.4`
local-Git probe reproduced verify exit 0 with all-zero counters while it removed
a live fetch directory, followed by install exit 254 / ENOENT. The control
installed successfully. The old source test results did not establish concurrent
safety. pnpm `11.21.0` remains unadmitted for the equivalent active-Git-fetch
failure. [Safety](SAFETY.md#native-tool-cache-maintenance) owns both probes.

Final local source checks pass: Rust format, strict workspace clippy, 404 Rust
tests (14 explicit opt-in tests excluded), release workspace build, source-only
doctor and nonmutating dry-run; 104 Swift tests / 18 suites, release App bundle
verification and two isolated report-only socket passes (7 + 7). The socket gate
also proves SQLite v13 cache observations exist without any PREPARED maintenance.
The 11 explicitly invoked uv native tests pass against a staged official 0.11.20
binary and test-owned roots, including real package import/rebuild, source-revision
retention, both lock directions, 0.11.19 holder interoperability, external canaries
and binary supervisor EOF settlement. A worker-panic regression proves activity
release, joined-worker unknown recovery and fail-close. No native test is evidence
of real-user-cache or installed-service acceptance.

The fixture Accessibility recheck passed 464 traversals with zero transient
misses and RSS max 72,624 KiB, final 16,272 KiB. It precedes the final cache-copy
correction, which subsequently passed Swift tests and release bundling; host
ownership and popup implementation were unchanged. An initial AX run failed on
repeated child-tree reads without exceeding its RSS limit. One intermediate
workspace run also missed an owned open-file reference in an unchanged Darwin
kernel test; its exact recheck and final default-parallel workspace passed without
weakened assertions. Neither transient failure's cause is established. Doctor and
dry-run each observed five unavailable executable identities; these checks do not
claim complete ambient coverage.

Those source checks changed no real user cache or installed tool/service/App.
The subsequent authorized installed operation is recorded above; reference facts
below retain their dated scope.

## Installed SQLite v11 storage cleanup result authority (2026-09-17)

The installed v11 baseline raised the daemon database authority from SQLite v10 to v11 and
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

## Earlier source and installed evidence (superseded by the 2026-09-21 record)

| Surface | Observed truth |
| --- | --- |
| Runtime implementation | `e26297b`: SQLite v10 / Playwright `0.6.0` composition described above; integrated on `main` by tree-identical ancestry merge `d2a685b` |
| Current storage authority | Source cache work adds SQLite v13; installed generation 36 retains SQLite v11 with a durable auto-cleanup result path: PREPARED before any clone-directory deletion, one terminal result committed in the same transaction as the real latest residue observation, `delivery_unknown` recovery that never infers attribution from a later count, and an additive optional schema-v5 `storage_cleanup_result` with typed outcome, timestamps and aggregate counts/logical bytes only; no v11 live deletion result has occurred yet |
| Subsequent test correction | `f22e08eb3410050b380eaee7b84c4ccda2a3ad1a`: bounded post-release offline-lock tests; production locking/timeouts unchanged |
| Baseline remote verification | [CI 34163955192](https://github.com/IndelibleVivi/unlinger/actions/runs/34163955192) passed for `b70bc94`; [CI 34165331792](https://github.com/IndelibleVivi/unlinger/actions/runs/34165331792) passed for `f22e08e`, including default-parallel Rust tests, release build, Swift tests and App bundling |
| Current reader preparation | Published source-preview candidate `de1a9c3`: bilingual reader guides, licensed material scopes, current architecture and safe demo teardown; [exact CI 34170593229](https://github.com/IndelibleVivi/unlinger/actions/runs/34170593229) passed all steps |
| Current source verification | The SQLite-v11/result/App tranche at `a9355fa` passes workspace Rust format, strict clippy, tests and release build, healthy source-only doctor, nonmutating dry-run, 101 Swift tests / 17 suites, release bundle verification, two isolated report-only v5 socket passes (7 + 7) and [exact-head CI 35203895092](https://github.com/IndelibleVivi/unlinger/actions/runs/35203895092). The subsequent clear-detail localization and root-title repairs again pass 101 Swift tests / 17 suites and release bundling; their final exact-head CI is tracked separately from installed acceptance. Earlier exact-head evidence remains: Chrome aggregate candidate-reference correction `516d483` [CI 35106242999](https://github.com/IndelibleVivi/unlinger/actions/runs/35106242999), App repair `c17e60f` [CI 35118609063](https://github.com/IndelibleVivi/unlinger/actions/runs/35118609063), mapped-vnode/rollback correction `b2ada65` [CI 35130438688](https://github.com/IndelibleVivi/unlinger/actions/runs/35130438688), per-candidate cleanup `c272942` [CI 35138131207](https://github.com/IndelibleVivi/unlinger/actions/runs/35138131207), and installed `.app.bundle` correction `a8aed45` [CI 35142430097](https://github.com/IndelibleVivi/unlinger/actions/runs/35142430097) |
| App memory repair | The installed App retains `c17e60f`'s native-popup/equal-publication/closed-window repair and adds the v11 cleanup-result UI, rendered clear-detail localization fix and root navigation-title restoration. Its final canonical Settings/native-popup process passed 180 one-second installed samples at 27,728 KiB start, 15,488 KiB final and 37,520 KiB maximum during repeated Accessibility traversal; the earlier exact `c17e60f` process also passed 2,400 samples and later reached 18h45m at 12,992 KiB. The exact intermittent trigger and multi-day acceptance remain open |
| Maintainer's reference service | Accepted generation 36, healthy and quiescent `ReadyEnforce` with no pending candidate lease; generation 35 proved v10→v11 migration, restart and real rollback to generation 34 reopening v10 before the exact `a9355fa` backend artifact was freshly installed, accepted and armed as generation 36. The earlier generation-34 production observation removed eight stale clones and retained the one live Chrome candidate; no v11 deletion result exists yet |
| Reference protocols/persistence | Source: SQLite v13 with optional cache-maintenance result; installed generation 36: SQLite v11. Both retain operator v1 and frontend v5/v4/v3 with optional storage-cleanup result; historical v2 rejected |
| Reference App | Current ad-hoc-signed schema-v5 App with cleanup-result presentation, native popup memory repair, bilingual clear-detail copy and root-title restoration; 101 Swift tests, strict bundle verification, rendered live UI navigation and the 180-sample installed Settings/Accessibility RSS guard passed. It is neither Developer ID signed nor notarized; displaced `c17e60f`, pre-localization `a9355fa`, pre-root-title, `628d822`, `e26297b` and `016ca58` bundles remain recoverable local siblings |
| Policy | Playwright `0.6.0`, agent-browser/Puppeteer `0.4.0`; process-only; every artifact flag false |
| Publication | [Repository public](https://github.com/IndelibleVivi/unlinger); source-available under SUL-1.0 + CC BY-NC-SA 4.0; anonymous API and reader/license/diagram access verified; no GitHub Release |

The source daemon still defaults to report-only. The reference generation 36 was
separately accepted and explicitly armed after its v11 transaction proof; its
current `ReadyEnforce` runtime remains distinct from the default and from
repository publication. The earlier generation-34 clone-cleanup result is not
a generation-36 durable-result field point, multi-day evidence or broad ambient
process-cleanup acceptance.

## Historical installed transaction evidence (through 2026-09-17)

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
exposed the new `session` surface; it later resolved to generation 36; the current target is recorded above.

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

## Historical controlled evidence (generation 23 and source publication)

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

- Installed generation 47 shows the independently retained two proved browser
  reclaims while History is empty. Read-only SQLite contains no retained cleanup
  events or cleanup-impact detail rows; those details have passed their 14-day
  retention window. A lifetime aggregate is not enough to reconstruct receipts.
  The October-17 settlement fallback and October-20 same-millisecond ordering/
  anchoring repairs are installed; the fallback can supply a row only while the
  daemon still returns that exact recent settlement. The October-17 baseline
  rejected the old mapper with 13 issues; current Swift validation passes 131
  tests, and ordinary Home/History navigation is accepted at its observed scope.
- The original generation-17 SQLite disk-I/O failure and generation-45 `CannotOpen` causes remain unproved. Exact-instance containment/recovery and later transactional replacement succeeded; a later healthy database check does not establish the original cause.
- The command wrapper does not integrate every Codex App host or browser tool automatically. The optional session-owner primitive introduced in v10 and retained by source v14 has no supported automatic Codex adapter. A bounded feasibility review found that later hooks/observers and turn-level lifecycle events cannot supply the current contract's exact parent/child activation plus thread-to-controller/session binding; a real adapter needs Codex host support. Exact CLI/browser compatibility, lifetime/client proof and all ordinary gates remain required; unregistered, active-owner, reused, unsupported or unverified controllers stay protected.
- Chrome clone observation accepts the actual `.app.bundle` shape and no-follow framework links. Historical installed generation 34 evaluated candidates independently across two production-interval observations, removed eight stable unreferenced clones and retained the one candidate containing the continuously running ordinary Chrome main. Generation 47 retains that gate; a current-generation clone deletion has not been field-verified. Its readback includes an inherited September-22 terminal result from generation 39; it is not a generation-47 action. The generation-34 point does not prove generation-36 result persistence, multi-day behavior, all future Chrome clone shapes or physical APFS reclaim equal to the logical byte reduction.
- All artifact admission is disabled. The dormant DAP engine still has a quarantine-after-crash recovery gap and a final pathname-swap TOCTOU. Native pathname-reference tests also intermittently returned no reference for an owned open ordinary or `O_EVTONLY` descriptor under parallel execution; exact serial tests passed, and the cause is unresolved. The active process path does not use that query. [Safety](SAFETY.md) owns these boundaries.
- The old zero-deadline offline-lock test failed because a concurrent fork can inherit an `O_CLOEXEC` descriptor until exec. A deterministic owned-child probe established that cause; `f22e08e` retains held-lock denial and gives post-release acquisition its existing bounded wait. The final exact-head CI passed. A later local full workspace run reproduced the separate dormant native-query failures above; no assertions were weakened.
- An earlier source fixture passed 460 Accessibility-tree probes with zero read failures and an external 22,208 KiB final RSS sample. The installed `628d822` App's earlier 300-sample/one-tree acceptance was superseded by the later 19-GB recurrence. The installed `c17e60f` native-popup repair then passed 2,400 one-second RSS samples with a 22,192 KiB maximum and the same process later reached 18h45m at 12,992 KiB. The September-17 installed cleanup-result/localization/root-title App passed rendered Home/History/Settings navigation and 180 one-second Settings/native-popup samples with a 37,520 KiB maximum during repeated Accessibility traversal. These bounded observations still do not establish the exact intermittent trigger, multi-day App behavior, packaged notifications or every menu organizer/display arrangement.
- No Intel/universal verification, signed/notarized distribution, automatic update path or public release is claimed. Recognition of agent-browser/Puppeteer is not controlled field acceptance.

## Publication preparation

The published candidate `de1a9c3` scan covered 216 text files in both working/index views and its exact committed tree, plus 786 reachable historical text blobs. Its scope-repeated findings were reviewed as synthetic test/fixture paths and notification event-token code. The automated scan skipped two current and six historical binary assets; they were separately inspected as generated source images and derived icons, including metadata and historical provenance. This is a bounded review, not a secrets-free certificate.

A historical frontend handoff was reviewed in full as a technical interface/implementation guide, without private chats, local personal paths or account data. It was removed from the current tree in `df161af`. No history rewrite or separate repository is needed on the inspected evidence. [Provenance](PROVENANCE.md) records material boundaries without copying private working notes.

The owner selected the license scope in [LICENSING.md](../LICENSING.md), confirmed rights authority and authorized public visibility after candidate checks. The existing independent repository was made public after exact-source CI passed; provider readback and unauthenticated API/raw-file requests confirmed access. No repository fork, history rewrite, release tag or binary upload was created. The owner-approved source/content licenses are present. Reader documentation and the architecture export do not imply a released App or completed binary-distribution review. See [acceptance levels](PRE_V0_1_ACCEPTANCE.md), [support](SUPPORT.md) and the [support matrix](support-matrix.v1.json) for claim vocabulary.
