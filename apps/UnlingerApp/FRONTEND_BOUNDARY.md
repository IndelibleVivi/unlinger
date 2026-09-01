# Frontend implementation boundary

先读：

1. [`Contract/README.md`](Contract/README.md)
2. [`Contract/v3/`](Contract/v3/) active fixtures
3. 需要 transport/error 细节时读 [`docs/IPC.md`](../../docs/IPC.md)

[`Contract/v2/`](Contract/v2/) 只保留历史审计证据。不要从它、CLI human output、schema-v1 `DaemonStatus` 或 service lifecycle code反推当前 UI。

## Backend truth

Unlinger 是 local-only macOS runtime-hygiene utility。Direct daemon与安装中的 generation 12默认/当前都保持 report-only；generation 12提供 schema-v3 App endpoint、SQLite v6，且是 unarmed。它保留到 generation 9/v5 的 rollback lease。Frontend 只投影 backend truth，不拥有 signal authorization、service installation/update/rollback、daemon mode switching或 lifecycle recovery。

Schema v3提供 status/history/explain/incidents/diagnostics、mutation status，以及 pause/resume/named retry/exact protect/unprotect。所有 actions使用 backend capabilities，并由同一 backend policy在 commit前重新授权。UI缺失 capability时 fail closed，不从 stage、score、reason string或 roster presence自行猜补。

Automatic admission依然极窄：controllerless exact Chrome for Testing `151.0.7922.34`，且所有 hard gates成立。Unknown/mixed/wrong versions、controller-bearing、headed/attached、standard/shared profile与不完整 identity保持 `PROTECTED`。UI不得添加 manual kill绕过它。

## State mapping

- Quiet 仅当 `healthy + readiness.ready + no attention + roster.current + no scan/cleanup activity`；
- `starting`、`draining`、`failed`、unknown、transport unavailable、backend incompatible与 stale roster都不能映射成 all clear；
- replacement scan进行中保留上一份 roster并标 scanning；失败后保留并标 stale；never-observed不制造 timestamp；
- `cleared_with_residue` 必须同时表达 process success与 artifact residue，不写成 process cleanup failed；
- ambiguous count、CPU、RSS、age、pressure或 protected incident只提供低调信息，不产生 action或 notification authority；
- incident detail只有 trusted `not_found`显示不存在。Transport/store/protocol failure保留旧 detail并标 stale；旧 request结果不得覆盖新 request。

Copy保持安静、直接、non-antivirus。未知 enum/reason显示 generic、保守 copy；绝不把未知值扩大成 success、eligibility、action或 notification authority。

## Mutation and transport boundary

每个 mutation在任何 connect/send前，把 namespace token、UUID、canonical command/args、semantic lock、created time和 visual dismissal写入 owner-private crash-durable journal。Journal失败则发送零字节。Pre-v0.1全局只允许一个 unresolved ordinary mutation；read-only commands继续工作。

任意字节可能发送后，timeout/EOF/reset/oversize/bad JSON/wrong schema或 request ID/wrong payload/DTO failure都进入 delivery uncertain。App restart或 “Check again” 只调用 `mutation_status`，永不重发原 mutation。Trusted committed/rejected或 unchanged-authority not-found可以收束；authority lost与 untrusted read保留 journal和 lock。Dismiss只隐藏 banner，不清 authority state。

Schema v3 request遇到 exact schema-v1 `unsupported_schema` framing时显示 incompatible daemon。App绝不 fallback至 v1 mutation。

## Diagnostics and identity

Diagnostics result属于发起 view的 local state；A view的 success不会被 B view的 failure覆盖。Document使用 required `document_schema_version`，export是 semantic-lossless JSON：保留未知 fields但不承诺 byte-for-byte layout。

History row使用 public `event_token`；action row使用 event token + mutation namespace + stable sequence。Artifact-only group必须显示。Internal event/attempt/PID/fingerprint不是 Swift identity，也不进入 ordinary UI。

## Notifications and routing

允许模式：`off`、default `attention`、`attention_and_reclaims`。首次 trusted full refresh把 retained tokens设为 baseline；off/default suppression仍标 seen，之后切 mode不追发 backlog。Notification ledger先 durable claim，再 schedule一次，选择 duplicate-avoidance；schedule failure不循环 retry。

Eligible attention只来自 typed durable cleanup/storage/daemon facts；App ordinary-mutation unresolved不产生系统 notification。Sustained unreachable只声称 App无法连接。No sound，foreground quiet，`userInfo`只含 route kind、redacted incident ID和 public event token。Fixture/preview/swift-run tests不接真实 notification center。

Click routing复用 shared `AppRouter`，进入 exact incident或 global status。AppKit拥有 `NSStatusItem`/`NSPopover` 与一个可关闭、可再次打开的 ordinary `NSWindowController`；两者通过 `NSHostingController` 投影同一份 SwiftUI state/router。Popover route必须提供显式 Back，并可在不改写当前 route 的前提下打开 ordinary window；window只保留原生 Back。不得退回依赖 lazy `WindowGroup` 注册或 `MenuBarExtra` scene replication 的并行宿主。`SMAppService.mainApp` 只负责 menu client launch at login，不触碰 daemon。

## Live boundary

Active generation-12 database属于安装服务并使用 SQLite v6；generation-9 binary只能打开 lease 中保留的 v5 snapshot。Acceptance-scoped prior manifest/plist/v5 DB lease、explicit accept/rollback和exact report-only restart已经通过 [`../../docs/INSTALLED_DOGFOOD.md`](../../docs/INSTALLED_DOGFOOD.md) 的真实 installed proof：generation 9 rollback/open成功，generation 12重新安装，App与daemon restart完成reconciliation，全程没有arm。Lease在initial dogfood期间继续保留。

使用 [`scripts/pre-v0.1-smoke.sh`](scripts/pre-v0.1-smoke.sh) 获得可重复的 isolated report-only integration。Owner-only CfT harness、ambient enforcement与 installed dogfood proof仍不属于 frontend source validation。
