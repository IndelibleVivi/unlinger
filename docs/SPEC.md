# Unlinger

> **The work ended. Its processes should too.**
> 任务结束，它启动的进程也该结束。

**Document:** Product & Technical Specification 0.1
**Status:** Working draft
**Date:** 2026-08-30; pre-v0.1 protocol/acceptance revision 2026-09-01
**Repository slug:** `unlinger`

---

## 1. Project thesis

Unlinger is a local, zero-touch runtime hygiene utility for agentic and browser automation.

It runs quietly in the background, reconstructs abandoned automation process trees, and reclaims them before they become visible as CPU spikes, memory pressure, swap growth, stale sockets, or days-old headless browser clusters.

The user should not have to:

- remember to close a browser;
- modify every `AGENTS.md`;
- wrap every command;
- install per-agent hooks;
- create leases or session records;
- inspect Activity Monitor;
- approve routine cleanup one incident at a time.

Unlinger assumes that agents forget, wrappers crash, parent processes disappear, upstream cleanup fails, and machines sleep or restart. Its job begins where those guarantees end.

The primary product surface is **absence**: abandoned automation disappears without becoming the user's problem. A CLI, history view, menu-bar app, notifications, and manual controls are secondary observability and exception-handling surfaces.

---

## 2. The problem

Local AI agents increasingly launch browsers, MCP servers, media workers, test runners, and helper daemons through several layers of shells and package runners. A single browser task can produce a controller, browser root, renderer processes, GPU and network services, crash handlers, temporary profiles, debugging endpoints, sockets, PID files, and sometimes media subprocesses.

These trees often outlive the work that created them because:

- the agent never issues the final close command;
- the task is interrupted or cancelled;
- the host app or terminal is killed;
- an `npx` / `pnpm dlx` / shell intermediary breaks lifecycle propagation;
- a controller exits but a detached child survives under PID 1;
- a browser or controller hangs during graceful shutdown;
- a supervisor revives only part of the tree;
- stale runtime metadata makes a new client believe an old session still exists;
- an upstream idle timeout is disabled, unsupported, or simply has not elapsed.

The failure is usually invisible until the machine is already degraded. The user then has to discover which Chrome is real, which profile is temporary, which controller belongs to which tree, and what can be killed without touching ordinary browsing.

### 2.1 Ghost incident

Unlinger does not treat one PID as the product object. It reconstructs a **ghost incident**:

> A same-user cluster of processes and runtime artifacts created for local automation whose initiating work has ended or disappeared, whose remaining components no longer have a legitimate live control path, and whose continued existence is unintended residue.

A ghost incident may contain:

- controller or CLI daemon;
- browser root;
- renderer, GPU, network, storage, utility, and crash-handler processes;
- recorder or `ffmpeg` subprocesses;
- temporary user-data directory;
- DevTools endpoint metadata;
- session socket, PID file, lock, or stale state record.

### 2.2 Explicit non-targets

Unlinger is not allowed to infer “ghost” merely from age, CPU, process name, or PPID.

The following are not targets by default:

- ordinary Chrome, Chromium, Edge, Safari, Firefox, or other user browsers;
- a user-attached CDP browser or manual DevTools session;
- headed automation that may be under direct human control;
- a declared persistent browser profile or saved login environment;
- an active development server;
- a long-running shared MCP service;
- databases, containers, terminals, editors, shells, or unrelated daemons;
- another user's processes;
- anything whose identity or abandonment state is ambiguous.

---

## 3. Product contract

### 3.1 Install-and-forget

After installation, Unlinger runs as a per-user background service and starts at login. Core cleanup must not depend on changes to Codex, Claude Code, Kimi, Playwright, `agent-browser`, project repositories, shell profiles, or prompts.

Optional integrations may provide stronger or faster evidence, but absence of an integration must never disable the core product.

### 3.2 Silent by default

Confirmed incidents are reclaimed automatically and recorded locally. Routine successful cleanup does not produce a notification in the default App mode.

The user is interrupted only when:

1. a high-impact incident repeatedly revives or cannot be cleaned;
2. a likely incident remains ambiguous and continues to create serious pressure;
3. Unlinger itself is unhealthy or has stopped protecting the machine.

The pre-v0.1 native App implements `off`, default `attention`, and `attention_and_reclaims`. The third mode may notify on a successful process reclaim; it is an explicit user preference, not the default. Notifications are a bounded best-effort local projection over trusted refreshes, not a gap-free event feed. Retained event tokens are baselined on first trusted refresh, suppressed events remain seen, and mode changes do not replay backlog.

No numeric authority exists yet for “ambiguous under serious sustained pressure.” Ambiguous count, CPU, RSS, age, memory pressure, scan activity, or a protected incident alone therefore cannot authorize either cleanup or a notification.

### 3.3 Decisive only after proof

Unlinger may wait briefly to gather evidence. Once an incident crosses the deterministic cleanup gate, it acts without asking the user to classify processes manually.

Ambiguity must reduce action, not create a queue of chores. Uncertain candidates remain untouched and are available in optional diagnostics.

### 3.4 Local and private

Unlinger operates locally, with no account and no telemetry by default. It does not inspect page contents, browser history, cookies, credentials, or application conversations.

Stored receipts use redacted fingerprints rather than full command lines, URLs, usernames, repository paths, profile identifiers, or tokens.

---

## 4. Product position

Unlinger is not:

- a task/session authority;
- a browser launcher;
- a required wrapper;
- a generic process killer;
- a dashboard-first system monitor;
- a prompt or agent skill;
- a replacement for upstream cleanup;
- a broad Mac cleaning suite.

It is **ambient cleanup for abandoned local automation**.

The first wedge is headless Chromium automation because it is common, expensive, multi-process, and distinguishable through strong runtime fingerprints. The long-term category is wider than browsers, but Unlinger must earn each new process family through evidence and counterexample testing rather than expanding through loose name matching.

---

## 5. Lessons taken from existing work

Unlinger is an independent implementation and repository. Existing projects are reference points, test competitors, and sources of hard-won design lessons.

### 5.1 Headless Guard

Useful lessons:

- reconstruct a launcher/browser/helper tree rather than killing helpers independently;
- combine multiple independent fingerprints;
- separate an explanatory score from hard cleanup eligibility;
- re-scan immediately before acting;
- TERM first, then re-identify survivors before KILL;
- protect normal profiles and ordinary browser roots as hard invariants;
- require a nearest normal-browser counterexample for every new signature.

Where Unlinger differs:

- automatic confirmed cleanup is the core product, not an opt-in rescue mode;
- the daemon and zero-touch behavior come before the dashboard;
- runtime artifacts and non-browser automation residue are part of the incident model;
- classification should be data-driven and versioned rather than concentrated in one hard-coded detector;
- process identity must move beyond PID plus elapsed time from the beginning.

### 5.2 reap

Useful lessons:

- development context must gate age/orphan signals;
- duplicate detection can reveal forgotten repeated launches;
- protected families and self/ancestor protection need explicit policy;
- machine-readable dry-run output is valuable during development.

Where Unlinger differs:

- no interactive `scan` → `kill` workflow for routine cases;
- no generic stale-process score as the final authorization mechanism;
- browser and automation sessions are reconstructed as incidents, not isolated PIDs;
- pre-signal identity revalidation and post-cleanup revival detection are mandatory.

### 5.3 cc-reaper and similar hook/script toolboxes

Useful lessons:

- PGID cleanup is valuable when groups remain intact;
- hooks provide a fast normal-exit path;
- a daemon is necessary for crashes and abandoned sessions;
- setsid, supervisors, and package-runner intermediaries defeat any single cleanup path;
- CPU and memory pressure can justify earlier scanning without proving abandonment.

Where Unlinger differs:

- no Claude-specific installation or project configuration;
- no shell-script bundle as the primary product;
- no assumption that PPID 1 or a known command pattern is sufficient;
- no user-maintained whitelist as a normal operating requirement;
- one coherent engine, policy model, receipt format, and release path.

### 5.4 Upstream lifecycle fixes

`agent-browser`, Playwright, and other runtimes should continue improving their own cleanup. Process groups, idle timeouts, parent-death handling, and graceful close paths materially reduce leaks.

They cannot remove the need for Unlinger because:

- a process cannot reliably run its own cleanup after SIGKILL or host failure;
- multiple launchers and versions coexist on one machine;
- outer hosts may omit close commands;
- wrappers and package runners can break signal and parent relationships;
- stale artifacts may survive after the process tree is gone;
- the local machine needs one final reconciliation layer across tools.

Unlinger treats upstream cleanup as the first defense and itself as the quiet last defense.

---

## 6. Detection model

### 6.1 Evidence families

Each candidate incident is evaluated through four independent evidence families.

#### A. Automation provenance

Evidence that the tree was created for automation:

- known controller lineage (`agent-browser`, Playwright CLI/MCP, Puppeteer, ChromeDriver, etc.);
- automation-specific executable or package path;
- canonical temporary profile convention;
- private debugging pipe or ephemeral debugging port;
- automation launch flags;
- creation-time clustering and shared process group;
- known session socket or runtime directory.

#### B. Abandonment

Evidence that the work which justified the tree is gone:

- original controller or host process identity no longer exists;
- root/controller has been reparented to the platform's orphan target;
- control socket has no live peer and remains unchanged;
- session PID record points to a dead or identity-mismatched process;
- no active recognized owner exists in the ancestor or peer graph;
- the candidate remains unchanged across a cooling interval;
- an optional host hint reports task completion or cancellation.

#### C. Isolation

Evidence that cleanup will not touch ordinary human state:

- dedicated temporary user-data directory;
- browser root belongs only to the reconstructed automation tree;
- all processes run under the current user;
- no standard browser profile path;
- no unrelated browser roots share the same profile or controller;
- no live process still references a candidate runtime artifact.

#### D. Protection / contrary evidence

Evidence that blocks automatic cleanup:

- standard browser profile;
- visible or headed session without a stronger deterministic automation contract;
- user-selected persistent profile;
- manual CDP port or attached operator browser;
- live terminal, host, controller, or protocol peer;
- shared service classification;
- changed PID birth identity;
- incomplete or contradictory process graph;
- unsupported framework/version whose nearest counterexample is not covered.

### 6.2 Authorization rule

Scores may rank and explain candidates, but scores alone never authorize termination.

A candidate is automatically reclaimable only when all hard gates pass:

```text
same_user
AND strong_automation_provenance
AND confirmed_abandonment
AND isolated_session
AND stable_across_two_observations
AND process_identity_unchanged
AND no_protection_rule
```

CPU, RSS, age, swap pressure, and duplicate count can alter scan urgency and candidate ordering. They cannot lower the cleanup threshold.

### 6.3 Confidence states

- **PROTECTED** — contrary evidence or explicit protected class; never touched.
- **ACTIVE** — automation exists and still has a legitimate live path.
- **COOLING** — likely abandoned; awaiting a second stable observation.
- **CONFIRMED** — all deterministic gates pass; eligible for automatic reclamation.
- **AMBIGUOUS** — some evidence exists, but a hard gate is missing or contradicted.
- **RECLAIMING** — cleanup state machine is in progress.
- **CLEARED** — process tree and eligible runtime residue are gone.
- **REVIVED** — a supervisor recreated part of the incident.
- **FAILED** — cleanup could not complete safely.

These states describe the whole frozen process-plus-artifact plan. A terminal cleanup receipt also projects independent `process_outcome`, `artifact_outcome`, `overall_outcome`, and `attention_required` facts. If exact liveness plus revival checks prove the process tree gone but an artifact is refused before any uncertain artifact side effect, the plan remains `FAILED` for durable attention/retry while the human-facing outcome is `cleared_with_residue`. Process or artifact `delivery_unknown`, and failures after a side effect whose terminal result cannot be proved, remain overall `failed` and retain global fail-close behavior.

---

## 7. Runtime behavior

### 7.1 Scan triggers

Unlinger combines event-driven and reconciliation behavior:

- full scan at daemon startup;
- low-frequency periodic sweep, initially every 60 seconds;
- immediate re-evaluation when a watched controller/root exits;
- immediate sweep after system wake;
- accelerated sweep under sustained memory pressure;
- optional host/task-ended hint.

Periodic reconciliation remains mandatory because events can be missed while the daemon is stopped and process trees can detach in ways that lose the original parent relationship.

### 7.2 Cooling interval

Initial defaults:

- minimum candidate age: 60 seconds;
- two observations separated by 15 seconds;
- normal abandonment grace: 90 seconds from owner disappearance;
- revival check: 15 seconds and 60 seconds after cleanup.

Memory pressure may shorten the delay before the second scan, but does not change the evidence required.

### 7.3 Cleanup state machine

1. Freeze the incident plan and evidence receipt.
2. Re-scan the entire relevant process graph.
3. Re-resolve every target by `(pid, birth identity)` and command/executable fingerprint.
4. Abort if confidence downgraded or any protection rule appears.
5. Use a framework-native graceful close adapter when it is unambiguously safe.
6. Send `SIGTERM` to the dedicated controller/root before helpers.
7. Wait a bounded grace period.
8. Re-scan and remove processes that exited from the plan.
9. Send `SIGTERM` to remaining verified tree members where needed.
10. Re-scan again.
11. Send `SIGKILL` only to exact identity-matching survivors or a verified dedicated process group.
12. Confirm the complete tree is gone.
13. Re-check for immediate supervisor-driven revival.
14. If the active pack explicitly admits an artifact, clean only artifacts whose live references are gone; current `0.3.0` packs admit none.
15. Commit a redacted local receipt with independent process/artifact/overall outcomes.

Unlinger never uses broad `killall`, process-name-only `pkill`, or an unrestricted PID list captured minutes earlier.

### 7.4 Runtime artifact cleanup

The engine contains a narrowly scoped `DevToolsActivePort` cleanup capability, but the current owner-approved process-only policy does not activate it. Every policy-version `0.3.0` signature pack sets `devtools_active_port = false`, so the analyzer produces no runtime-artifact candidate and enforcement cannot schedule or journal an artifact action. Re-enabling this path requires a separate owner decision after the crash-after-quarantine and final same-UID swap residuals are resolved or explicitly accepted.

If a later pack explicitly admits the capability, it may remove only one exact `DevToolsActivePort` regular file after proving its frozen file identity, safe parent, exclusive ownership, complete absence of live references, complete process-tree exit, and no revival. Socket, PID-file, lock-file, and other runtime-metadata cleanup remains part of the incident model but is not automatically admitted until a framework-specific canonical convention and the same ownership, reference, and race guarantees have field evidence.

Temporary profile deletion is deferred. A later version may quarantine or delete canonical ephemeral profiles only after a longer delay and a separate safety gate. Standard profiles, persistent profiles, saved authentication state, cookies, and browser data are never deleted by default.

---

## 8. Architecture

### 8.1 Implementation choice

The core is a Rust workspace, macOS-first and cross-platform by design.

Reasons:

- low resident overhead for a permanent daemon;
- strong process and concurrency primitives;
- memory safety in signal and graph-handling code;
- static CLI/daemon distribution;
- clean separation between shared policy and platform backends;
- future Linux and Windows support without replacing the core engine.

A native SwiftUI menu-bar and Dock App is the thin human-facing client. It translates one daemon-owned schema-v4 browser snapshot into browser-session language but does not own classification, compatibility, product phase, cleanup policy, lifecycle, installation, or signal authority.

### 8.2 Components

```text
unlinger/
├── crates/
│   ├── unlinger-core/       process graph, incident model, state machine
│   ├── unlinger-rules/      compiled signature packs and validation
│   ├── unlinger-macos/      libproc/sysctl/signals/pressure/wake backend
│   ├── unlinger-daemon/     scheduler, persistence, IPC, launchd service
│   └── unlinger-cli/        status, history, explain, doctor, pause
├── rules/
│   ├── agent-browser.toml
│   ├── playwright.toml
│   └── puppeteer.toml
├── fixtures/
│   └── macos/
├── apps/
│   └── UnlingerApp/        native browser-first menu-bar and Dock client
├── tests/
│   ├── integration/
│   └── chaos/
└── docs/
    ├── SPEC.md
    ├── SAFETY.md
    ├── SIGNATURES.md
    ├── PRIVACY.md
    └── FIELDLAB.md
```

### 8.3 macOS backend

The first backend should:

- use native process inspection through libproc/sysctl rather than parsing `ps` as the long-term implementation;
- identify processes by PID plus birth/start identity and executable fingerprint;
- capture UID, PPID, PGID, executable, arguments, start time, state, RSS, CPU samples, and relevant descriptors;
- use process-exit dispatch sources for known candidates;
- use launchd as a per-user LaunchAgent;
- observe sleep/wake and memory-pressure events;
- require no administrator privileges, Accessibility, Screen Recording, browser extension, or EndpointSecurity entitlement.

### 8.4 Persistence and IPC

The daemon stores a compact local SQLite database containing:

- incident IDs and state transitions;
- redacted evidence categories;
- cleanup actions and outcomes;
- resource estimates before/after;
- signature pack/version;
- bounded error details.

Event history defaults to 14 days or 10,000 events, whichever is smaller. Ordinary-mutation receipts have a separate minimum 14-day reconciliation window and cannot be pruned early to satisfy a count cap; capacity pressure rejects a new mutation rather than destroying authority.

A Unix-domain socket exposes read-mostly local IPC. Schema v1 remains the Rust CLI/service compatibility protocol. Frontend schema v4 projects strict public status/history/incident/roster/diagnostics DTOs, exact readiness/freshness, stable public event tokens, explicit capabilities, ordinary mutations, read-only mutation reconciliation, and one atomic browser overview. Schema v3 remains a transitional endpoint for its existing request/response meaning but cannot return the v4-only overview. Both omit process and service lifecycle identities. Schema v2 was superseded before installation and receives typed `unsupported_schema`; the source App never downgrades to v3 or v1.

Every frontend mutation carries a public-safe receipt namespace and canonical UUID. Before applying a new request, the daemon serializes lifecycle state with one immediate SQLite transaction, replays an exact receipt before current lifecycle policy, recomputes the same shared policy used for capability projection, and atomically commits state, durable cleanup-policy revision, and a typed `applied | no_change | rejected` receipt. Pruning rotates namespace in the same transaction. Current-authority `not_found` can prove absence; `authority_lost` cannot.

The App durably records pending mutation intent before connect/send. Any post-send untrusted result remains unresolved and is reconciled only through `mutation_status`; the original mutation is never automatically resent. Pre-v0.1 permits one unresolved ordinary mutation at a time while read-only surfaces remain available.

Ordinary user mutations are pause/resume, explicit protect/unprotect, and named retry failed cleanup. Diagnostics export and browser overview are read-only. Generation- and instance-bound arm, disarm, and drain messages exist only in schema v1; frontend schemas have no install, mode, signal, update, rollback, or lifecycle authority.

The browser overview captures status and the latest roster under one in-memory source boundary, then projects the authoritative phase, bounded session summaries, typed product/version compatibility and coverage, attention/protection, exact recent settlement, and a support catalog generated from embedded rule packs. Any unhealthy, not-ready, scanning, stale, never-observed, or observation-time-incoherent source becomes `unknown`. Trusted phase precedence is `attention → reclaiming → confirmed → verifying → active → protected → clear`. The App and CLI may format this result but may not recompute it from separate calls.

Compatibility is a typed classification fact, not an evidence-string convention. Current in-memory reports carry browser product, optional observed version, `automatic | observe_only | protected | unknown`, and an optional stable reason ID. These app-bundle facts may enter the current v4 projection but do not enter retained SQLite history. The generated support catalog carries a readable revision and must remain derived from the same embedded version policies that authorize classification.

A recent browser settlement joins the exact public cleanup event token to its durable receipt and then selects the greatest earlier event ID containing an observation for that incident. This preserves event identity when multiple records share a millisecond. Missing or contradictory links return no settlement; bounded frontend history and timestamps are not substitutes.

### 8.5 Signature packs

Each supported runtime has a versioned signature pack that parameterizes one shared deterministic sessionizer and cleanup-safety algorithm. A pack contains:

- controller and browser fingerprints;
- profile and runtime-path conventions;
- graph/sessionization markers and parameters;
- protection rules;
- graceful close strategy;
- artifact cleanup policy;
- an exact, bounded, or observational version policy;
- positive fixtures;
- nearest counterexample fixtures.

Version 0.1 ships rules inside the signed binary. Remote executable rule updates are out of scope. Later data-only updates must be signed and auditable.

The first automatic-cleanup admission is intentionally narrower than the recognized family list: a controllerless Chrome-for-Testing browser root with bundle identifier `com.google.chrome.for.testing` at exact version `151.0.7922.34`, plus every ordinary hard gate. Controller anchors remain protected until their own product/version identity is verified. Broader versions or family shapes require a positive fixture and the nearest normal/manual counterexample before their pack policy may expand.

---

## 9. Version 0.1 scope

### Supported platform

- macOS 14 or later;
- Apple silicon is the currently verified source/controlled-field architecture;
- Intel and a universal binary remain 0.1 release requirements but are not yet verified;
- current-user processes only.

Exact support truth and evidence classes live in [`SUPPORT.md`](SUPPORT.md) and the Rust-validated [`support-matrix.v1.json`](support-matrix.v1.json). Recognition, deterministic classification, automatic eligibility, synthetic evidence, controlled field evidence, and ambient evidence are separate claims.

### Supported automation families

1. `agent-browser` local Chromium / Chrome-for-Testing sessions;
2. Playwright CLI and Playwright MCP Chromium sessions;
3. Puppeteer Chromium sessions.

### Eligible process families

- controller/daemon;
- browser root;
- renderer/GPU/network/storage/utility helpers;
- dedicated crash handler;
- recorder/`ffmpeg` only when strongly joined to the same incident.

### Explicitly deferred

- Firefox and WebKit;
- Selenium and arbitrary ChromeDriver setups;
- generic Node, Python, Rust, or dev-server cleanup;
- containers and remote hosts;
- deletion of temporary browser profiles;
- a dashboard that requires continuous user attention;
- public distribution of the private menu-bar App;
- Linux and Windows enforcement;
- remote accounts, cloud sync, telemetry, or hosted reports.

---

## 10. User surfaces

The daemon works without opening an app.

Version 0.1 CLI:

```text
unlinger status
unlinger browser status [--json]
unlinger history [--json]
unlinger explain <incident-id>
unlinger doctor
unlinger pause <duration>
unlinger resume
unlinger retry <incident-id>
unlinger protect <incident-id>
unlinger unprotect <incident-id>
unlinger scan --dry-run
unlinger export-diagnostics <incident-id>
```

`status` should answer only what matters:

- is Unlinger healthy;
- is a scan or cleanup in progress;
- are any confirmed/ambiguous incidents present;
- what was reclaimed most recently.

The pre-v0.1 native menu-bar App projects schema-v4 status, atomic browser overview, history/detail, diagnostics, ordinary actions, notification preferences, menu-client launch at login, App/daemon versions, and explicit App-only quit semantics. Its first screen answers in browser language: whether supported leftovers are absent, active, being verified, confirmed, being reclaimed, deliberately protected, or need attention; which automation family is involved; whether Unlinger is observe-only or allowed to clean; and what the latest exact settlement proved. It must not turn the product into a dashboard the user has to watch and owns no daemon lifecycle or signal authority.

The transitional roster is observability, not a work queue. The browser overview is the composition surface: the daemon alone validates healthy `ready` status, current roster, no replacement scan, and equal non-null status/roster observation timestamps before returning a positive phase. Starting, draining, failed, unknown, unavailable, incompatible, and stale are never all-clear. On transport loss the App may retain old rows as stale context, but it must display an App-local unknown phase rather than reinterpret the snapshot.

The App may select user-facing coverage copy only from typed compatibility reason IDs it explicitly understands. Unknown reasons remain generic and are never displayed raw. Current session rows may show their own process count and resident memory, but schema v4 does not authorize global current process/RSS totals. The App consumes the server's exact settlement as-is; it does not join bounded history or invent a browser identity or resource estimate.

---

## 11. Safety invariants

1. Never terminate another user's process.
2. Never terminate a root/system process.
3. Never terminate Unlinger or any of its ancestors.
4. Never terminate ordinary browser sessions or standard profiles.
5. Never authorize cleanup from age, CPU, PPID, process name, or one flag alone.
6. Never act on a PID without revalidating birth identity immediately before each signal.
7. Never delete a profile or runtime directory while a live process references it.
8. Never use broad process-name kills.
9. Never lower confidence thresholds because the machine is under pressure.
10. Never claim success without a post-action scan.
11. Never enter an unbounded kill/revival loop.
12. Every automatic action must produce a redacted evidence receipt.
13. Any new signature must include a positive fixture and the nearest plausible normal-process counterexample.
14. A frontend ordinary mutation is journaled before send, committed with its receipt and durable revision in one transaction, and never automatically resent after delivery becomes uncertain.
15. Frontend capability projection and authoritative mutation admission use the same policy; a UI affordance or score never authorizes a change.
16. Stable public event tokens identify retained events without exposing internal event/attempt IDs; missing typed outcome never becomes a fabricated success.
17. Frontend schemas cannot encode service lifecycle or signal authority, and version skew never falls back from v4 to v3 or v1.
18. Browser product phase, compatibility, coverage, support catalog and recent settlement have one daemon-owned v4 projection; a frontend must not reconstruct stronger truth from separate reads.

---

## 12. Success criteria

### Product

- Canonical ghost incidents require zero user action.
- Successful cleanup is normally invisible.
- Median cleanup begins within 120 seconds of confirmed owner disappearance.
- P95 cleanup begins within 3 minutes.
- The daemon survives sleep/wake, host-app restarts, and its own restart without losing reconciliation ability.

### Safety

- Zero ordinary-browser terminations in the fixture suite, chaos suite, dogfood period, and public alpha.
- No cleanup from a single heuristic.
- All SIGKILL actions are preceded by TERM, grace, and identity revalidation unless the process is already in an unrecoverable dedicated containment group.

### Completeness

- At least 99% of supported deterministic incidents leave no surviving verified process-tree member after cleanup.
- Current process-only packs produce zero runtime-artifact actions. Any future admitted `DevToolsActivePort` file may be removed only after the tree is confirmed dead and its exact file identity, ownership, references, and parent are revalidated; re-enabling also requires an accepted resolution of the remaining final unlink-race boundary. Socket/PID artifact admission remains evidence-gated.
- Revival is detected and attributed rather than silently counted as success.

### Overhead

Initial target on an idle Mac:

- average daemon CPU below 0.2%;
- resident memory below 30 MB;
- normal sweep completes within 100 ms on a typical developer machine;
- no network traffic during normal operation.

---

## 13. Test and evidence programme

### 13.1 Fixture corpus

Build a redacted corpus from real process snapshots covering:

- active and abandoned `agent-browser` sessions;
- Playwright CLI/MCP normal exit, cancellation, host crash, and SIGKILL;
- Puppeteer temporary and persistent profiles;
- normal Chrome with multiple profiles;
- Chrome manually launched with remote debugging;
- headed automation under human control;
- concurrent sessions from different agents;
- supervisor revival;
- stale sockets with no process;
- live process with stale-looking metadata;
- PID reuse and identity mismatch.

Fixtures contain process topology and redacted arguments, never page contents or credentials.

### 13.2 Live integration harness

The harness must create real synthetic process trees and assert behavior for:

- clean parent exit;
- missing close command;
- terminal closure;
- host SIGTERM;
- host SIGKILL;
- controller hang during shutdown;
- child `setsid` escape;
- partial tree exit;
- immediate supervisor restart;
- daemon restart while candidate is cooling;
- sleep/wake during cooling;
- simultaneous normal Chrome use.

### 13.3 Adversarial requirements

- property tests for idempotent state transitions;
- PID-reuse simulation;
- time-of-check/time-of-use race tests;
- malformed and contradictory process graphs;
- signature version drift;
- corrupted persistence;
- bounded retry and revival-loop tests;
- resource-overhead benchmarks.

### 13.4 Comparative fieldlab

Run the same known incidents through Unlinger, Headless Guard, reap, and representative script/hook solutions. Compare:

- detection precision;
- false-positive counterexamples;
- time to detection;
- complete-tree cleanup;
- revival handling;
- artifact cleanup;
- idle overhead;
- user actions required.

The goal is not to imitate another project's UI or wording. The fieldlab identifies failure modes and establishes whether Unlinger's zero-touch thesis is actually achieved.

---

## 14. Development plan

### Phase 0 — Evidence lab

- initialize repository and documents;
- capture first redacted real-world fixtures;
- implement macOS process snapshot and stable process identity;
- build graph/sessionization engine;
- implement report-only classification;
- write nearest-counterexample tests;
- produce the first fieldlab report.

**Exit gate:** the engine reconstructs the known seven-session/70-process class of incident correctly while protecting normal Chrome and active automation.

### Phase 1 — Deterministic cleanup engine

- implement incident state machine;
- implement two-observation cooling;
- implement TERM → rescan → exact KILL;
- implement revival detection;
- add agent-browser, Playwright, and Puppeteer signature packs;
- ship dry-run CLI and synthetic chaos harness.

**Exit gate:** all canonical chaos cases pass with no ordinary-browser termination.

### Phase 2 — Ambient daemon

- launchd service;
- startup/wake/periodic reconciliation;
- process-exit and memory-pressure triggers;
- SQLite receipts and bounded retention;
- confirmed auto-clean enabled by default;
- health checks and local diagnostics.

**Exit gate:** multi-day dogfood with successful zero-touch cleanup and no false positives.

### Phase 3 — Distribution hardening

- universal binaries;
- signing and notarization;
- Homebrew tap/cask;
- atomic updates and rollback;
- privacy/safety documentation;
- public alpha fieldlab and false-positive intake.

### Phase 4 — Optional surfaces and platforms

- continue hardening and distributing the now-source-complete native menu-bar projection;
- signed data-only signature updates;
- Linux backend using `/proc`, user services, and optional cgroup integration;
- Windows backend using native process identity and optional Job Object integration;
- carefully admitted non-browser automation families.

Pre-v0.1 source, isolated, installed, enforcement, private acceptance, public-alpha, and public-release claims are defined separately in [`PRE_V0_1_ACCEPTANCE.md`](PRE_V0_1_ACCEPTANCE.md). The source SQLite-v6 candidate may enter the owner-authorized installed report-only lane only with the acceptance-scoped v5 rollback lease active; level 3 still requires a real generation-9 rollback/open and packaged App/restart acceptance before any installed claim.

---

## 15. Naming

### Project name: Unlinger

The name states the product action without turning it into a guard, reaper, janitor, authority, or dashboard.

Something lingered after its reason to exist disappeared. Unlinger makes that lingering stop, quietly and automatically.

It also creates a clean command and daemon vocabulary:

```text
unlinger
unlingerd
unlinger status
unlinger explain
```

The visual identity should avoid cartoon ghosts, skulls, scythes, warning-red system-cleaner tropes, and “AI utility” gradients. The strongest motif is a process trace that loses opacity and resolves into a clean baseline: disappearance as restored quiet, not violence.

---

## 16. Source landscape

Primary references reviewed for this draft:

- Headless Guard: https://github.com/study8677/HeadlessGuard
- reap: https://github.com/vignesh07/reap
- cc-reaper: https://github.com/theQuert/cc-reaper
- agent-browser changelog: https://github.com/vercel-labs/agent-browser/blob/main/CHANGELOG.md
- Codex orphaned agent-browser report: https://github.com/openai/codex/issues/34178
- Playwright orphan-cleanup proposal: https://github.com/microsoft/playwright/pull/41009
- Apple process exit events: https://developer.apple.com/documentation/dispatch/dispatchsource/processevent
- Windows Job Objects: https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
- Linux cgroup v2: https://docs.kernel.org/admin-guide/cgroup-v2.html

These sources inform the safety and testing requirements. Unlinger remains a clean independent implementation.
