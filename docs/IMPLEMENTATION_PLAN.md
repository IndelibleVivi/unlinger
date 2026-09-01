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
| STORE-01 | bounded redacted durable authority | SQLite v6 migration; stable public event/recovery tokens; atomic observation batch; durable policy revision; namespace-aware typed mutation receipts with 14-day proof window | installed v6 migration intentionally blocked; max-retention performance | source implemented; focused migration/atomicity tests |
| IPC-01 | local single-shot operator + App protocols | schema v1 operator/lifecycle preserved; schema v3 strict App surface; v2 typed rejection; bounded 0600 socket/eight workers; shared action policy; honest roster freshness | installed v3 endpoint absent | source/isolated target |
| APP-01 | trustworthy private native client | strict DTOs, cancellable phase-aware transport, pre-send durable journal, no-resend reconciliation, coalesced refresh, typed detail, local diagnostics, stable row identity, bilingual UI | owner visual acceptance and installed integration | source implemented; 62-test gate + bundle verified |
| APP-02 | bounded local notifications and routing | off/attention/attention-and-reclaims, first-refresh baseline, seen suppression, durable claim-before-schedule, health episodes, current OS permission, compact shared-router window | best-effort bounded polling is not gap-free; packaged owner observation | source implemented; fake-scheduler verified |
| APP-03 | basic private usability | Settings/About, App and connected daemon versions, explicit quit semantics, `SMAppService.mainApp` menu-client login item with typed failures | ad-hoc login-item behavior may depend on macOS bundle policy; owner observation | source implemented |
| DIST-01 | transactional install/update/rollback and release | generation 9 historically installed through sealed immutable generations and remains report-only; source release build/bundle gates exist | acceptance-scoped v5 rollback lease, installed v3 acceptance, universal, signing/notarization, packaging/update | installed v3 blocked; release open |
| PRIV-01 | local/private and truthful publication boundary | no runtime account/network/telemetry; redacted IPC/store; owner-private App state; public-safe notification routes; private continuity outside Git | repeat the boundary review before any future visibility change | implemented; tracked candidate/diff scan verified |
| TEST-01 | executable pre-v0.1 contract | Rust migration/policy/IPC fixtures; Swift transport/state/detail/notification/settings tests; active v3 fixture bundle gate; isolated report-only smoke script | owner-only live/installed/release gates remain separate | local full gate verified: Rust 268 passed/2 owner-only ignored; Swift 62; isolated smoke 6 + restart 6; exact-head macOS CI green |

## Pre-v0.1 tranche dependency order

1. **Protocol/authority fence — implemented:** server accepts schemas 1 and 3, rejects 2; App emits v3 only; lifecycle remains v1-only.
2. **SQLite v6 authority — implemented:** event tokens, receipt namespace, durable revision, typed receipts, atomic migration and observation batch.
3. **Shared policy and control linearization — implemented:** projection and v3 commit call the same policy under the lifecycle/status serialization boundary; replay precedes current lifecycle denial.
4. **Strict Swift transport/state — implemented:** exact DTOs, phase-aware uncertainty, pre-send journal, status-only restart reconciliation and concurrency generations.
5. **UI/notifications/usability — implemented:** truthful readiness/freshness/detail/export, stable identity, bounded notifications/routing and menu-client login item.
6. **Contracts/support — implemented:** active v3 docs/fixtures, acceptance levels, support matrix and installed blocker.
7. **Source + isolated gate — verified locally and at remote exact head:** full Rust/Swift/build/bundle/doctor/dry-run passed; the same isolated report-only database passed six live-socket tests before and after daemon restart; macOS CI repeated the repository format/lint/test/release/frontend/bundle gate.
8. **Installed v3 lane — deliberately closed:** first implement an acceptance-scoped rollback lease and prove a real generation-9 v5 database rollback/open. Never arm in that lane.

## Scope/order notes

- **Schema v2 is historical, not a compatibility endpoint.** It never reached the installed service or a public release, so v3 replaces it without a downgrade path while its fixtures remain as audit evidence.
- **V3 receipt authority is App-scoped.** Schema-v1 CLI/service ordinary behavior remains compatible; lifecycle commands never enter v3.
- **Notifications are bounded and best-effort.** Duplicate avoidance is preferred: durable claim occurs before one OS schedule attempt. A mode change never replays suppressed history. Ambiguous count/CPU/RSS/age/pressure has no notification authority.
- **Artifact admission remains DAP-only.** The current tranche does not change either P2 residual or widen automatic deletion.
- **The original install target is gated, not silently dropped.** SQLite v6 creates a real rollback incompatibility with generation 9; source/isolated work may push with that blocker recorded.

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

Owner-only live CfT harnesses remain ignored. Passing these commands permits only **pre-v0.1 source candidate — isolated report-only verified**. Multi-day dogfood, ambient real eligibility, current-candidate narrow enforcement, both artifact P2 decisions, Intel/universal, signing/notarization/distribution and public alpha remain separate gates.
