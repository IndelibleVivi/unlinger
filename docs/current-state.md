# Current state

**Updated:** 2026-09-02

**Programme:** Unlinger 0.1

**Source:** private pre-v0.1 schema-v4 process-only candidate plus incident-centric browser-history/detail UI and bounded IPC-worker lifetime hardening; policy-version `0.3.0` preserves exact process TERM/KILL eligibility and disables runtime-artifact admission in every embedded pack; the last full Rust/Swift/release/bundle/isolated-smoke gate predates the IPC-worker change, whose focused Swift suite passes

**Remote:** browser-history/detail UI head `267e3f34bf715219ee54071b9671d7f9ba9f9a94` passed exact-head private macOS `backend` run `33636283051`, including formatting, strict clippy, workspace tests, release build, native frontend tests and bundle. The installed generation-15 daemon remains sourced from process-only implementation head `17cb6a5035f979dc1849c7165038e365d8a202e1`, which passed run `33621944450`

**Installed runtime:** accepted generation 15 from process-only implementation head `17cb6a5`, frontend schemas v4/v3 plus operator schema v1, SQLite v6, healthy `ReadyEnforce`; exact armed generation 15 and a non-empty enforcement epoch; no candidate rollback lease remains

**Activation:** process-only enforcement is active on generation 15 after exact-head CI, candidate-A install/restart/real rollback to generation 13, candidate-B reinstall/restart/accept, a full-timing managed process-only field pass, final report-only containment, and a later stable post-arm sweep. The schema-v4 packaged App from UI head `267e3f3` remains installed but is deliberately stopped after an owner-observed approximately 9 GB App-memory spike during intensive rendered QA. Process eligibility remains limited to exact admitted CfT `151.0.7922.34`; newer observed CfT `152.0.7977.42` sessions remain `PROTECTED`. Runtime-artifact eligibility is disabled.

**Highest claim currently permitted:** **private enforcement candidate for the exact admitted point** (acceptance level 4). Ambient process-only enforcement is active, but it has not been accepted as private v0.1, multi-day dogfood, broad field support or release evidence.

## Source truth

The backend source now uses SQLite schema v6 and accepts frontend schemas v4 and 3 while preserving schema-v1 CLI/service compatibility. Schema v4 adds the daemon-owned atomic browser product projection; schema v3 remains a transition endpoint whose existing responses retain their schema and meaning but rejects `browser_overview` without downgrade. Schema v2 remains historical and is rejected with typed `unsupported_schema`. The source App emits v4 only and never falls back to v3 or v1.

All embedded rule packs are now policy version `0.3.0`. Their exact process eligibility, protection gates, timing, revalidation, TERM/KILL sequence, revival handling and durable process-action journal are unchanged. Every pack sets `devtools_active_port = false`, so analysis produces no runtime-artifact candidate. The managed acceptance verifier requires both terminal receipt and durable attempt journal to contain zero artifact actions and records a separate process-only proof artifact. The existing DAP engine and its focused tests remain dormant for a future explicitly authorized residual-resolution lane.

Frontend ordinary mutations carry a public-safe namespace token plus canonical UUID. The daemon serializes lifecycle state with a `BEGIN IMMEDIATE` store transaction, replays an exact stored receipt before current lifecycle policy, re-evaluates authoritative durable facts through one shared public-action policy, applies state, advances a durable cleanup-policy revision and stores a typed `applied | no_change | rejected` receipt atomically. Same-ID canonical replay is idempotent; conflicting reuse fails; old-namespace missing requests cannot apply. Receipt pruning rotates namespace atomically and retains a minimum 14-day reconciliation window.

SQLite v6 also assigns stable opaque public event tokens, retains storage-recovery identity across clean restarts, and commits each observation cycle's history as one batch before publishing the roster. The public roster distinguishes `never_observed`, `scan_in_progress`, `current` and `stale_after_failure`, with optional time/token when no real snapshot exists. Last scan uses cycle completion rather than cycle start.

BGX-2 adds typed browser product/version compatibility to the transient `IncidentReport` only; serde skips it for persisted observations, so the database schema remains v6 and history does not retain app-bundle facts. `RuleSet` generates the public family/product/admitted-version/action-level catalog from embedded version policies with a readable support revision. `ControlPlane` captures status and roster under one in-memory boundary, then `public_ipc` produces the canonical phase, current session summaries, typed coverage, attention/protection and recent settlement. Untrusted readiness/freshness/time becomes `unknown`; trusted phase precedence is server-owned. Settlement joins exact cleanup event token and earlier event ID, including same-millisecond histories, rather than depending on a bounded App history page.

The native App under `apps/UnlingerApp` now has:

- strict schema-v4 DTO/envelope decoding, explicit v3 fixture compatibility and a narrow exact v1 incompatibility parser;
- phase-aware, single-attempt, cancellable socket I/O on a shared four-operation worker queue, with one explicit autorelease pool drained per request instead of three new OS threads per five-second refresh;
- an owner-private crash-durable pending-mutation journal written before connect/send;
- one global unresolved mutation lock and startup/status-only reconciliation without resend;
- one pure `BrowserOverviewMapper` that maps daemon-owned v4 product truth into localized copy/display shapes without recomputing phase, scanning evidence or joining history;
- one separate `BrowserHistoryMapper` that groups the bounded history index by incident, enriches current rows from the coherent browser snapshot, and collapses only consecutive same-family/same-state observations in detail while retaining cleanup receipts and state changes;
- one browser overview request per coalesced refresh; transport loss retains prior rows as stale context but forces an App-local unknown phase;
- browser-first overview phases, typed family/session compatibility, closed-set coverage explanations, saved protections, exact settlement and a rule-generated support catalog, without raw evidence IDs or unsupported global process/RSS totals;
- product/version-aware browser-context detail from a coherent current row, retained observation or exact joined settlement, with named gate results and absent estimates remaining absent;
- coalesced refresh generations, polling-session isolation, stale browser-snapshot retention and typed incident-detail failures;
- local-view diagnostics results with required `document_schema_version` and semantic-lossless JSON export;
- stable event/action identity, including artifact-only action groups;
- bilingual browser copy and VoiceOver labels with language-matched value formatting, plus Settings/About/Quit surfaces, shared notification routing and menu-client-only `SMAppService.mainApp` launch at login;
- a direct AppKit `@main` that strongly retains the single delegate for the blocking App run loop, with AppKit-owned `NSStatusItem`/`NSPopover` and reusable ordinary-window lifecycles around one shared SwiftUI state/router and one shared `340 × 420` content size; the packaged App remains a regular foreground/Dock app, closing the last ordinary window does not terminate it, and Dock reopen plus popover detail retain explicit window/Back/current-route controls;
- a release bundler that builds in an internal temporary SwiftPM scratch path, removes removable-volume toolchain rpaths, and rejects packaged executables that retain removable-volume resource fallbacks or loader paths;
- bounded local notifications: `off`, default `attention`, or `attention_and_reclaims`; first trusted refresh baselines retained tokens, suppressed events remain seen, and durable claim precedes one schedule attempt.

Notifications are best-effort local projections over bounded polling. They have no sound, are quiet in foreground, use public-safe route data, and never treat an App mutation response fault as a backend cleanup event. Runtime remains local-only with no account, telemetry, cloud sync or normal-operation network behavior.

The service source extends its durable install transaction through `CandidateReadyReportOnly`, `AcceptanceInProgress`, `RollbackInProgress` and `Accepted`. `DatabaseBackedUp` is a conservative restore boundary because a crash may occur after either candidate selection file is published but before `CandidateSelected` is durable. Rollback validates the immutable backup's recorded schema and the sealed prior daemon/CLI/manifest, persists replay intent before physical mutation, accepts only transaction-owned candidate/prior/mixed selection cuts, restores the same snapshot and republishes the prior report-only selection until convergence. Offline containment directly updates the compatible managed-lifecycle row and verifies that the database `user_version` is unchanged; it no longer opens a restored prior database through the current migrating `HistoryStore::open` path.

`restart-report-only` still requires exact candidate selection, a report-only manifest and executable rollback material, but it no longer requires a stopped, PID-less or terminal-failed candidate to already be healthy before replacement. Acceptance remains stricter: exact healthy, quiescent ReadyReportOnly plus report-only desired mode and valid rollback material. Service-command JSON now serializes a dedicated schema-v1 public projection that retains lifecycle/lease truth while excluding PID, instance ID, absolute paths and raw errors.

The owner-only managed harness consumes that public lifecycle projection and follows each mutation with one separate read-only schema-v1 daemon-status call for instance, generation and enforcement-epoch proof. Machine-local database/socket paths come from `LocalPaths`; the harness does not restore private fields to service-command JSON and never resends a lifecycle mutation after uncertain delivery.

## Installed evidence

The level-3 rollback lease was exercised rather than merely inspected: an earlier candidate restored generation 9 and its SQLite-v5 snapshot, and the exact generation-9 CLI/daemon reopened that database healthy at `ReadyReportOnly`. Generation 12 was then reinstalled, its restart path passed, and the owner explicitly accepted it on 2026-09-02, durably retiring the generation-9 lease before another install.

The exact BGX-2 release binaries were installed transactionally as generation 13 in report-only mode. Readback proved healthy `ReadyReportOnly`, exact launchd/IPC/generation/binary/permission agreement and a new rollback lease to generation 12. The matching schema-v4 ad-hoc-signed App bundle passed strict signature, plist and byte-for-byte build-bundle comparison, replaced the prior App recoverably, and launched from the installed Applications location. The owner then accepted generation 13, retiring its generation-12 lease. That generation-13 lease was validated but was **not actually executed before acceptance**; the process-only replacement below closes that gap rather than borrowing the earlier generation-12→9 proof.

The first generation-13 managed field attempt stopped before launching CfT because the harness still expected the pre-BGX-2 private service JSON. Service readback confirmed healthy report-only containment. The harness was corrected to join the public service projection with one independent read-only daemon status and locally derived service paths; its focused unit suite passed before the live retry.

The corrected full-timing run passed in 378.73 seconds. Incident `inc-e2cabdde69de66dd` contained eight exact CfT 151 processes; the durable receipt and journal agreed on eight TERM actions followed by one exact-root KILL, zero survivors, two revival checks, `52,838,400` estimated bytes reclaimed and one DAP `removed` disposition. Same-generation restart changed daemon instance and enforcement epoch without duplicating journal state. The 0700 profile and eight 0600 proof artifacts remain retained outside Git. The harness ended healthy report-only; the owner-authorized postflight then re-armed the accepted generation 13.

Post-arm stable readback showed generation 13 healthy `ReadyEnforce`, exact armed generation and a fresh epoch, no cleanup in progress, and zero attention items. The v4 atomic overview was `current/protected`: it retained the exact generation-13 settlement and protected all three observed CfT `152.0.7977.42` sessions with `protection.browser_version_unsupported`. After independent review identified that process authorization did not separately accept the two artifact-removal P2s, generation 13 was contained without restart at healthy `ReadyReportOnly` before replacement work began.

Process-only implementation head `17cb6a5` passed exact-head CI before installation. Candidate A installed as generation 14 report-only with a fresh lease to generation 13, passed exact readiness and `restart-report-only` with a new daemon instance, and exposed the `0.3.0` support revision. An initial concurrent seven-test installed App pass encountered one read-only `browser_overview` delivery uncertainty during a scan while the other six tests passed; the narrowed quiescent overview test then passed, and all seven tests passed individually after restart. Generation 14 then executed `rollback-candidate`. The exact generation-13 CLI/daemon reopened the restored SQLite-v6 store healthy, quiescent, unarmed and report-only, and its browser projection reverted to the old `0.2.0` support revision, proving physical old-generation restoration.

The same exact-head binaries reinstalled as candidate B generation 15 with a fresh lease to generation 13. Exact readiness, same-generation report-only restart with a new instance, the `0.3.0` browser projection and all seven individually serialized installed App checks passed. The matching ad-hoc-signed v4 App bundle was installed recoverably, passed signature/Info.plist/source-bundle equality checks, and launched. Generation 15 was then accepted from a stable quiescent report-only projection, retiring its lease only after the candidate-A rollback proof and candidate-B repeat checks.

The generation-15 managed process-only field run passed on 2026-09-02 in 384.21 seconds. One isolated eight-member CfT 151 tree produced eight TERM actions followed by one exact KILL, zero survivors, two completed revival checks and `54,984,704` estimated reclaimed bytes. The analyzer admitted zero runtime-artifact candidates; terminal receipt and durable journal each contained zero artifact actions; `DevToolsActivePort` remained present; the public settlement reports `artifact_outcome: not_applicable` and `overall_outcome: cleared`. Same-generation restart changed the daemon instance without duplicating the journal, and harness teardown returned generation 15 to healthy report-only. The retained 0700 profile and 0600 proof files stay outside Git.

After a later full completed report-only sweep remained current, quiescent and free of attention or eligible incidents, generation 15 was explicitly armed. Immediate and next-sweep readback both showed the same healthy `ReadyEnforce` instance, exact armed generation 15, non-empty enforcement epoch, SQLite v6, no scan/cleanup/drain or cleanup block, and zero attention. The v4 overview retained the process-only settlement, exposed `rules:agent-browser@0.3.0|playwright@0.3.0|puppeteer@0.3.0`, and protected all four then-observed CfT `152.0.7977.42` sessions. The canonical user Applications location now retains only the installed current App; prior visible sibling bundles were moved out recoverably.

The current packaged App fixes the prior 360-point SwiftUI root inside 340-point AppKit hosts, so overview, history and detail content no longer clip horizontally. Browser history now presents one incident-centric row rather than one row per periodic observation, and the detail timeline summarizes consecutive observations while keeping cleanup/state changes distinct. The canonical user Applications location contains only the current App; older visible sibling bundles were removed recoverably. The direct AppKit Dock/window fallback remains the supported reachability path if an external menu host cannot place the status item. The previously diagnosed Thaw/macOS hosted-menu compatibility gap remains unresolved and must not be reported as fixed.

During repeated installed-App rendered and accessibility QA, the owner observed the `Unlinger` App at approximately 9 GB memory and closed it. Post-close process readback found no App process; the accepted generation-15 daemon remained healthy `ReadyEnforce` at a small resident footprint. No App memory-pressure/crash diagnostic was retained, so the exact allocator or host interaction is unproved. Source inspection found that each five-second refresh created three fresh Foundation threads and lacked an explicit per-request autorelease pool. The source candidate now replaces that churn with one shared four-operation queue and drains one pool per request. This is focused source hardening, not proof that the 9 GB incident is resolved: the installed App remains stopped and unchanged until a separately authorized, bounded memory soak accepts a rebuilt candidate.

## Safety and field truth

Automatic process eligibility remains limited to controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34`, plus every existing identity, isolation, durable abandonment, stability and protection gate. Agent-browser, Playwright and Puppeteer are recognized/classified families; only the exact controllerless CfT point is automatically eligible. Controller-bearing, headed/attached, standard/shared profile, wrong/mixed/unknown version and incomplete identity stay protected.

Current source automatic artifact cleanup is disabled: all `0.3.0` packs produce zero runtime-artifact candidates and actions. The dormant exact `DevToolsActivePort` path retains two P2 residuals: a crash after canonical-to-quarantine rename can strand the exact quarantine entry, and the final path revalidation-to-`unlinkat` interval retains a same-UID swap TOCTOU.

Generations 9 and 13 each prove one historical owner-approved managed full-timing Playwright-style CfT process/artifact/restart point. Generation 15 separately proves the current process-only installed transaction: exact process cleanup/revival/restart with zero artifact admission/actions and final containment. The later explicit arm plus one completed stable sweep proves current activation state only; it does not establish an ambient real eligible incident, multi-day zero-false-positive dogfood, or broader families/versions.

## Verification truth

The current process-only source gate passed locally on 2026-09-02:

- `cargo fmt --all -- --check`, strict workspace clippy and the release workspace build passed;
- `cargo test --workspace` passed 287 tests; the two owner-only live CfT tests remained ignored;
- source-only doctor inspected 552/552 listed processes with zero unreadable, argument-unavailable or descriptor-unavailable entries, 17 executable-identity-unavailable entries, all three signature packs at `0.3.0`, `healthy: true` and no errors;
- dry-run inspected 552/552 listed processes with the same complete readable/argument/descriptor coverage, 17 executable-identity-unavailable entries and seven `PROTECTED` CfT 152 automation sessions; no eligible incident, runtime-artifact candidate or signal action was produced;
- `swift test` passed 75 tests in 16 suites, including strict v4 fixture decoding, daemon-phase preservation, typed compatibility/unknown copy, incident-grouped history, consecutive-observation collapse with cleanup retention, direct settlement projection, single-overview refresh, stale-snapshot containment, all seven product phases, bilingual VoiceOver copy, shared AppKit host geometry and existing transport/mutation/notification contracts;
- after the memory incident, the focused 75-test Swift suite passed again with the bounded four-operation IPC queue and per-request autorelease-pool change; the App was not launched for this source-only check;
- the release App bundle assembled from its internal scratch path, packaged explicit v4 and v3 fixture directories, validated localizations, passed ad-hoc signature verification, and rejected source-volume resource fallback and removable-volume loader paths;
- the isolated smoke passed seven live-socket suite tests, including the v4 atomic browser overview, restarted the source daemon over the same private temporary SQLite database, then passed the same seven tests again. It verified effective report-only mode, durable receipt replay and owner-private temp/database/socket/lock modes, and removed only its owned temporary root; and
- fresh installed Simplified Chinese rendered QA exercised overview → browser history → current browser session detail at the fixed 340 × 420 content size. It confirmed unclipped horizontal geometry, current product/version context, one incident row for repeated observations, a 56-observation grouped timeline entry, a readable `3 / 7` safety-check summary and expanded named pass/block results. Popover uses the same tested size contract, but external menu-host placement remains unresolved and packaged notification delivery was not re-observed.

Browser-history/detail UI head `267e3f3` passed private GitHub exact-head macOS `backend` run `33636283051`, including formatting, strict clippy, workspace tests, release workspace build, native frontend tests and native frontend bundle. Process-only implementation head `17cb6a5` remains separately verified by run `33621944450`; earlier BGX-2 implementation head `19c58e5` and documentation head `a52ed6b` remain verified by runs `33608082841` and `33608496345`.

The reproducible commands were:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release --workspace
cargo run -p unlinger-cli -- doctor --source-only --json
cargo run -p unlinger-cli -- scan --dry-run --json
cd apps/UnlingerApp
swift test
scripts/bundle.sh
scripts/pre-v0.1-smoke.sh
```

Ordinary workspace tests still ignore both owner-only CfT harnesses. Separately, the explicitly acknowledged generation-15 `managed_cft_fieldlab` ran at full production timing and passed the exact managed process-only/restart contract, including zero artifact actions and final report-only containment. The accepted installed generation was then armed and remained healthy through a later completed sweep; this operational evidence is not part of ordinary source validation.

For the process-only implementation, strict red/green tests first failed on the old `0.2.0` pack/artifact expectation and the historical mandatory-artifact managed receipt contract. After the policy and verifier change, `unlinger-rules` passes 16/16, the nonignored `managed_cft_fieldlab` suite passes 8/8 with its live case still ignored, and the full local plus exact-head CI gates pass. Installed rollback, field and activation evidence are recorded separately above.

## Open gates

- multi-day ambient dogfood and an ordinary real eligible incident;
- owner observation of the installed schema-v4 App's final visual and packaged notification behavior;
- bounded rebuilt-App memory soak without Computer Use/accessibility-tree stress, followed separately by any deliberate stress reproduction; until then the approximately 9 GB installed-App incident remains unresolved and the App stays stopped;
- Thaw/macOS hosted-menu compatibility for the still-unresolved status item; this external presentation gap does not authorize further private-default or identity workarounds in Unlinger;
- exact field evidence before admitting CfT `152.0.7977.42` or any other version/family expansion;
- resolution or explicit product acceptance of both artifact P2 residuals before any future artifact re-enable;
- broader family/version/controller field evidence, chaos, sleep/wake and sustained-pressure evidence;
- Intel/universal build evidence;
- Developer ID signing, notarization, packaging/update/rollback and distribution;
- public-safe repository/license/rights decision, public alpha and public release.

## Repository visibility boundary

The GitHub repository remains private and has no license file. The final clean-tree public-candidate scan covered 191 text files in each candidate view and 564 reachable historical text blobs. Its 25 scope-repeated signals were manually reviewed as synthetic machine-path fixtures/tests or notification business-token field names rather than private paths or credentials. Six tracked binary icon assets are outside automated text coverage and still require an owner rights confirmation.

Field evidence and the text scan now exist. A visibility change still requires the owner to select the exact code/document/asset license scope, confirm binary-asset redistribution rights, and approve bilingual reader documentation plus a publication-grade architecture artifact. The current Mermaid document remains explicitly labelled a private working diagram. None of these preparation facts changes the highest product/release claim above.

See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md), [`SUPPORT.md`](SUPPORT.md), and [`support-matrix.v1.json`](support-matrix.v1.json) for the exact claim and support vocabulary.
