# Architecture / 架构

This view answers one question: **how does a finished browser task or exact Chrome clone residue become an observed cleanup result, and which component is allowed to act?** It describes current source paths on one macOS user account. Installation transactions and protocol compatibility are explained below rather than mixed into the primary flow.

本图回答：**一个结束的浏览器任务或精确 Chrome clone 残留，如何成为真实清理结果；谁有权执行？** 范围是单个 macOS 用户下的当前源码路径。存储清理已经安装并有一个有边界的实机结果；图不表示 multi-day 或广泛环境验证。

![Unlinger process-only flow](architecture.svg)

Editable source: [architecture.mmd](architecture.mmd). The SVG is a rendered export of that source. Labels use English component names; the Chinese reading guide below follows the same flow. Regenerate with Mermaid using the source's theme; inspect the rendered export after changing it.

## Read the flow / 读图

1. **Lifetime evidence:** `unlinger task run` reserves a durable task, creates a gated command child, and activates its exact PID/birth/UID identity before execution. The issued session binds only verified Playwright CLI controllers. Separately, an optional host adapter may keep an existing ordinary session name while declaring an exact registry selector, live child owner and controller version. The daemon releases either kind only after exact owner absence; an active client, unsupported controller or incomplete proof keeps protection. No release grants signal permission.
2. **Observation and decision:** native snapshots derive executable dev/inode identity from each process's already-mapped Darwin vnode, require its path to match `pidpath`, and never reopen the executable pathname. Exact bundle versions then feed the shared rule sessionizer. Protections, age/stability and durable abandonment checks select eligible process trees. Periodic/process-exit/wake/pressure inputs only request a fresh snapshot; pressure never lowers a gate.
3. **Execution:** report-only records the decision. Enforce mode additionally needs valid signal authority. A frozen plan is revalidated, its signal action is durably PREPARED, then exact controller/root/member TERM and necessary survivor KILL stages run. Absence and bounded revival checks precede terminal settlement.
4. **Visible result:** The source SQLite v11 store composes v10 optional host session ownership, v9 action attribution and the earlier task/impact authorities with redacted observations, action journals and receipts. One daemon-owned browser overview joins coherent current state, proved process impact, current clone residue and the latest terminal clone-cleanup result; the App only maps that authority into presentation.
5. **Chrome clone storage cleanup:** the exact current-user scanner returns typed shape/size facts plus an in-memory candidate identity set. The storage gate needs the same set at two consecutive 15-minute observations and a complete native snapshot proving no executable/absolute argv path enters a candidate and no cleanup Helper names a matching suffix. Ordinary Chrome/Helpers outside candidates and valid unrelated suffixes do not block; insufficient Chrome-looking facts fail closed. Report-only or any incomplete lifecycle/process/filesystem fact records state without mutation. A healthy ready enforce daemon must persist PREPARED before removing the descriptor-held exact candidates, then immediately rescan and atomically commit the terminal result with the actual latest residue observation. An interrupted or unproved attempt becomes `delivery_unknown`; a later smaller count never retroactively claims success.

中文对应路径：`task run` 可登记真实 command owner；兼容宿主也可通过 optional adapter 提供普通会话的精确 owner lifetime。没有 adapter、版本不支持、owner 仍存活或事实不完整时，controller 会继续受保护。daemon 还要经过完整分类、客户端、profile、冷却和身份条件。report-only 只记录。enforce 还必须获得有效执行权限，先持久化 action，再对精确身份发送信号。最终 receipt 和独立 impact 决定 App 显示的成果，任务结束或一次观察不会增加成绩。Chrome clone 存储路径则要求候选连续两次 15 分钟观察稳定、完整快照证明没有进程通过 executable/绝对 argv 路径引用候选且没有 Helper 指向匹配 suffix；候选外的普通 Chrome/helper 不阻塞。它只在有效 enforce 生命周期中先持久化 PREPARED，再执行 descriptor-relative 删除并立即重扫，随后把 terminal result 与真实 latest observation 原子提交；中断或无法证明的动作只会成为 `delivery_unknown`，不会靠之后更小的目录数倒推成功。

The Chrome clone path is separate from process runtime artifacts and has its own explicit deletion edge. It does not enable any artifact-pack flag or reuse DAP authority. Every runtime-artifact admission flag remains false. The dormant DAP engine is outside the active diagram and retains the unresolved risks in [SAFETY.md](SAFETY.md).

Chrome clone 的磁盘状态单独进入状态库；源码逐个保留 candidate identity，对每个 candidate 独立评估稳定性与 live reference，只删除 scanner 再次确认的 eligible 子集。一份正在使用的 clone 会受保护，但不阻塞其他已稳定且无人引用的 clone。逻辑字节不是保证可释放的 APFS 空间；profile、浏览器数据和 runtime artifact 仍无自动删除入口。已安装 generation 34 是 healthy `ReadyEnforce`，含 `.app.bundle`-aware per-candidate gate；一个 production-timing 实机点在普通 Chrome 持续开启时移除了八份 stale clone 并保留一份 live clone。

## Ownership and evidence map

| Node / boundary | Canonical implementation |
| --- | --- |
| Command wrapper and activation gate | [CLI task runner](../crates/unlinger-cli/src/task.rs), [task contract](TASKS.md) |
| Task and optional host lifetime/controller binding | [daemon ownership stores](../crates/unlinger-daemon/src/store), [operator IPC](IPC.md#optional-host-session-owner-commands), [rules](../crates/unlinger-rules/src/lib.rs) |
| Exact native identity and snapshots | [macOS adapter](../crates/unlinger-macos/src/lib.rs) |
| Chrome clone scan, stability, process gate and descriptor-relative removal | [macOS storage residue path](../crates/unlinger-macos/src/storage_residue.rs), [daemon lifecycle wiring](../crates/unlinger-daemon/src/main.rs) |
| Classification and protection | [core](../crates/unlinger-core/src/lib.rs), [embedded rule packs](../rules) |
| Frozen plans, revalidation, signals and revival | [core cleanup](../crates/unlinger-core/src), [reconciliation engine](../crates/unlinger-daemon/src) |
| Durable SQLite state and impact | [history store](../crates/unlinger-daemon/src/store.rs) |
| Atomic browser projection and ordinary commands | [protocol](../crates/unlinger-protocol/src/lib.rs), [public IPC](../crates/unlinger-daemon/src/public_ipc.rs) |
| App presentation only | [browser mapper](../apps/UnlingerApp/Sources/UnlingerKit/State/BrowserOverviewMapper.swift), [history mapper](../apps/UnlingerApp/Sources/UnlingerKit/State/BrowserHistoryMapper.swift) |
| Installed lifecycle | [service CLI](../crates/unlinger-cli/src/service.rs), [operator runbook](INSTALLED_DOGFOOD.md) |

## Durable boundaries

Managed installs use immutable generations and a candidate acceptance transaction. Each new generation first reaches a healthy, quiescent report-only state. A pending lease retains prior generation/database material until explicit acceptance or rollback. `RollbackInProgress` is durable before physical restoration; only durable `Accepted` retires rollback material. Same-generation restarts recover signal-free, require a fresh first scan and create a new enforcement epoch before restoring applicable intent. `Failed` is terminal for that daemon instance. [Current state](current-state.md) owns the exact installed generation and evidence; no generation number is a permanent architecture rule.

安装、接受与启用分别拥有持久状态。失败实例不会因后来一次成功扫描自动恢复执行权；回滚恢复的是精确旧 generation 和数据库。当前运行事实以 current-state 为准。

The owner-private Unix socket serves frontend schema v5, transitional v4, legacy-compatible v3 and operator v1; historical v2 is rejected. The App emits v5 only and cannot encode lifecycle operations. Ordinary mutations are journalled before send and reconciled after uncertain delivery without automatic resend. The daemon commits mutation state and receipt atomically. Each IPC request makes one bounded attempt.

Raw arguments, executable/profile paths and frozen signal targets stay transient. Public persistence and UI use typed redacted records. Task and host-session capabilities remain private operator authority, excluded from ordinary App DTOs and diagnostic exports. There is no normal-operation network service. See [IPC](IPC.md) and [privacy](PRIVACY.md) for exact schema and data boundaries.
