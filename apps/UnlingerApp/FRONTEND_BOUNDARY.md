# Frontend implementation boundary

先读：

1. [`Contract/README.md`](Contract/README.md)
2. [`Contract/v5/`](Contract/v5/) 唯一 current frontend contract/scenario fixtures
3. 需要 transport/error 细节时读 [`docs/IPC.md`](../../docs/IPC.md)

不要从历史协议、CLI human output、schema-v1 `DaemonStatus` 或 service lifecycle code
反推当前 browser UI。v2/v3/v4 已退役，当前树不保留这些 fixture/resource 路径。

## Backend truth

Unlinger 是 local-only macOS runtime-hygiene utility，direct daemon 默认 report-only。
Source 只服务 frontend v5/operator v1，直接创建或打开 SQLite v14；App 只发送 v5，
要求 atomic `browser_overview`。旧 frontend/unknown schemas 在 dispatch 前收到 typed
refusal。精确 installed generation、mode、health 与 field scope 由
[`current-state`](../../docs/current-state.md) 维护，不能从旧验收记录继承。
Frontend 不拥有 signal authorization、storage deletion、service installation/update/
rollback、daemon mode switching 或 lifecycle recovery。

V5 提供 status/history/explain/incidents/diagnostics、mutation status、read-only browser
overview，以及 pause/resume/named retry/exact protect/unprotect。所有 action 消费 backend
capabilities，并在 commit 前由同一 backend policy 重新授权。缺失 capability 时 fail
closed，不从 stage、score、reason string 或 session presence 自行猜补。

Automatic process admission 只允许精确 CfT `151.0.7922.34` 或 `152.0.7977.42`，且所有
hard gates 成立；controller-bearing sessions 还须满足 task/optional owner lifetime。
Playwright pack `0.6.0`、其他 packs `0.4.0`；runtime-artifact admission 仍关闭。UI 不得从
settlement、residue observation 或历史 DAP evidence 推断当前会删 artifact。Unknown/
mixed/wrong versions、不明 controller ownership、headed/attached、standard/shared
profile 与 incomplete identity 保持 protected；没有 manual kill 绕过入口。

## State mapping

- schema-v5 `BrowserOverviewSnapshot`是phase、session compatibility、coverage、support catalog、attention/protection、exact recent settlement、independent impact与typed residue的唯一canonical product projection；daemon在一个status+roster snapshot boundary内生成它；
- daemon先验证`healthy + ready + roster.current + no scan + equal non-null observation time`，不可信或不一致直接给`phase: unknown`；其余phase优先级在server内固定为attention → reclaiming → confirmed → verifying → active → protected → clear；
- `BrowserOverviewMapper`只负责localized copy与display shape。Popover、ordinary window、detail和preview消费同一组presentation types；Swift不得重扫evidence、重做phase truth table或从history重算settlement事实；独立的bounded History索引可用exact typed recent settlement补回被prune的terminal row；
- `BrowserHistoryMapper`是独立的bounded-history presentation path：history index只发布terminal cleanup outcome；detail消费server-owned observation span并保持cleanup receipt/state change独立。它可以用coherent current session补足当前row的product/version与状态，但不得生成compatibility、impact或cleanup authority；
- terminal Failed提示只消费live status的`readiness == failed`，Home与唯一status item共享该presentation flag；不另建service状态机或轮询，paused/report-only/断连不冒充Failed；
- transport unavailable、backend incompatible或stale retained snapshot在App层保持unknown，不能映射成all clear；失败后可保留上一份rows供查看，但不得恢复positive phase；
- current row可以显示该row自身的member count/RSS；schema未提供global current totals，因此不得加总展示；
- compatibility是typed `product + observed_version + automatic|observe_only|protected|unknown + optional reason_id`。Coverage copy只接受mixed/product/version/missing version、controller unverified、observation only、debug-peer visibility incomplete；未知ID显示generic copy且不原样展示；
- support catalog必须来自embedded rule authority并携带`support_revision`，Swift fixture/UI不得维护另一份hard-coded eligibility matrix；
- recent settlement由daemon用exact cleanup `event_token`与更早的event identity生成；同毫秒事件仍按daemon数组中的durable event order处理，opaque token不参与先后比较；retained terminal与不同token的settlement同毫秒且没有跨来源顺序证明时保留retained terminal。缺失proof返回nil，不由App猜family/process/memory/artifact facts；
- impact由独立durable authority提供tracking起点、历史完整度、terminal/proved counts、process count与measurement-complete memory total；UI不得从bounded history重新累计；
- storage residue仅显示最新typed observation。Chrome code-sign clone logical bytes不是physical reclaim承诺；`automatic_cleanup_eligible`表示至少一份 candidate 已满足自己的稳定性、reference 与 lifecycle gate。它可以与 aggregate `reference_check: referenced`同时出现，表示另一份 candidate 仍在被引用。App只显示这个backend状态，没有删除入口、不能从该字段发起或授权清理；
- `cleared_with_residue` 必须同时表达 process success与 artifact residue，不写成 process cleanup failed；
- ambiguous count、CPU、RSS、age、pressure或 protected incident只提供低调信息，不产生 action或 notification authority；
- incident detail优先复用 coherent current session，否则使用 retained detail里的最新 observation；若 detail没有 observation但 exact-token settlement join成立，则用该 settlement继续显示 browser family和已有 typed facts，缺失 estimate保持不显示。只有 trusted `not_found`显示不存在。Transport/store/protocol failure保留旧 detail并标 stale；旧 request结果不得覆盖新 request。

Copy保持安静、直接、non-antivirus。未知 enum/reason显示 generic、保守 copy；绝不把未知值扩大成 success、eligibility、action或 notification authority。

## Mutation and transport boundary

每个 mutation在任何 connect/send前，把 namespace token、UUID、canonical command/args、semantic lock、created time和 visual dismissal写入 owner-private crash-durable journal。Journal失败则发送零字节。Pre-v0.1全局只允许一个 unresolved ordinary mutation；read-only commands继续工作。

任意字节可能发送后，timeout/EOF/reset/oversize/bad JSON/wrong schema或 request ID/wrong payload/DTO failure都进入 delivery uncertain。App restart或 “Check again” 只调用 `mutation_status`，永不重发原 mutation。Trusted committed/rejected或 unchanged-authority not-found可以收束；authority lost与 untrusted read保留 journal和 lock。Dismiss只隐藏 banner，不清 authority state。

Schema v5 request遇到exact schema-v1 `unsupported_schema` framing时显示incompatible daemon。App绝不fallback至v4、v3或v1 mutation。

## Diagnostics and identity

Diagnostics result属于发起view的local state；A view的success不会被B view的failure覆盖。V5 document使用required `document_schema_version: 5`，export是semantic-lossless JSON：保留未知fields但不承诺byte-for-byte layout。

History index使用redacted incident ID作为一条session row的稳定identity并显示聚合event count；detail timeline继续以最新public `event_token`标识每个保留phase。Action row使用 event token + mutation namespace + stable sequence。Artifact-only group必须显示。Internal event/attempt/PID/fingerprint不是 Swift identity，也不进入 ordinary UI。

## Notifications and routing

允许模式：`off`、default `attention`、`attention_and_reclaims`。首次 trusted full refresh把 retained tokens设为 baseline；off/default suppression仍标 seen，之后切 mode不追发 backlog。Notification ledger先 durable claim，再 schedule一次，选择 duplicate-avoidance；schedule failure不循环 retry。

Eligible attention只来自 typed durable cleanup/storage/daemon facts；App ordinary-mutation unresolved不产生系统 notification。Sustained unreachable只声称 App无法连接。No sound，foreground quiet，`userInfo`只含 route kind、redacted incident ID和 public event token。Fixture/preview/swift-run tests不接真实 notification center。

Notification routing 使用既有 `AppRouter`，进入 exact incident 或 global status。
Direct AppKit `@main` 强持有唯一 `UnlingerAppDelegate`；closing the last ordinary window
不终止 menu client。Popover 和 ordinary window 各有独立 router；menu-to-window 是
一次性 copy，并清空 hidden popover route，两个 NavigationStack 不绑定同一路径。
`AppWindowController` 复用 NSWindow，仅在展示时挂载 hosting controller，关闭时清空
直接 host references。语言、暂停与 notification popup 使用 `StablePopUpButton`，不能
在 polling graph 中恢复 SwiftUI popup adaptors。`SMAppService.mainApp` 只负责 menu
client launch at login，不触碰 daemon。

## Live boundary

安装服务的数据库、socket、App 与 process 不属于 source smoke。准确 installed truth 与
历史 bounded field evidence 见 [`current-state`](../../docs/current-state.md)；frontend
不得将这些结果解释成新的 UI authority 或 multi-day acceptance。

[`scripts/pre-v0.1-smoke.sh`](scripts/pre-v0.1-smoke.sh) 只操作其唯一临时 report-only
数据库/socket/lock，执行当前 v5 App integration 和 restart reconciliation。Retired-schema
refusal 由 Rust IPC tests 验证。Owner-only CfT、installed rollback 和 enforcement field
不属于 frontend source validation。
