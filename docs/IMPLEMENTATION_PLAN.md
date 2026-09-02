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
| DET-01..03 | deterministic evidence, hard gates and confidence states | native/redacted fixture corpus, shared rule-pack sessionizer, exact CfT version gate, controller fail-closed, two observations and durable abandonment grace | broader real corpus and controller/version evidence | source implemented; fixture verified |
| RUN-01 | startup/periodic/exit/wake/pressure observation | coalesced native hints plus periodic fallback; each trigger begins with a fresh snapshot; pressure never lowers gates | real sleep/wake, sustained pressure and watched-exit field evidence | source implemented |
| RUN-02 | frozen TERM→rescan→exact KILL→postscan→revival | exact identity revalidation, PREPARED-before-signal journal, delivery-unknown recovery, owned-child tests and one historical generation-9 full-timing point | ambient real eligible incident, broader families and chaos | exact source mechanics + one historical controlled point; ambient open |
| RUN-03 | remove only admitted low-risk artifact | exact DAP identity, targeted pathname-reference + complete argv proof, quarantine and durable artifact journal | crash-after-quarantine and final same-UID swap P2s; sockets/PID files | partial by design |
| STORE-01 | bounded redacted durable authority | SQLite v6 migration; stable public event/recovery tokens; atomic observation batch; durable policy revision; namespace-aware typed mutation receipts with 14-day proof window | max-retention performance | installed generation 12 migrated under a retained v5 rollback lease; focused migration/atomicity tests |
| IPC-01 | local single-shot operator + App protocols | schema v1 operator/lifecycle preserved; schema v4 strict App surface with atomic browser overview; v3 transition responses preserved; v2 typed rejection; bounded 0600 socket/eight workers; shared action policy | broader dogfood observation | source v4/v3 verified locally and at exact remote head; installed v3 endpoint remains report-only |
| APP-01 | trustworthy private native client | strict DTOs, cancellable phase-aware transport, pre-send durable journal, no-resend reconciliation, one pure browser overview mapper, exact snapshot coherence, browser phases/session/coverage/settlement/detail projection, bilingual VoiceOver copy, direct AppKit process lifetime, AppKit-owned status popover/window, explicit popover Back and open-current-route action | browser-first payload not installed; external menu-host placement remains environment-owned | source implemented; 73-test/15-suite BGX-2 gate green; prior rendered fixture QA remains historical |
| APP-02 | bounded local notifications and routing | off/attention/attention-and-reclaims, first-refresh baseline, seen suppression, durable claim-before-schedule, health episodes, current OS permission, reusable AppKit window with shared router | best-effort bounded polling is not gap-free; packaged notification observation | source implemented; fake-scheduler and window host verified |
| APP-03 | basic private usability | Settings/About, App and connected daemon versions, explicit quit semantics, `SMAppService.mainApp` menu-client login item with typed failures | ad-hoc login-item behavior may depend on macOS bundle policy; owner observation | source implemented |
| DIST-01 | transactional install/update/rollback and release | immutable generations plus acceptance-scoped prior manifest/plist/v5 DB lease; durable crash-replayable rollback intent; schema-preserving cross-generation containment; executable-material validation; recoverable report-only restart; redacted public service JSON; real generation-9 rollback/open | sustained dogfood, another installed crash-cut drill, universal, signing/notarization, packaging/update | source regression tests green; installed report-only rollback path verified; generation-12 lease retained |
| PRIV-01 | local/private and truthful publication boundary | no runtime account/network/telemetry; redacted IPC/store; owner-private App state; public-safe notification routes; private continuity outside Git | repeat the boundary review before any future visibility change | implemented; tracked candidate/diff scan verified |
| TEST-01 | executable pre-v0.1 contract | Rust migration/policy/IPC/service-lease fixtures; Swift transport/state/browser-projection/detail/notification/settings/router/AppKit-host tests; explicit v4/v3 fixture bundle gate; isolated and installed report-only live-socket checks | owner-only enforcement/release gates remain separate | BGX-2 source gate verified: Rust 287 passed/2 owner-only ignored; Swift 73/15 suites; isolated v4 smoke 7 + restart 7 green; exact-head macOS CI green |
| BGX-01 | browser-ghost product surface without changing daemon authority | canonical Swift browser overview projection, phase/session/coverage/settlement/detail UI, accessibility copy and rendered fixture QA | superseded by the BGX-2 atomic daemon projection once v4 migrates the App | source verified at exact head; not installed |
| BGX-02 | daemon-owned atomic browser truth and generated compatibility support | schema-v4 contract, v3 transition endpoint, atomic overview, typed compatibility, rule-generated catalog, v4 App and CLI projection | any later install remains separate | source-complete and exact-head verified; not installed |
| BGX-03 | owner-triggered task-finished action with real report-only semantics | not implemented | owner decision between unavailable report-only action and persisted observe-only intent; field evidence remains separate | blocked on product decision, not part of BGX-2 |
| BGX-04 | distributable private/public alpha | current ad-hoc private bundle only | license/rights decision, universal build, signing, notarization, packaging, update and publication gates | planned; not authorized for release work |

## Pre-v0.1 tranche dependency order

1. **Protocol/authority fence — implemented:** source server accepts schemas 1, 3 and 4, rejects 2; source App emits v4 only; v3 retains its existing meaning; lifecycle remains v1-only.
2. **SQLite v6 authority — implemented:** event tokens, receipt namespace, durable revision, typed receipts, atomic migration and observation batch.
3. **Shared policy and control linearization — implemented:** projection and v3 commit call the same policy under the lifecycle/status serialization boundary; replay precedes current lifecycle denial.
4. **Strict Swift transport/state — implemented:** exact DTOs, phase-aware uncertainty, pre-send journal, status-only restart reconciliation and concurrency generations.
5. **Browser-first UI/notifications/usability — implemented:** one daemon-owned atomic browser projection, presentation-only Swift mapping, truthful readiness/freshness, typed session compatibility/coverage/settlement/detail copy, stable identity, bilingual accessibility, bounded notifications/routing and menu-client login item.
6. **Contracts/support — implemented:** active v4 plus transitional v3 docs/fixtures, acceptance levels, support matrix and installed blocker.
7. **Source + isolated gate — BGX-2 verified locally and at exact remote head:** full Rust/Swift/build/bundle/doctor/dry-run passed; the same isolated report-only database passed seven live-socket tests before and after daemon restart. Exact-head macOS `backend` run `33608082841` repeated the repository format/lint/test/release/frontend/bundle gate for implementation head `19c58e5`.
8. **Installed v3 lane — verified report-only:** exact-head CI, real generation-9 v5 rollback/open, generation-12 reinstall, installed App and daemon/App restart reconciliation passed. The acceptance lease remains pending for initial dogfood. Never arm in this lane.
9. **BGX-2 atomic product contract — source-complete and exact-head verified:** frontend schema v4 preserves the installed/transitional v3 endpoint, moves browser phase/compatibility/coverage/settlement authority into one daemon snapshot, generates the support catalog from embedded rules, and migrates the App plus `unlinger browser status` to that projection. Installation is not part of this tranche.
10. **BGX-3 owner action — decision-gated:** do not implement a misleading report-only action until the owner chooses whether it is unavailable or records observe-only intent. Either choice must retain the installed report-only floor and requires its own acceptance evidence.
11. **BGX-4 distribution — future lane:** signing, notarization, packaging, updating, licensing and publication remain distinct owner-controlled gates after product/field acceptance.

## Scope/order notes

- **Schema v2 is historical, not a compatibility endpoint.** It never reached the installed service or a public release, so v3 replaces it without a downgrade path while its fixtures remain as audit evidence.
- **Frontend receipt authority is App-scoped.** Schema-v3/v4 mutation behavior shares the durable receipt contract; schema-v1 CLI/service ordinary behavior remains compatible; lifecycle commands never enter frontend schemas.
- **Notifications are bounded and best-effort.** Duplicate avoidance is preferred: durable claim occurs before one OS schedule attempt. A mode change never replays suppressed history. Ambiguous count/CPU/RSS/age/pressure has no notification authority.
- **BGX-1 changed presentation only and is now historical source groundwork.** BGX-2 is the authorized successor source tranche: it may change the frontend protocol and rule-derived public projection, but it must not widen detection or cleanup admission, replace the installed service, or change report-only mode.
- **V3 remains a real transition boundary during BGX-2.** Existing v3 requests retain their response schema and meaning; the v4 App emits v4 only, schema v2 remains rejected, and lifecycle commands remain absent from every frontend schema.
- **BGX-3 remains separately decision-gated.** BGX-2 must not invent task-finished behavior or smuggle activation authority into the new overview payload.
- **Artifact admission remains DAP-only.** The current tranche does not change either P2 residual or widen automatic deletion.
- **The installed claim is narrow.** SQLite v6 creates a real rollback incompatibility with generation 9; the exact old-binary rollback/open and installed App runbook passed, but the retained lease, multi-day observation and future enforcement/release decisions remain separate gates.

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
```

Owner-only live CfT harnesses remain ignored. These source/isolated commands plus the completed installed runbook permit **pre-v0.1 installed report-only candidate** for generation 12. Multi-day dogfood, ambient real eligibility, current-candidate narrow enforcement, both artifact P2 decisions, Intel/universal, signing/notarization/distribution and public alpha remain separate gates.
