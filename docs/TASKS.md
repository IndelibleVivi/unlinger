# Task-owned browser sessions

`unlinger task run -- COMMAND...` gives one command an exclusive Playwright CLI session and records its actual process lifetime. The command inherits normal stdin, stdout, stderr and caller-owned inheritable descriptors. The activation pipe uses its own allocated descriptor and closes before the command executes. Unlinger returns its exit code and prints the opaque task ID to stderr.

```bash
unlinger task run -- ./browser-task.sh
unlinger task status <task-id> --json
unlinger explain <incident-id>
```

Use the CLI from the accepted installed generation (or its explicit full path). A source build alone does not replace the active service. A local PATH symlink may point to that exact CLI; it must be updated when a different generation is accepted. The service installer does not create or automatically update that shell entry.

The command can make several Playwright CLI calls and can use several workspaces. All calls must inherit `PLAYWRIGHT_CLI_SESSION`; do not override it with `-s`, `--session`, or a different environment value. Each `task run` creates a fresh session. Existing sessions and unrelated agents are not adopted retroactively. The native App continues to show the resulting browser state and actual terminal cleanup impact through schema v5.

The source Playwright pack `0.5.0` verifies `playwright-core` **`1.63.0-alpha-2026-08-31`**, its actual `cliDaemon.js` process, matching owner-private session metadata and the controller's exact listening Unix socket. Browser admission remains Chrome for Testing **`151.0.7922.34` or `152.0.7977.42`**, isolated, ephemeral and headless. Persistent, attached, headed/manual, standard-profile, unknown-version and unregistered sessions remain protected. Isolated full-timing task tests cover both exact CfT versions; these controlled points do not establish multi-day dogfood. See [current installed truth](current-state.md) before assuming a built CLI has been activated.

A task script can use an existing compatible Playwright CLI with an explicit local configuration:

```json
{
  "browser": {
    "browserName": "chromium",
    "isolated": true,
    "launchOptions": {
      "executablePath": "/path/to/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
      "headless": true
    }
  }
}
```

For example, `browser-task.sh` can run `playwright-cli open --config ./browser.json` followed by other commands. Unlinger does not install a browser or Playwright, change the command's workspace, override its browser configuration, or grant permission to signal ordinary Chrome.

## What task completion means

The daemon reserves a task before the CLI creates a gated child. It verifies the socket peer and the child's PID, birth time and UID, then commits activation before the child executes the actual command. The PID/birth/UID identity survives `exec`; the wrapper's disappearance alone does not release a live command.

The registry has `reserved`, `active`, and `released` phases. Normal command exit requests release once. If the wrapper disappears or delivery is uncertain, the daemon independently checks exact command-owner absence. A read failure preserves protection. Released tasks cannot be activated again or assigned another owner. Controllers may be discovered after command exit only if their exact birth falls inside the activation/release interval; later reuse of the session name does not inherit cleanup eligibility.

**Released is not cleaned.** A still-running non-browser command carrying the task selector or an explicit session argument, a live named Unix-socket client, or incomplete client visibility keeps the browser protected. Verified same-bundle/version Crashpad infrastructure is not counted as independent task work; this does not add those helpers to a signal plan. Browser socket pairs are distinct from external clients.

Every ordinary age, 90-second abandonment, 15-second second-observation, exact-identity, frozen-plan, protection, pause, generation/epoch and revival gate still applies. Report-only mode records ownership and observations but sends no signal. In enforcement mode, the existing journal commits a real TERM before delivery and normal Playwright shutdown can close the browser. Only the terminal receipt and independent impact authority prove what was reclaimed; task release never increments those totals. Commands do not wait for the cleanup/revival interval before returning.

Task records stay for at least 14 days after release and are pruned only after a complete snapshot no longer observes their bound controller or task-tagged process. The registry is bounded at 4,096 records. Cleanup history and lifetime impact keep their independent retention rules. Task IDs and session names are opaque local selectors; command text, working directory, raw environment, profile paths and socket paths are not stored in task history. A capability stays in private operator IPC/SQLite and is omitted from `task status`, App DTOs and diagnostic export.

## Verification

Ordinary workspace tests cover durable migration/ownership, no reassignment or reactivation, late discovery and multiple controllers, preserved exit/output and inherited descriptors, activation-pipe closure, unavailable-daemon launch refusal, wrapper crash with a live `exec` owner, owner-loss recovery, and protection counterexamples.

The ignored field test creates two dedicated sessions, verifies live task/client protection, releases one task, runs production timing, checks canonical cleanup impact, and proves the unrelated session remains usable. It uses an isolated database/socket and an extra runtime signal boundary admitting only exact descendants of its own issued controller. It retains its private workspace and receipt and never touches the installed daemon. Set the explicit acknowledgement and local paths:

```bash
UNLINGER_TASK_FIELDLAB_ACK=I_ACCEPT_OWNED_CFT_SIGNALING \
UNLINGER_TASK_FIELDLAB_CORE=/path/to/node_modules/playwright-core \
UNLINGER_FIELDLAB_CFT_APP="/path/to/Google Chrome for Testing.app" \
  cargo test -p unlinger-cli --test task_run \
  task_owned_playwright_cli_reclaims_only_its_finished_session -- --ignored --nocapture
```

This is a command-lifetime integration. It does not automatically add lifetime support to every Codex App task or every browser tool. Host-specific multi-call integrations and disk-residue deletion remain separate incomplete parts of the product.
