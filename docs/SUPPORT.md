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
| `playwright` | yes | yes | exact two-point CfT allowlist; controllerless or task-owned CLI `1.63.0-alpha-2026-08-31` with every lifetime/client gate | unregistered or unverified controller, all ordinary browser/profile/identity protections | yes | historical CfT 151 points; isolated task-owned CfT 151/152 with one TERM, both revival checks and unrelated-session protection; installed task-owned CfT 152 receipt and App impact (exact evidence in current-state) | no |
| `puppeteer` | yes | yes | same exact controllerless two-point CfT allowlist and all hard gates | same protections | yes | no | no |

“Recognized” means the rule pack can reconstruct and explain a candidate. It does not authorize a signal. “Deterministic classification” means the shared sessionizer and hard gates produce a typed state for the available facts. Automatic eligibility is the much narrower intersection of exact product/version, controller absence or verified released task ownership, ephemeral isolated shape, complete identity, durable abandonment, two-observation stability, and every protection gate.

Unregistered or unverified controller-bearing sessions remain protected. The narrow source task-owned CLI lane is specified in [TASKS.md](TASKS.md). Ordinary Chrome, standard/shared profiles, headed/manual sessions, attached CDP sessions, other users, root/system processes, Unlinger and its ancestors are also always protected.

## Artifact matrix

| Artifact | Recognized | Automatic eligibility | Synthetic evidence | Controlled field evidence | Ambient evidence | Residuals |
| --- | --- | --- | --- | --- | --- | --- |
| exact `DevToolsActivePort` regular file | yes | **disabled in every source and installed pack**; process-only enforcement produces no candidate or action | yes, for the dormant engine | exact historical removals in generation-9 and generation-13 controlled runs | no | crash after canonical-to-quarantine rename may strand the exact quarantine entry; final revalidation-to-`unlinkat` retains a same-UID swap TOCTOU |
| Chrome `code_sign_clone` residue family | yes, exact current-user path shape | never; observation only and `automatic_cleanup_eligible = false` | yes; traversal also proves no fixture deletion | none | one owner-observed residue family motivated the tranche; historical clear/absent observation; latest runtime state is maintained in [`current-state.md`](current-state.md) and is not an absence guarantee | logical bytes are not a physical-reclaim guarantee; no deletion design or authority |
| profiles/browser data/runtime directories | observed only as transient classification context | never | n/a | n/a | n/a | deletion is outside 0.1 |
| browser sockets and PID files | not automatically admitted | never | no | no | no | ownership/reference contract remains open |

The controlled DAP results repeat one historical exact admitted point across two installed generations. They do not establish current artifact authority, broad artifact safety, or removal of either P2 residual. Re-enabling any artifact flag requires a separate decision.

## Protocol and runtime skew

| Layer | Current truth |
| --- | --- |
| Rust CLI/service operator protocol | schema v1; lifecycle authority remains here |
| Native App source protocol | schema v5 only; requires daemon-owned atomic `browser_overview` with impact/residue and consumes server-owned observation spans |
| Transitional frontend endpoint | schema v4 prior overview/response shapes retained without v5-only fields |
| Legacy-compatible frontend endpoint | schema v3 existing commands/response meaning retained; overview rejected without downgrade |
| Schema v2 | historical fixtures retained; superseded before installation; current server returns typed `unsupported_schema` |
| Installed generation | see [current-state](current-state.md) for exact generation, activation and rollback lease; current compatible runtime uses operator v1, frontend v5/v4/v3 and SQLite v8 |
| Installed App/v5 integration | schema-v5 ad-hoc-signed private App installed and running; bundle identity/signature/plist verified; seven live-socket tests passed before and after candidate restart; v5 home and cleanup-only history UI read live; packaged notification observation remains an owner gate |
| Unmerged source candidate | schema v5/v4/v3 plus operator v1; SQLite v9 attribution repair over the v8 task-ownership authority; Playwright `0.5.0`, other packs `0.4.0`; exact CfT 151/152 process allowlist; all runtime-artifact flags false; task ownership, impact/spans and observe-only storage residue |

SQLite v8 has a candidate-specific v7 backup/rollback/open proof: generation 22 really rolled back to healthy generation 19 with the old CLI/daemon reopening v7; the corrected candidate freshly reinstalled as generation 23, repeated App/socket checks before/after restart and was accepted before arming. The unchanged schema-v5 App remains compatible. Earlier schema migrations remain historical evidence.

## Acceptance meaning

Synthetic verification proves code paths and counterexamples. Controlled field verification proves only the exact owner-approved case. Ambient verification requires ordinary long-running observation on the target machine without a harness-created incident. These evidence classes are not interchangeable.

The task-owned lane has isolated controlled process-only evidence for exact CfT 151 and 152, plus installed generation-23 CfT-152 cleanup and actual App impact readback. Unregistered or unverified controllers remain protected. Generation 15 separately retains the historical complete managed process/restart/containment point; it is not substituted for current-generation evidence. Ordinary ambient enforcement acceptance remains false. Multi-day dogfood, broader field evidence, Intel/universal, signing/notarization/distribution and public alpha remain open. See [current-state](current-state.md) for exact outcomes and [PRE_V0_1_ACCEPTANCE](PRE_V0_1_ACCEPTANCE.md) for claim levels.
