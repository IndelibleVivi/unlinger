# Getting started / 开始使用

These commands are for a developer building the experimental source on macOS 14+ (Apple silicon verified). Use Rust 1.98.0, Git and command-line developer tools; the App needs Swift 6.0+ and ripgrep. No browser installation is required for observation or the command-lifetime smoke below. Run commands from the repository root.

这些步骤用于开发者从源码试用。观察模式和下方 command lifetime 示例不需要另外安装浏览器；它们也不证明浏览器已被清理。命令均在 repo 根目录执行。

## 1. Build and observe / 构建与观察

```bash
cargo build --locked --release --workspace
./target/release/unlinger doctor --source-only
./target/release/unlinger scan --dry-run
./scripts/preview.sh
```

The preview runs one real native observation with a private temporary database/socket/lock and removes only that temporary state on exit. It cannot signal a browser. `scan --dry-run` independently explains current sessions without persistence or signals. A protected session is an intentional result, not evidence of a failed scan. A doctor failure or incomplete snapshot must remain visible rather than being treated as “nothing to clean.”

预览只观察一次，退出后移除自己的临时状态；不会安装服务或发送浏览器信号。全部会话为 protected 可能是正常结果。doctor 失败或快照不完整不能解释为“没有需要清理的东西”。

## 2. Keep an isolated daemon open / 交互式临时 daemon

In terminal A, retain the printed directory for terminal B. `pwd -P` resolves macOS temporary-directory symlinks, because lock validation requires a real private path.

```bash
UNLINGER_DEMO_DIR="$(mktemp -d "${TMPDIR:-/tmp}/unlinger-reader.XXXXXX")"
UNLINGER_DEMO_DIR="$(cd "$UNLINGER_DEMO_DIR" && pwd -P)"
printf 'Demo directory: %s\n' "$UNLINGER_DEMO_DIR"
./target/release/unlingerd --report-only \
  --database "$UNLINGER_DEMO_DIR/history.sqlite3" \
  --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" \
  --instance-lock "$UNLINGER_DEMO_DIR/instance.lock"
```

In terminal B, replace the example with the exact printed directory. First read status and wait for `healthy: true` and `ready: true`: the socket can exist before the first observation finishes. A task command is intentionally refused before readiness.

先确认 daemon 已 healthy、ready，再执行 task。socket 已存在不等于首次扫描已经完成。

```bash
UNLINGER_DEMO_DIR="/path/from/terminal-A"
./target/release/unlinger --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" status --json
./target/release/unlinger --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" browser status
./target/release/unlinger --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" history
./target/release/unlinger --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" \
  task run -- /usr/bin/true
# Copy the task ID printed to stderr by the preceding command.
./target/release/unlinger --socket "$UNLINGER_DEMO_DIR/unlingerd.sock" \
  task status <task-id> --json
```

The task should reach `released`; `/usr/bin/true` creates no browser and earns no cleanup impact. This proves the command registration/exit path. For browser work, substitute your script following [TASKS.md](TASKS.md): use the exact verified Playwright CLI, an admitted CfT version, the supplied isolated headless config and the inherited session. Report-only still leaves that browser running; close it through the same Playwright session when finished. Task release never enables enforcement.

这里的 `/usr/bin/true` 不创建浏览器，也不会增加清理成绩；它验证命令注册与退出。实际浏览器脚本必须满足 [任务指南](TASKS.md) 中的精确版本、配置和 session 继承要求。report-only 不会替你关闭浏览器，应通过同一 Playwright session 正常 close。

Stop terminal A with **Ctrl-C**, wait for the daemon to exit, then remove only the directory created above if you no longer need its log/history:

```bash
rm -r "$UNLINGER_DEMO_DIR"
```

This directory belongs only to the demo. An installed daemon, if present, uses its own state. Never point a second source daemon at the installed database/socket.

## 3. Optional per-user service / 可选安装为用户服务

This is a persistent installation. It changes your user LaunchAgent and service state; it is not part of the temporary preview. On a machine with an existing Unlinger service, inspect it first and follow the [upgrade/rollback runbook](INSTALLED_DOGFOOD.md). No `sudo` is needed.

这一步会安装持久运行的用户服务。已有安装时应先检查状态，并遵循升级/回滚指南；不需要 `sudo`。

```bash
./target/release/unlinger service status --json
./target/release/unlinger service install \
  --daemon ./target/release/unlingerd --mode report-only --json
./target/release/unlinger service status --json
./target/release/unlinger doctor
```

Installation creates an immutable generation and stops at `candidate_ready_report_only`. Verify healthy, ready, report-only and quiescent state. Use the CLI inside the generation reported by service status for subsequent operations:

```bash
# Replace N with active_generation from service status.
UNLINGER_CLI="$HOME/Library/Application Support/Unlinger/generations/N/unlinger"
"$UNLINGER_CLI" service restart-report-only --json
"$UNLINGER_CLI" doctor
"$UNLINGER_CLI" service status --json
```

A pending candidate must be accepted or rolled back before another install, uninstall or mode change. Acceptance retires its rollback material. For a first install, rollback restores the pre-install absence of a service; it cannot restore a nonexistent prior generation. For upgrades, validate the real rollback path before acceptance as described in the runbook.

```bash
# Choose only after inspecting the candidate:
"$UNLINGER_CLI" service accept-candidate --json
# OR, while its lease is still pending:
# "$UNLINGER_CLI" service rollback-candidate --json
```

Acceptance keeps report-only mode. After reviewing [support](SUPPORT.md) and [safety](SAFETY.md), an operator may explicitly enable process cleanup:

```bash
"$UNLINGER_CLI" service set-mode enforce --json
"$UNLINGER_CLI" service status --json
# Return to observation:
"$UNLINGER_CLI" service set-mode report-only --json
```

A timed-out mutation may already have committed: read `service status` before deciding what to do; do not blindly resend it. The installer does not create or update a PATH symlink. Keep the exact CLI path or deliberately update your shell entry when accepting a different generation.

安装完成和接受 candidate 都不会自动开启清理。启用 enforcement 是独立操作，仍受全部保护与等待期约束。命令超时后先读状态，不要盲目重复发送；CLI 的 PATH 入口需要自己维护。

## 4. App and removal / App 与卸载

```bash
apps/UnlingerApp/scripts/bundle.sh
open apps/UnlingerApp/build/Unlinger.app
```

The App uses frontend schema v5 and connects to the installed daemon. A local build is ad-hoc signed, not Developer ID signed or notarized. See the [App guide](../apps/UnlingerApp/README.md) for window/menu behavior and optional launch at login. Keep the bundle in a stable location before enabling that App-only login setting.

To remove an accepted service, use its exact CLI (finish or roll back any pending candidate first):

```bash
"$UNLINGER_CLI" service uninstall --json
./target/release/unlinger service status --json
```

Uninstall unloads the LaunchAgent and removes managed binaries; history and logs remain. It does not remove the App. Turn off the App's launch-at-login setting, quit it, and move only the App bundle you installed to Trash. Remove any shell alias/symlink you personally created. Deleting retained history/logs is a separate deliberate data deletion.

卸载服务会移除 LaunchAgent 与受管程序，保留历史和日志；不会移除 App。先关闭 App 的登录启动设置，再退出并将自己安装的 bundle 移到废纸篓。退出 App 本身不会停止后台服务。

## Verification scope / 验证范围

A clean-checkout build and isolated preview can be checked without installing anything. The maintainer's managed install/restart/rollback and cleanup evidence is recorded separately in [current state](current-state.md). A walkthrough on an existing development account does not establish first-time installation on a separate clean macOS account.
