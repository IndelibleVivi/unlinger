# Frontend implementation boundary

先只读：

1. [`Contract/README.md`](Contract/README.md)
2. [`Contract/v2/`](Contract/v2/) 全部 fixtures
3. 需要更完整 transport/error 背景时再读 [`docs/IPC.md`](../../docs/IPC.md)

不需要先翻 Rust workspace，也不要从 CLI human output、schema v1 `DaemonStatus` 或 service lifecycle code 反推 UI。

## Backend truth

Unlinger 是 local-only macOS runtime hygiene utility。当前 source 默认 report-only；当前安装的 generation 9 也稳定 report-only、unarmed。Frontend 是 backend truth 的 projection，不拥有 signal authorization、service installation/mode switching 或 lifecycle recovery。

v2 已提供 status/history/explain、pause/resume、named retry、exact protect/unprotect 和 explicit diagnostics export。所有 actions 使用 backend capabilities；缺失 capability 不在 Swift 侧猜补。

2026-08-31 起 v2 还提供只读 `incidents` roster（owner 授权）：最近一次 reconciliation cycle 的当前 incident snapshot，上限 32 条，复用 public history 的脱敏 projection。App 把它渲染为 popover 的「正在观察 / Watching right now」区。它是 observability surface，不推导 action availability，也不附带 manual kill。

当前 automatic family admission 仍刻意很窄：exact Chrome for Testing `151.0.7922.34`、无 controller 的 detached tree。Unknown/mixed versions 与 controller-bearing sessions 保持 `PROTECTED`。这不是 frontend bug，也不授权 UI 加 manual kill。

## UI state mapping boundary

- `healthy + readiness: ready + effective_mode: report_only`：正常观察；明确说明 automatic cleanup off。
- `scan_in_progress` / `cleanup_in_progress`：短暂 activity，不升级为 warning。
- `paused_until_unix_millis`：展示 pause deadline；observation/history 仍继续。
- `most_recent_reclaim`：按 `overall_outcome` 表达；`cleared_with_residue` 不得写成 process failure。
- `attention.items`：真正的 degraded/failed/residue/revival item；按 `kind + reason_id` 映射，unknown reason 使用 generic fallback。
- `ambiguous_incident_count`：可作为低调信息，不单独产生 warning 或 notification。
- transport unavailable：不能伪装成 all clear；使用 app-local unavailable fixture。

Copy register 默认安静、直接、非 antivirus/运维口吻。中英文都优先说 observable fact；不要展示 raw backend identifier 或 lifecycle terminology。

## Mutation rule

Mutation 收到可信 success 才能显示 confirmed。若 timeout/EOF/disconnect：

1. 进入 `mutation_delivery_uncertain`；
2. 不自动重发；
3. 根据 command 读取 `status` 或 `explain`；
4. 只有 readback 明确证明未提交且用户再次操作时才发送新的 mutation。

## Fixture 与 live mode

所有主要 states 先由 canonical fixtures 驱动 Preview/tests。当前 installed generation 9 是 v1-only，不能作为 v2 live endpoint。若 backend integration 需要 source v2，可在独立 terminal 启动完全隔离的 report-only daemon：

```bash
UNLINGER_DEV_DIR="$(mktemp -d)"
cargo run -p unlinger-daemon -- \
  --report-only \
  --database "$UNLINGER_DEV_DIR/history.sqlite3" \
  --socket "$UNLINGER_DEV_DIR/unlingerd.sock" \
  --instance-lock "$UNLINGER_DEV_DIR/unlingerd.lock"
```

这不会替换、reload 或改 mode 于 active generation 9。退出 source daemon 后，保留或手动清理这个 exact temp directory；App 不自动删除它。

## Notification boundary

允许的 private v0 notification categories：successful reclaim、daemon/service needs attention。不得仅因 ambiguous count、CPU、RSS、age 或 memory pressure 通知。`cleared_with_residue` 可以出现在 activity/attention 中，但文案必须保持 process success 与 artifact residue 的双重事实。
