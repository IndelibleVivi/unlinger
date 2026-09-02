# Frontend implementation boundary

先读：

1. [`Contract/README.md`](Contract/README.md)
2. [`Contract/v4/`](Contract/v4/) active browser-product fixtures and [`Contract/v3/`](Contract/v3/) transitional shared-command fixtures
3. 需要 transport/error 细节时读 [`docs/IPC.md`](../../docs/IPC.md)

[`Contract/v2/`](Contract/v2/) 只保留历史审计证据。不要从它、transitional v3 data、CLI human output、schema-v1 `DaemonStatus` 或 service lifecycle code反推当前 browser UI。

## Backend truth

Unlinger 是 local-only macOS runtime-hygiene utility。Direct daemon默认report-only；安装中的accepted generation 15当前是healthy `ReadyEnforce`，提供schema v4与transitional v3、SQLite v6，按generation/epoch绑定`0.3.0` process-only authority且无pending lease。Installed App只发送v4，并要求atomic `browser_overview`。Frontend只投影backend truth，不拥有signal authorization、service installation/update/rollback、daemon mode switching或lifecycle recovery。

Schemas v3/v4都提供 status/history/explain/incidents/diagnostics、mutation status，以及pause/resume/named retry/exact protect/unprotect。V4另提供read-only `browser_overview`；v3请求该command会收到typed `invalid_request`，不会downgrade或拼装替代结果。所有actions使用backend capabilities，并由同一backend policy在commit前重新授权。UI缺失capability时fail closed，不从stage、score、reason string或session presence自行猜补。

Automatic process admission依然极窄：controllerless exact Chrome for Testing `151.0.7922.34`，且所有 hard gates成立。Current `0.3.0` packs关闭runtime-artifact admission；这不改变frontend schema，UI也不得从settlement或历史DAP evidence推断当前会删artifact。Unknown/mixed/wrong versions、controller-bearing、headed/attached、standard/shared profile与不完整 identity保持 `PROTECTED`。UI不得添加 manual kill绕过它。

## State mapping

- schema-v4 `BrowserOverviewSnapshot`是phase、session compatibility、coverage、support catalog、attention/protection与recent settlement的唯一canonical product projection；daemon在一个status+roster snapshot boundary内生成它；
- daemon先验证`healthy + ready + roster.current + no scan + equal non-null observation time`，不可信或不一致直接给`phase: unknown`；其余phase优先级在server内固定为attention → reclaiming → confirmed → verifying → active → protected → clear；
- `BrowserOverviewMapper`只负责localized copy与display shape。Popover、ordinary window、detail和preview消费同一组presentation types；Swift不得重扫evidence、重做phase truth table或再次用history join settlement；
- `BrowserHistoryMapper`是独立的bounded-history presentation path：history index按incident聚合，detail只合并连续且family/state相同的observation；cleanup receipt与state change必须保持独立。它可以用coherent current session补足当前row的product/version与状态，但不得生成compatibility或cleanup authority；
- transport unavailable、backend incompatible或stale retained snapshot在App层保持unknown，不能映射成all clear；失败后可保留上一份rows供查看，但不得恢复positive phase；
- current row可以显示该row自身的member count/RSS；v4没有提供global current totals，因此不得加总展示；
- compatibility是typed `product + observed_version + automatic|observe_only|protected|unknown + optional reason_id`。Coverage copy只接受mixed/product/version/missing version、controller unverified、observation only、debug-peer visibility incomplete；未知ID显示generic copy且不原样展示；
- support catalog必须来自embedded rule authority并携带`support_revision`，Swift fixture/UI不得维护另一份hard-coded eligibility matrix；
- recent settlement由daemon用exact cleanup `event_token`与更早的event identity生成；同毫秒事件仍按durable event order处理。缺失proof返回nil，不由App猜family/process/memory/artifact facts；
- `cleared_with_residue` 必须同时表达 process success与 artifact residue，不写成 process cleanup failed；
- ambiguous count、CPU、RSS、age、pressure或 protected incident只提供低调信息，不产生 action或 notification authority；
- incident detail优先复用 coherent current session，否则使用 retained detail里的最新 observation；若 detail没有 observation但 exact-token settlement join成立，则用该 settlement继续显示 browser family和已有 typed facts，缺失 estimate保持不显示。只有 trusted `not_found`显示不存在。Transport/store/protocol failure保留旧 detail并标 stale；旧 request结果不得覆盖新 request。

Copy保持安静、直接、non-antivirus。未知 enum/reason显示 generic、保守 copy；绝不把未知值扩大成 success、eligibility、action或 notification authority。

## Mutation and transport boundary

每个 mutation在任何 connect/send前，把 namespace token、UUID、canonical command/args、semantic lock、created time和 visual dismissal写入 owner-private crash-durable journal。Journal失败则发送零字节。Pre-v0.1全局只允许一个 unresolved ordinary mutation；read-only commands继续工作。

任意字节可能发送后，timeout/EOF/reset/oversize/bad JSON/wrong schema或 request ID/wrong payload/DTO failure都进入 delivery uncertain。App restart或 “Check again” 只调用 `mutation_status`，永不重发原 mutation。Trusted committed/rejected或 unchanged-authority not-found可以收束；authority lost与 untrusted read保留 journal和 lock。Dismiss只隐藏 banner，不清 authority state。

Schema v4 request遇到exact schema-v1 `unsupported_schema` framing时显示incompatible daemon。App绝不fallback至v3或v1 mutation。

## Diagnostics and identity

Diagnostics result属于发起view的local state；A view的success不会被B view的failure覆盖。V4 document使用required `document_schema_version: 4`，export是semantic-lossless JSON：保留未知fields但不承诺byte-for-byte layout。

History index使用redacted incident ID作为一条session row的稳定identity并显示聚合event count；detail timeline继续以最新public `event_token`标识每个保留phase。Action row使用 event token + mutation namespace + stable sequence。Artifact-only group必须显示。Internal event/attempt/PID/fingerprint不是 Swift identity，也不进入 ordinary UI。

## Notifications and routing

允许模式：`off`、default `attention`、`attention_and_reclaims`。首次 trusted full refresh把 retained tokens设为 baseline；off/default suppression仍标 seen，之后切 mode不追发 backlog。Notification ledger先 durable claim，再 schedule一次，选择 duplicate-avoidance；schedule failure不循环 retry。

Eligible attention只来自 typed durable cleanup/storage/daemon facts；App ordinary-mutation unresolved不产生系统 notification。Sustained unreachable只声称 App无法连接。No sound，foreground quiet，`userInfo`只含 route kind、redacted incident ID和 public event token。Fixture/preview/swift-run tests不接真实 notification center。

Click routing复用 shared `AppRouter`，进入 exact incident或 global status。Direct AppKit `@main`在 `NSApplication.run()` 整个生命周期强持有唯一 `UnlingerAppDelegate`；closing the last ordinary window不得终止 menu client。该 delegate拥有 `NSStatusItem`/`NSPopover` 与一个可关闭、可再次打开的 ordinary `NSWindowController`；两者通过 `NSHostingController` 投影同一份 SwiftUI state/router。Popover route必须提供显式 Back，并可在不改写当前 route 的前提下打开 ordinary window；window只保留原生 Back。不得退回依赖 SwiftUI `App`/lazy `WindowGroup` 注册或 `MenuBarExtra` scene replication 的并行宿主。`SMAppService.mainApp` 只负责 menu client launch at login，不触碰 daemon。

## Live boundary

Active generation-15 database属于安装服务并使用SQLite v6；它已accepted、healthy process-only enforce且没有pending lease。Candidate A generation 14通过[`../../docs/INSTALLED_DOGFOOD.md`](../../docs/INSTALLED_DOGFOOD.md)完成install、restart与真实rollback到generation 13；same exact-head candidate以generation 15 fresh reinstall，重复restart与serialized App checks后才accept。Full-timing field run结束时先回到report-only，之后才独立arm并通过later sweep。

使用 [`scripts/pre-v0.1-smoke.sh`](scripts/pre-v0.1-smoke.sh) 获得可重复的isolated report-only v4 App integration；它也保留v3 transitional regression coverage。Owner-only CfT harness、process-only activation与installed dogfood proof仍不属于frontend source validation，即使当前generation 15已经分别通过这些runtime gates。
