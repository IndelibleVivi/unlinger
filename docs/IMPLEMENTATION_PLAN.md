# Unlinger 0.1 implementation plan

## Authority and status language

- Product/technical scope: [`SPEC.md`](SPEC.md), still a working draft.
- This file owns implementation coverage and tranche status; it may change technique, not product meaning or safety thresholds.
- Volatile source/installed/field/release truth: [`current-state.md`](current-state.md).
- Claim levels: [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md).
- Exact support/evidence vocabulary: [`SUPPORT.md`](SUPPORT.md) and Rust-validated [`support-matrix.v1.json`](support-matrix.v1.json).

`implemented` means the canonical source path exists with focused evidence. `verified` names the exact gate that passed. Source-complete, isolated, installed, activated, field-accepted and released are separate states.

## Coverage ledger

| ID | Intended outcome | Current implementation/evidence | Remaining boundary | Status |
| --- | --- | --- | --- | --- |
| DET-01..03 | deterministic evidence, hard gates and confidence states | native/redacted fixture corpus, shared rule-pack sessionizer, exact CfT allowlist (`151.0.7922.34`, `152.0.7977.42`), controller fail-closed, two observations and durable abandonment grace | CfT 152 controlled cleanup evidence; broader real corpus and controller/version evidence | source implemented; fixture verified |
| RUN-01 | startup/periodic/exit/wake/pressure observation | coalesced native hints plus periodic fallback; each trigger begins with a fresh snapshot; pressure never lowers gates | real sleep/wake, sustained pressure and watched-exit field evidence | source implemented |
| RUN-02 | frozen TERM→rescan→exact KILL→postscan→revival | exact identity revalidation, PREPARED-before-signal journal, delivery-unknown recovery, owned-child tests, historical generation-9/13 points, and generation-15 process-only full-timing pass | ambient real eligible incident, broader families and chaos | exact source mechanics + current controlled point; ambient open |
| RUN-03 | remove only admitted low-risk artifact | exact DAP engine, targeted pathname-reference + complete argv proof, quarantine and durable artifact journal remain tested but dormant; source `0.4.0` and installed `0.3.0` packs disable admission | crash-after-quarantine and final same-UID swap P2s; sockets/PID files; explicit future re-enable decision | implemented capability, disabled by source and installed policy |
| STORE-01 | bounded redacted durable authority | source SQLite v7 coalesces equivalent observations into spans, preserves cleanup impacts independently of observation retention, maintains lifetime aggregates plus at least 14 days of cleanup detail, and retains existing event/recovery/mutation authority | installed migration/rollback transaction and long-running retention performance | source v7 migration/atomicity and full workspace gate verified; installed generation 15 remains v6 |
| IPC-01 | local single-shot operator + App protocols | schema v1 operator/lifecycle preserved; schema v5 strict App surface adds impact/residue/span facts; v4 overview responses and v3 existing responses preserved; v2 typed rejection; bounded 0600 socket/eight workers; shared action policy | installed v5 transaction and broader dogfood observation | source v5/v4/v3 plus two-pass isolated v5 socket gate verified; installed generation 15 remains v4/v3 |
| APP-01 | trustworthy private native client | strict v5 DTOs, cancellable phase-aware transport, pre-send durable journal, no-resend reconciliation, stored snapshot/history presentation rebuilt once per state transition and published only on value change, exact snapshot coherence, product/version-aware browser phases/session/coverage/settlement plus durable impact and observe-only storage-residue presentation, cleanup-outcome history index, server-owned observation spans in detail, stable SwiftUI identity, one outer Accessibility element per history link, bilingual VoiceOver copy, transient popover root lifetime, one 340×420 content contract across hosts | installed v5 acceptance; multi-day App dogfood; packaged notification observation; external menu-host placement remains environment-owned | source 81-test suite, bundle and 456-probe RSS/Accessibility gate verified; installed v4 App evidence remains valid only for v4 |
| APP-02 | bounded local notifications and routing | off/attention/attention-and-reclaims, first-refresh baseline, seen suppression, durable claim-before-schedule, health episodes, current OS permission, reusable AppKit window with its own router, independent popover router and consuming menu-to-window route handoff | best-effort bounded polling is not gap-free; packaged notification observation | source implemented; host isolation, notification routing and window host verified |
| APP-03 | basic private usability | Settings/About, App and connected daemon versions, explicit quit semantics, `SMAppService.mainApp` menu-client login item with typed failures | ad-hoc login-item behavior may depend on macOS bundle policy; owner observation | source implemented |
| DIST-01 | transactional install/update/rollback and release | immutable generations plus acceptance-scoped prior manifest/plist/database lease; durable crash-replayable rollback intent; schema-preserving cross-generation containment; executable-material validation; recoverable report-only restart; redacted public service JSON; real generation-9 and generation-14→13 rollback/open | sustained dogfood, universal, signing/notarization, packaging/update | generation 15 accepted after fresh rollback/reinstall; no lease remains |
| PRIV-01 | local/private and truthful publication boundary | no runtime account/network/telemetry; redacted IPC/store; owner-private App state; public-safe notification routes; private continuity outside Git | repeat the boundary review before any future visibility change | implemented; tracked candidate/diff scan verified |
| TEST-01 | executable pre-v0.1 contract | Rust migration/policy/IPC/service-lease fixtures; Swift transport/state/browser-projection/impact/residue/history/detail/notification/settings/router/AppKit-host tests; 50-incident fixture plus real Accessibility-tree external-RSS gate; explicit v5/v4/v3 fixture bundle gate; isolated and installed report-only live-socket checks | v5 installed stress and enforcement/release gates remain owner-only | full local source/build/bundle and isolated v5 gates pass; prior installed v4 evidence remains recorded separately |
| BGX-01 | browser-ghost product surface without changing daemon authority | canonical Swift browser overview projection, phase/session/coverage/settlement/detail UI, accessibility copy and rendered fixture QA | superseded by the installed BGX-2 atomic daemon projection | historical source tranche; installed successor active |
| BGX-02 | daemon-owned atomic browser truth and generated compatibility support | schema-v4 contract, v3 transition endpoint, atomic overview, typed compatibility, rule-generated catalog, v4 App and CLI projection | broader dogfood observation | source-complete, exact-head verified and installed on generation 15 |
| IMPACT-01 | make completed work visible without trusting noisy history | SQLite v7 independent impact rows/aggregate, exact recent-settlement lookup, observation spans, cleanup-only App history, v5 impact UI and partial-backfill disclosure | installed migration/rollback and longer retention observation | source-complete and isolated-v5 verified; no installed claim |
| RESIDUE-01 | identify exact Chrome code-sign clone residue without unsafe deletion | bounded exact-root scanner, typed latest observation, logical-size/APFS caveat, v5 observe-only UI; no path persistence and no cleanup authority | complete live-reference proof and a separate owner decision before any cleanup path | source-complete observe-only; synthetic filesystem/full workspace gates passed; no live deletion path |
| ENF-01 | current process-only enforcement policy | source `0.4.0` admits exact CfT 151 and 152 while producing no artifact candidate; installed generation 15 remains `0.3.0`/CfT 151 only; managed verifier requires zero artifact receipt/journal rows | CfT 152 field evidence, installed v0.4 transaction, multi-day dogfood and ambient ordinary eligible incident | source allowlist fixture-verified; installed generation-15 level-4 evidence unchanged |
| BGX-03 | owner-triggered task-finished action with real report-only semantics | not implemented | owner decision between unavailable report-only action and persisted observe-only intent; field evidence remains separate | blocked on product decision, not part of BGX-2 |
| BGX-04 | distributable private/public alpha | current ad-hoc private bundle only | license/rights decision, universal build, signing, notarization, packaging, update and publication gates | planned; not authorized for release work |

## Pre-v0.1 tranche dependency order

1. **Protocol/authority fence — implemented:** source server accepts schemas 1, 3, 4 and 5, rejects 2; source App emits v5 only; v4 and v3 retain their existing meaning; lifecycle remains v1-only.
2. **SQLite v7 authority — implemented:** v6 authority plus observation spans, independent cleanup impact/detail, lifetime aggregates, typed storage-residue observation and atomic migration/backfill.
3. **Shared policy and control linearization — implemented:** projection and v3 commit call the same policy under the lifecycle/status serialization boundary; replay precedes current lifecycle denial.
4. **Strict Swift transport/state — implemented:** exact DTOs, phase-aware uncertainty, pre-send journal, status-only restart reconciliation and concurrency generations.
5. **Browser-first UI/notifications/usability — implemented:** one daemon-owned atomic browser projection, presentation-only Swift mapping cached once per state transition, truthful readiness/freshness, typed session compatibility/coverage/settlement/detail copy, stable identity, bilingual accessibility, independent per-host navigation with consuming handoff, bounded notifications/routing and menu-client login item.
6. **Contracts/support — implemented:** active v5 plus transitional v4 and legacy-compatible v3 docs/fixtures, acceptance levels, source/evidence support matrix, and separate volatile installed truth.
7. **Source + isolated gate — BGX-2 verified locally and at exact remote head:** full Rust/Swift/build/bundle/doctor/dry-run passed; the same isolated report-only database passed seven live-socket tests before and after daemon restart. Exact-head macOS `backend` run `33608082841` repeated the repository format/lint/test/release/frontend/bundle gate for implementation head `19c58e5`.
8. **Installed migration lane — verified historically:** exact-head CI, real generation-9 v5 rollback/open, generation-12 reinstall, installed App and daemon/App restart reconciliation passed. Generation 12 was accepted; generation 13 was later installed and accepted, but its own lease was not executed before acceptance.
9. **BGX-2 atomic product contract — installed:** frontend schema v4 preserves the transitional v3 endpoint, moves browser phase/compatibility/coverage/settlement authority into one daemon snapshot, generates the support catalog from embedded rules, and powers the installed App plus `unlinger browser status`.
10. **Process-only activation — level 4 complete:** exact-head CI; generation-14 report-only install/restart and real rollback to generation 13; generation-15 fresh reinstall/restart/App checks/accept; full-timing managed process-only cleanup with zero artifact actions; final containment; explicit arm; later stable sweep.
11. **Impact + residue source tranche — source-complete and isolated verified:** v5/v7 makes successful work durable and visible, admits exact CfT 152 in source, compresses observation noise, and detects the exact Chrome code-sign clone family as observe-only. Full local source/build/bundle, two-pass isolated v5 socket and final exact-source 456-probe Accessibility/RSS gates passed; implementation head `53142bd` also passed exact-head macOS CI run `33872302035`. Installed generation 15/App remain unchanged.
12. **BGX-3 owner action — decision-gated:** do not implement a misleading report-only action until the owner chooses whether it is unavailable or records observe-only intent. Either choice must retain the installed report-only floor and requires its own acceptance evidence.
13. **BGX-4 distribution — future lane:** signing, notarization, packaging, updating, licensing and publication remain distinct owner-controlled gates after product/field acceptance.

## Scope/order notes

- **Schema v2 is historical, not a compatibility endpoint.** It never reached the installed service or a public release, so v3 replaces it without a downgrade path while its fixtures remain as audit evidence.
- **Frontend receipt authority is App-scoped.** Schema-v3/v4/v5 mutation behavior shares the durable receipt contract; schema-v1 CLI/service ordinary behavior remains compatible; lifecycle commands never enter frontend schemas.
- **Notifications are bounded and best-effort.** Duplicate avoidance is preferred: durable claim occurs before one OS schedule attempt. A mode change never replays suppressed history. Ambiguous count/CPU/RSS/age/pressure has no notification authority.
- **BGX-1 changed presentation only and is now historical source groundwork.** BGX-2 is the authorized successor source tranche: it may change the frontend protocol and rule-derived public projection, but it must not widen detection or cleanup admission, replace the installed service, or change report-only mode.
- **V4 and v3 remain real compatibility boundaries.** Existing requests retain their response schema and meaning; the source App emits v5 only, schema v2 remains rejected, and lifecycle commands remain absent from every frontend schema.
- **BGX-3 remains separately decision-gated.** BGX-2 must not invent task-finished behavior or smuggle activation authority into the new overview payload.
- **Artifact capability remains DAP-only, but current admission is none.** Source `0.4.0` and installed `0.3.0` packs suppress every artifact candidate while preserving the dormant implementation and both P2 residuals for a separate decision. Storage-residue observation is a separate typed family and has no delete path.
- **The installed claim is narrow.** Generation 15 is active process-only after a full rollback/reinstall and one controlled field point. Multi-day observation, an ambient ordinary eligible incident and release decisions remain separate gates.

## Verification and completion

Narrow tests run first. A level-2 claim then requires:

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
scripts/accessibility-memory-smoke.sh
```

Owner-only live CfT harnesses remain ignored. Installed generation 15 and its controlled process-only evidence permit **private enforcement candidate for the exact admitted point**. Multi-day dogfood, an ambient ordinary eligible incident, artifact re-enable, Intel/universal, signing/notarization/distribution and public alpha remain separate gates.
