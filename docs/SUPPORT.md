# Support truth

Unlinger 不使用一个模糊的 “supported” 标签把识别、自动授权和 field evidence 混在一起。机器可读 authority 是 [`support-matrix.v1.json`](support-matrix.v1.json)，并由 `unlinger-rules` test 对照当前 embedded rule packs。它只记录 source capability 与累计 evidence；易变的 installed generation、active mode 与当前 lease 只由 [`current-state.md`](current-state.md) 记录。

## Platform

| Surface | Current truth |
| --- | --- |
| Source target | macOS 14+ |
| Verified development/controlled-field architecture | Apple silicon (`arm64`) |
| Intel (`x86_64`) | 未验证 |
| Universal binary | 未构建、未验证 |
| Signed/notarized/distributed build | 不存在 |

## Process-family matrix

| Family | Recognized | Deterministic classification | Automatic process eligibility | Always-protected shapes | Synthetic evidence | Controlled field evidence | Ambient evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `agent-browser` | yes | yes | controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34` or `152.0.7977.42`, plus every hard gate | controller-bearing, headed/attached, standard or persistent profile, unknown/mixed/wrong version, incomplete or contradictory identity | yes | no | no |
| `playwright` | yes | yes | exact two-point CfT allowlist; controllerless, task-owned CLI `1.63.0-alpha-2026-08-31`, or optional host-owned ordinary CLI `1.62.1`, always with every lifetime/client gate | unsupported, unregistered, active-owner or unverified controller; reused/unproved owner generation; all ordinary browser/profile/identity protections | yes | historical CfT 151 points; isolated task-owned CfT 151/152 with one TERM, both revival checks and unrelated-session protection; installed task-owned CfT 152 receipt and App impact (exact evidence in current-state). The new ordinary host-owned lane has source tests only | no |
| `puppeteer` | yes | yes | same exact controllerless two-point CfT allowlist and all hard gates | same protections | yes | no | no |

“Recognized” means the rule pack can reconstruct and explain a candidate. It does not authorize a signal. “Deterministic classification” means the shared sessionizer and hard gates produce a typed state for the available facts. Automatic eligibility is the much narrower intersection of exact product/version, controller absence or verified released task/host ownership, ephemeral isolated shape, complete identity, durable abandonment, two-observation stability, and every protection gate.

Unregistered or unverified controller-bearing sessions remain protected. The narrow source task-owned CLI lane is specified in [TASKS.md](TASKS.md). The optional host-owned lane retains the existing ordinary session name and requires path-free registry namespace, exact host owner lifetime and controller identity; no supported Codex/other-host adapter drives it automatically yet. In the App, an otherwise recognized ordinary session without that evidence reports the explicit owner-lifetime protection instead of a generic unknown reason. Ordinary Chrome, standard/shared profiles, headed/manual sessions, attached CDP sessions, other users, root/system processes, Unlinger and its ancestors are also always protected.

## Artifact matrix

| Artifact | Recognized | Automatic eligibility | Synthetic evidence | Controlled field evidence | Ambient evidence | Residuals |
| --- | --- | --- | --- | --- | --- | --- |
| exact `DevToolsActivePort` regular file | yes | **disabled in every source and installed pack**; process-only enforcement produces no candidate or action | yes, for the dormant engine | exact historical removals in generation-9 and generation-13 controlled runs | no | crash after canonical-to-quarantine rename may strand the exact quarantine entry; final revalidation-to-`unlinkat` retains a same-UID swap TOCTOU |
| Chrome `code_sign_clone` residue family | yes, exact current-user path shape | valid enforce evaluates each exact candidate independently; a candidate must span two 15-minute observations, have no matching-suffix cleanup Helper or executable/absolute-argv reference, and pass every lifecycle/filesystem gate; a referenced candidate stays protected without blocking stable unreferenced siblings, while incomplete global proof blocks all; an exact non-cleanup main executable inside a scanner-admitted `.app.bundle` candidate protects that candidate even without generic `.app` bundle facts | yes; temp fixtures cover full and subset deletion, report-only mixed projection, per-candidate stability, ordinary-Chrome non-blocking, exact/unrelated/malformed suffixes, executable/absolute-argv references, observed missing-bundle main, missing-bundle cleanup-helper failure, incomplete process proof, no-follow shapes, partial remover failure and unavailable rescan | generation 34 completed one production-timing installed result: 9→1 candidates and 13,273,958,043→1,474,884,227 logical bytes while the continuously running ordinary Chrome candidate remained protected | one bounded ordinary-machine result with Chrome continuously open; not multi-day | logical bytes are not a physical-reclaim guarantee; broader clone shapes and longer ambient evidence remain open |
| profiles/browser data/runtime directories | observed only as transient classification context | never | n/a | n/a | n/a | deletion is outside 0.1 |
| browser sockets and PID files | not automatically admitted | never | no | no | no | ownership/reference contract remains open |

The controlled DAP results repeat one historical exact admitted point across two installed generations. They do not establish current artifact authority, broad artifact safety, or removal of either P2 residual. Re-enabling any artifact flag requires a separate decision.

## Protocol and runtime skew

| Layer | Current truth |
| --- | --- |
| Rust CLI/service operator protocol | schema v1; lifecycle authority remains here |
| Native App source protocol | schema v5 only; requires daemon-owned atomic `browser_overview` with impact/residue/latest optional storage-cleanup result and consumes server-owned observation spans |
| Transitional frontend endpoint | schema v4 prior overview/response shapes retained without v5-only fields |
| Legacy-compatible frontend endpoint | schema v3 existing commands/response meaning retained; overview rejected without downgrade |
| Schema v2 | historical fixtures retained; superseded before installation; current server returns typed `unsupported_schema` |
| Installed generation | see [current-state](current-state.md) for exact generation, activation and rollback lease; current compatible runtime uses operator v1, frontend v5/v4/v3 and SQLite v10 |
| Installed App/v5 integration | matching schema-v5 ad-hoc-signed private App `c17e60f` installed and running; bundle identity/signature/plist and exact launch verified; its exact process passed a 2,400-second RSS guard with a 22,192 KiB maximum; fresh visual UI and packaged-notification observation remain unverified |
| Current source/installed implementation | both retain schema v5/v4/v3 plus operator v1, Playwright `0.6.0`, other packs `0.4.0`, exact CfT 151/152 process allowlist and disabled runtime-artifact flags; source uses SQLite v11 with durable path-free Chrome clone attempt/result authority, an additive optional v5 result summary and App presentation of that result, while installed generation 34/App `c17e60f` remain SQLite v10-era artifacts without the result authority/presentation; generation 34 contains mapped-vnode snapshot, transaction recovery and the exact `.app.bundle`-aware per-candidate clone gate from `a8aed45`, and is `ReadyEnforce` |

SQLite v10 has a candidate-specific v8 backup/rollback/open proof: generation 24 really rolled back to healthy, unarmed generation 23 with the old CLI/daemon reopening v8; the exact candidate freshly reinstalled as generation 25, repeated matching-App/socket checks before and after restart, and was accepted before arming. Earlier schema migrations remain historical evidence.

## Acceptance meaning

Synthetic verification proves code paths and counterexamples. Controlled field verification proves only the exact owner-approved case. Ambient verification requires ordinary long-running observation on the target machine without a harness-created incident. These evidence classes are not interchangeable.

The task-owned lane has isolated controlled process-only evidence for exact CfT 151 and 152, plus historical installed generation-23 CfT-152 cleanup and actual App impact readback. The host-owned ordinary lane is now installed with its fixture/store/IPC/CLI and migration proof, but it still has no automatic host adapter or ambient process-cleanup receipt. Unregistered, unsupported or unverified controllers remain protected. Generation 15 separately retains the historical complete managed process/restart/containment point; it is not substituted for current-generation cleanup evidence. The reference service is presently `ReadyEnforce`; generation 34 adds one successful bounded per-candidate clone cleanup while ordinary Chrome stayed open, but ordinary ambient process enforcement acceptance remains false. Multi-day dogfood, broader field evidence, Intel/universal, signing/notarization/distribution and public alpha remain open. See [current-state](current-state.md) for exact outcomes and [PRE_V0_1_ACCEPTANCE](PRE_V0_1_ACCEPTANCE.md) for claim levels.
