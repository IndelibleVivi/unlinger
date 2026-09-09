# Architecture / 架构

This view answers one question: **how does a finished browser task become an observed cleanup result, and which component is allowed to act?** It describes the current process-only source path on one macOS user account. Installation transactions and protocol compatibility are explained below rather than mixed into the primary flow.

本图回答：**一个结束的浏览器任务，如何成为真实清理结果；谁有权执行？** 范围是单个 macOS 用户下的当前 process-only 源码路径。图不表示已经安装、启用或通过长期验证。

![Unlinger process-only flow](architecture.svg)

Editable source: [architecture.mmd](architecture.mmd). The SVG is a rendered export of that source. Labels use English component names; the Chinese reading guide below follows the same flow. Regenerate with Mermaid using the source's theme; inspect the rendered export after changing it.

## Read the flow / 读图

1. **Task ownership:** `unlinger task run` reserves a durable task, creates a gated command child, and activates its exact PID/birth/UID identity before execution. The issued session binds only verified Playwright CLI controllers. The daemon releases ownership after exact command absence; an active client or incomplete proof keeps protection. Task release grants no signal permission.
2. **Observation and decision:** native snapshots and exact bundle versions feed the shared rule sessionizer. Protections, age/stability and durable abandonment checks select eligible process trees. Periodic/process-exit/wake/pressure inputs only request a fresh snapshot; pressure never lowers a gate.
3. **Execution:** report-only records the decision. Enforce mode additionally needs valid signal authority. A frozen plan is revalidated, its signal action is durably PREPARED, then exact controller/root/member TERM and necessary survivor KILL stages run. Absence and bounded revival checks precede terminal settlement.
4. **Visible result:** The source SQLite v9 store retains redacted observations, task state, action journals, receipts and independent cleanup impact. One daemon-owned browser overview joins coherent current state and proved impact; the App only maps it into presentation.

中文对应路径：CLI 先登记真实 command owner；daemon 独立核实任务结束和客户端状态，再经过完整分类、保护及冷却条件。report-only 只记录。enforce 还必须获得有效执行权限，先持久化 action，再对精确身份发送信号。最终 receipt 和独立 impact 决定 App 显示的成果，任务结束或一次观察不会增加成绩。

The separate Chrome clone observer records only typed count/logical-size/shape/reference facts. It has **no deletion edge**. Every artifact admission flag is false. The dormant DAP engine is outside the active diagram and retains the unresolved risks in [SAFETY.md](SAFETY.md).

Chrome clone 的磁盘观察单独进入状态库，**没有删除出口**。逻辑字节不是保证可释放的 APFS 空间；当前策略不删除 profile、目录或任何 runtime artifact。

## Ownership and evidence map

| Node / boundary | Canonical implementation |
| --- | --- |
| Command wrapper and activation gate | [CLI task runner](../crates/unlinger-cli/src/task.rs), [task contract](TASKS.md) |
| Task lifetime and controller binding | [daemon crate](../crates/unlinger-daemon/src), [rules](../crates/unlinger-rules/src/lib.rs) |
| Exact native identity and snapshots | [macOS adapter](../crates/unlinger-macos/src/lib.rs) |
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

Raw arguments, executable/profile paths and frozen signal targets stay transient. Public persistence and UI use typed redacted records. Task capabilities remain private operator authority, excluded from ordinary App DTOs and diagnostic exports. There is no normal-operation network service. See [IPC](IPC.md) and [privacy](PRIVACY.md) for exact schema and data boundaries.
