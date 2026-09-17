# Unlinger

**任务结束，它启动的进程也该结束。**

[English](README.md) · [开始使用](docs/GETTING_STARTED.md) · [工作原理](docs/ARCHITECTURE.md)

Unlinger 在 macOS 上观察浏览器自动化留下的会话，并清理其中经过严格验证的遗留进程。原生菜单栏 App 展示当前会话、受到保护的原因，以及清理实际完成了什么。

**当前是需要从源码构建的实验性开发者预览。** daemon 默认 **report-only（仅观察）**。自动清理需要另行启用服务，并满足全部安全条件。目前没有签名公证的下载包、连续多日可靠性承诺，也不会自动接入所有 AI agent 的任务。

## 现在能做什么

- **看见遗留会话及其原因。** 原生进程快照、版本兼容性、保护原因和脱敏的本地历史。
- **跟踪一个命令的浏览器生命周期。** `unlinger task run -- COMMAND` 注册实际 command owner；兼容的 Playwright CLI 会话在任务结束后进入候选判断。任务结束本身不授权清理。
- **准确解释普通 Playwright 为什么受保护。** 源码能识别已评估的 `playwright-core` `1.62.1` daemon 形态；如果宿主没有提供精确生命周期证据，App 会明确说明并保留会话不动。Operator schema v1 与 `unlinger session` 已提供 adapter primitive，但目前还没有 Codex adapter 自动驱动它。
- **看见真实清理成果。** 完成后的 receipt 结合已送达信号的耐久记录，决定清理会话数、进程数和估算内存。未发送信号就结束的会话单独显示为“已结束，未介入”。重复观察会合并，不会计成清理成绩。
- **观察并安全清理一个精确的磁盘残留家族。** 展示 Chrome code-sign clone 数量及文件逻辑大小。源码 daemon 在默认 report-only 模式下保持不动；有效 enforce 模式会逐个判断精确候选，只移除连续两次观察未变、且没有 executable、绝对 argv 或匹配 cleanup-helper 引用的候选。正在被使用的一份 clone 会继续受保护，但不会阻塞同组中已经稳定且无人引用的旧 clone。运行在候选目录外的普通 Chrome 及 Chrome Helper 也不会阻止 stale clone 清理。当前源码会把每次尝试持久记录为不含路径的结果与汇总计数；中断的尝试会报告 delivery-unknown，而不会根据之后更小的计数反推成功，源码 App 也会展示这份结果。profile、浏览器数据和 runtime artifact 不属于这条路径。参考安装早于这套结果 authority，但已有一个有边界的实机结果：普通 Chrome 全程保持开启，八份稳定旧 clone 被移除，正在使用的一份继续受保护。这不是 multi-day 证据，逻辑字节也不等于保证释放的 APFS 物理空间。

| 范围 | 当前边界 |
| --- | --- |
| 平台 | macOS 14+；Apple silicon 已验证，Intel/universal 未验证 |
| 浏览器 | Chrome for Testing 精确版本 `151.0.7922.34` 或 `152.0.7977.42` |
| 命令生命周期接入 | `playwright-core` `1.63.0-alpha-2026-08-31` 中的 Playwright CLI，完整继承 Unlinger 发出的 session |
| 可选现有会话接入 | 普通 `playwright-core` `1.62.1`，且必须有精确 host owner lease；源码 primitive 与测试已存在，自动 host adapter 与 field evidence 尚无 |
| 其他可识别家族 | agent-browser、Puppeteer；自动清理仅考虑无 controller 且通过所有条件的进程树，这两个家族尚无受控实机清理证据 |
| 始终保护 | 普通 Chrome、有界面/手动或附着的会话、标准/共享/持久 profile、未验证的 controller、身份不完整的进程 |

[支持范围](docs/SUPPORT.md) 分别说明“可识别”“允许自动清理”和“有实机证据”，三者不能互换。

## 不安装服务，先试一次

需要 Rust **1.98.0**、macOS command-line developer tools 和 Git。构建 App 还需要 **Swift 6.0+**；App bundle 验证使用 Python 3，无需 ripgrep。

```bash
git clone https://github.com/IndelibleVivi/unlinger.git
cd unlinger
cargo build --locked --release --workspace
./target/release/unlinger doctor --source-only
./target/release/unlinger scan --dry-run
./scripts/preview.sh
```

`preview.sh` 使用独立的临时数据库、socket 和 lock，只运行一次 report-only reconciliation，输出脱敏 receipt，随后仅移除自己创建的临时状态。它不安装 LaunchAgent，也不发送清理信号。没有符合条件的遗留会话时，结果全部是 protected 属于正常行为。

[开始使用](docs/GETTING_STARTED.md) 继续介绍交互式临时 daemon、可执行的 command lifetime 示例、可选服务安装与卸载；[任务指南](docs/TASKS.md) 给出精确浏览器配置和接入约定。

## 清理权限从哪里来

![Unlinger 进程清理架构](docs/architecture.svg)

daemon 负责分类、持久化 task/optional-host 生命周期证据以及清理授权。它核对精确进程身份、浏览器与 controller 版本、所有权、活跃客户端和 profile 保护，再检查存活时间、稳定性与遗弃等待期。普通 detached Playwright controller 没有精确 host lifetime 时仍受保护；存活时间、PPID 1 或暂时没有 socket client 都不能替代任务意图。真正执行还需要明确启用 enforcement。每次信号发送都先写 journal、重新验证；确认进程消失并经过 revival 检查后，才形成最终 receipt。独立的 Chrome clone 路径逐个保留两次相隔 15 分钟的候选身份、要求完整原生 candidate-reference 证明，只对 eligible 子集执行 no-follow descriptor-relative 删除，随后立即重新扫描。App 只展示 daemon-owned 结果，不发起删除。

[架构说明](docs/ARCHITECTURE.md) 提供中英说明、图中组件的源码依据和可编辑图源。

## 原生 App

```bash
swift test --package-path apps/UnlingerApp
apps/UnlingerApp/scripts/bundle.sh
open apps/UnlingerApp/build/Unlinger.app
```

App 连接另行安装的 daemon；没有 daemon 时会显示 unavailable。构建出的 bundle 只有本地 ad-hoc 签名，不是已公证的发行包。界面支持英文和简体中文，提供菜单栏 popover 与普通 Dock 窗口。退出 App 不会停止已安装的 daemon。开发预览和通知限制见 [App 指南](apps/UnlingerApp/README.md)。

## 证据与限制

维护者的参考安装已有受控 task-owned CfT-152 清理 receipt：两次专门创建的会话、16 个进程被清理，App 的实际影响展示与之相符。隔离环境中的完整等待期测试覆盖两个精确 CfT 版本。这证明了对应场景，不等于连续多日无人看管的安全性或广泛浏览器支持。[当前状态](docs/current-state.md) 区分源码、CI、已安装程序与实机证据，也记录停用的 artifact engine 中尚未解决的问题。

正常运行完全在本地：没有账户、遥测、云同步或正常运行所需的网络请求。原始进程参数和浏览器/profile 路径仅在瞬时内存中使用；持久化历史和 diagnostics 使用结构化脱敏记录。构建会下载依赖。详见 [隐私](docs/PRIVACY.md) 与 [安全模型](docs/SAFETY.md)。

## 文档导航

| 想了解什么 | 文档 |
| --- | --- |
| 试用、安装、退出和卸载 | [开始使用](docs/GETTING_STARTED.md) |
| 如何接入自己的脚本 | [Task-owned sessions](docs/TASKS.md) |
| 宿主如何提供精确生命周期证据 | [Operator IPC](docs/IPC.md#optional-host-session-owner-commands) |
| 为什么会话受到保护 | [支持范围](docs/SUPPORT.md)、[规则](docs/SIGNATURES.md)、[安全模型](docs/SAFETY.md) |
| 谁拥有状态和执行权限 | [架构](docs/ARCHITECTURE.md)、[IPC](docs/IPC.md)、[App contract](apps/UnlingerApp/Contract/README.md) |
| 实际验证到哪一步 | [当前状态](docs/current-state.md)、[验收层级](docs/PRE_V0_1_ACCEPTANCE.md)、[Field Lab](docs/FIELDLAB.md) |
| 服务升级和回滚 | [服务操作指南](docs/INSTALLED_DOGFOOD.md) |
| 产品总体约定与实现覆盖 | [Working specification](docs/SPEC.md)、[Implementation plan](docs/IMPLEMENTATION_PLAN.md) |

## 许可与来源

软件、脚本、规则和功能性 fixtures 使用 [SUL-1.0](LICENSE)；文档、架构图和项目图像素材使用 [CC BY-NC-SA 4.0](LICENSE-DOCUMENTATION.md)。项目是 **source-available，不是 OSI 开源**：SUL 允许个人、非商业及企业内部使用，向他人分发或提供时必须免费且非商业。完整范围见 [许可说明](LICENSING.md) 与 [素材来源](docs/PROVENANCE.md)。
