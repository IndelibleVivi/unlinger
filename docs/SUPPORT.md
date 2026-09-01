# Support truth

Unlinger 不使用一个模糊的 “supported” 标签把识别、自动授权和 field evidence 混在一起。机器可读 authority 是 [`support-matrix.v1.json`](support-matrix.v1.json)，并由 `unlinger-rules` test 对照当前 embedded rule packs。

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
| `playwright` | yes | yes | same exact controllerless CfT point and all hard gates | same protections | yes | one owner-approved managed full-timing point | no |
| `puppeteer` | yes | yes | same exact controllerless CfT point and all hard gates | same protections | yes | no | no |

“Recognized” means the rule pack can reconstruct and explain a candidate. It does not authorize a signal. “Deterministic classification” means the shared sessionizer and hard gates produce a typed state for the available facts. Automatic eligibility is the much narrower intersection of exact product/version, controller absence, ephemeral isolated shape, complete identity, durable abandonment, two-observation stability, and every protection gate.

Controller-bearing sessions are protected because controller product/version authority is still observational. Ordinary Chrome, standard/shared profiles, headed/manual sessions, attached CDP sessions, other users, root/system processes, Unlinger and its ancestors are also always protected.

## Artifact matrix

| Artifact | Recognized | Automatic eligibility | Synthetic evidence | Controlled field evidence | Ambient evidence | Residuals |
| --- | --- | --- | --- | --- | --- | --- |
| exact `DevToolsActivePort` regular file | yes | only after the exact admitted process tree is proved gone, both revival checks pass, targeted pathname-reference and complete argv proofs succeed, and exact file/parent identity survives quarantine/revalidation | yes | one exact removal | no | crash after canonical-to-quarantine rename may strand the exact quarantine entry; final revalidation-to-`unlinkat` retains a same-UID swap TOCTOU |
| profiles/browser data/runtime directories | observed only as transient classification context | never | n/a | n/a | n/a | deletion is outside 0.1 |
| browser sockets and PID files | not automatically admitted | never | no | no | no | ownership/reference contract remains open |

The controlled DAP result is one exact point. It does not establish broad artifact safety or remove either P2 residual.

## Protocol and runtime skew

| Layer | Current truth |
| --- | --- |
| Rust CLI/service operator protocol | schema v1; lifecycle authority remains here |
| Native App source protocol | schema v3 only |
| Schema v2 | historical fixtures retained; superseded before installation; current server returns typed `unsupported_schema` |
| Installed generation 9 | v1-only, report-only, unarmed |
| Installed App/v3 integration | not performed |

Source SQLite schema v6 is intentionally isolated from the installed generation-9 database. Generation 9 cannot open a v6 store, and the current service transaction does not retain a post-install acceptance rollback lease. Installed v3 integration therefore remains blocked until a prior-generation manifest/plist/database lease and a real v5 rollback/open test exist.

## Acceptance meaning

Synthetic verification proves code paths and counterexamples. Controlled field verification proves only the exact owner-approved case. Ambient verification requires ordinary long-running observation on the target machine without a harness-created incident. These evidence classes are not interchangeable.

Current ambient enforcement acceptance is false. Multi-day dogfood, an ambient real eligible incident, narrow owner-authorized enforce acceptance, Intel/universal, signing/notarization/distribution, and public alpha remain open. See [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md) for claim levels.
