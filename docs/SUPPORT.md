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
| `agent-browser` | yes | yes | controllerless `com.google.chrome.for.testing` exactly `151.0.7922.34`, plus every hard gate | controller-bearing, headed/attached, standard or persistent profile, unknown/mixed/wrong version, incomplete or contradictory identity | yes | no | no |
| `playwright` | yes | yes | same exact controllerless CfT point and all hard gates | same protections | yes | owner-approved managed full-timing points on generations 9 and 13 | no |
| `puppeteer` | yes | yes | same exact controllerless CfT point and all hard gates | same protections | yes | no | no |

“Recognized” means the rule pack can reconstruct and explain a candidate. It does not authorize a signal. “Deterministic classification” means the shared sessionizer and hard gates produce a typed state for the available facts. Automatic eligibility is the much narrower intersection of exact product/version, controller absence, ephemeral isolated shape, complete identity, durable abandonment, two-observation stability, and every protection gate.

Controller-bearing sessions are protected because controller product/version authority is still observational. Ordinary Chrome, standard/shared profiles, headed/manual sessions, attached CDP sessions, other users, root/system processes, Unlinger and its ancestors are also always protected.

## Artifact matrix

| Artifact | Recognized | Automatic eligibility | Synthetic evidence | Controlled field evidence | Ambient evidence | Residuals |
| --- | --- | --- | --- | --- | --- | --- |
| exact `DevToolsActivePort` regular file | yes | **disabled in every current `0.3.0` pack**; process-only enforcement produces no candidate or action | yes, for the dormant engine | exact historical removals in generation-9 and generation-13 controlled runs | no | crash after canonical-to-quarantine rename may strand the exact quarantine entry; final revalidation-to-`unlinkat` retains a same-UID swap TOCTOU |
| profiles/browser data/runtime directories | observed only as transient classification context | never | n/a | n/a | n/a | deletion is outside 0.1 |
| browser sockets and PID files | not automatically admitted | never | no | no | no | ownership/reference contract remains open |

The controlled DAP results repeat one historical exact admitted point across two installed generations. They do not establish current artifact authority, broad artifact safety, or removal of either P2 residual. Re-enabling any artifact flag requires a separate decision.

## Protocol and runtime skew

| Layer | Current truth |
| --- | --- |
| Rust CLI/service operator protocol | schema v1; lifecycle authority remains here |
| Native App source protocol | schema v4 only; requires daemon-owned atomic `browser_overview` |
| Transitional frontend endpoint | schema v3 existing commands/response meaning retained; v4-only overview rejected without downgrade |
| Schema v2 | historical fixtures retained; superseded before installation; current server returns typed `unsupported_schema` |
| Installed generation 13 | schema v1 operator plus v4 App and transitional v3 endpoints; SQLite v6; accepted, healthy `ReadyReportOnly`; no candidate rollback lease; its installed `0.2.0` artifact-enabled binary is inert while report-only |
| Installed App/v4 integration | schema-v4 ad-hoc-signed private App installed and running; bundle identity/signature/plist verified; v4 atomic live readback passed through the exact installed CLI; final App visual and packaged notification observation remain owner gates |
| Current source candidate | policy version `0.3.0`; same exact process eligibility; all runtime-artifact flags false; full local gate passed; not installed or active until exact-head CI and transactional rollback/reinstall pass |

SQLite schema v6 is incompatible with the generation-9 binary. The real v5 rollback/open and packaged generation-12 App/restart runbook passed before the owner accepted generation 12. Generation 13 then installed with a fresh rollback lease to generation 12, passed exact report-only readiness, and was explicitly accepted; both candidate leases are now retired, but generation 13's lease was not executed. A future install must create and actually exercise its own lease rather than treating the historical generation-12→9 proof as current rollback evidence.

## Acceptance meaning

Synthetic verification proves code paths and counterexamples. Controlled field verification proves only the exact owner-approved case. Ambient verification requires ordinary long-running observation on the target machine without a harness-created incident. These evidence classes are not interchangeable.

Generation 13 was owner-authorized, armed and proved by its full-timing managed harness for exact CfT `151.0.7922.34`; it is currently contained report-only. The authorized next lane is process-only, but ambient enforcement acceptance remains false: source policy, activation, and a controlled incident do not substitute for the required transactional candidate cycle, multi-day dogfood, or an ordinary real eligible incident. Broader versions—including the currently observed protected CfT `152.0.7977.42`—remain outside automatic eligibility. Intel/universal, signing/notarization/distribution and public alpha also remain open. See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md) for claim levels.
