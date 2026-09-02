# 本地 IPC contract

Unlinger source接受三条exact local wire：

- `schema_version = 1`：Rust CLI、diagnosis 与 service lifecycle/operator compatibility；
- `schema_version = 3`：transitional frontend compatibility endpoint；
- `schema_version = 4`：current native App contract与atomic browser product projection。

`schema_version = 2`是从未安装或发布的历史frontend draft。其fixtures保留审计价值，但current server在dispatch前以schema-v1 framing返回typed `unsupported_schema`。没有schema negotiation、silent downgrade、App→v3 fallback或App→v1 fallback。

Rust authority：

- v1 envelope/commands/lifecycle：[`crates/unlinger-daemon/src/ipc.rs`](../crates/unlinger-daemon/src/ipc.rs)
- v4 DTO/envelope与v3 compatibility contract：[`crates/unlinger-protocol/src/lib.rs`](../crates/unlinger-protocol/src/lib.rs)
- internal→public projection：[`crates/unlinger-daemon/src/public_ipc.rs`](../crates/unlinger-daemon/src/public_ipc.rs)
- shared ordinary-action policy：[`crates/unlinger-daemon/src/public_action_policy.rs`](../crates/unlinger-daemon/src/public_action_policy.rs)

## Socket and local-account boundary

- Transport 是 per-user Unix-domain stream socket，不监听 TCP/UDP，也没有 normal-operation network traffic。
- 默认路径位于 effective user home下的 `Library/Application Support/Unlinger/run/unlingerd.sock`。Home通过 effective UID/account database解析，不信任 `HOME`；root被拒绝。
- Socket parent必须是 current effective user拥有、非 symlink、mode `0700`的 directory；socket是 `0600`。
- Server通过 `getpeereid` 要求 peer UID等于 daemon effective UID。Authorization boundary是“同一 local account”，不是同一账户内 hostile-process isolation。
- CLI ordinary commands可用全局 `--socket <path>` 指向 isolated source daemon；service lifecycle commands不接受该 override。
- 每个 connection恰好一个 LF-delimited JSON request和一个 response。Client为每个 request建立新 connection。
- Request上限 64 KiB，response上限 4 MiB。
- Ordinary/default和service clients各有一次 15-second I/O attempt。Server最多并发8个 accepted connections，每个connection 3-second I/O timeout。Worker saturation、peer failure或early close不保证 JSON response。

## Framing and response trust

V4 request example：

```json
{"schema_version":4,"request_id":17,"command":{"command":"browser_overview"}}
```

Client必须用 strict integer types验证 exact `schema_version`和`request_id`，并验证：

- `ok: true` 恰有 `payload`，无 `error`；
- `ok: false` 恰有 `error`，无 `payload`；
- payload discriminator与 requested command匹配；
- required DTO fields存在且类型 exact。

不通过上述检查的 response不可信。JSON field order无意义；unknown enum必须在 UI保守 fallback，不得扩大 capability、eligibility、success或 notification authority。

### Phase-aware failure semantics

Transport区分：

1. encode / before connect；
2. connected but no request byte written；
3. partial or full request write；
4. response received but untrusted；
5. trusted response。

对frontend mutation（v3/v4语义相同）：

- encode/connect/明确 zero-byte first write failure：`failed_before_send`；
- 任意 byte可能写出后发生 timeout、EOF、reset、oversize、bad JSON、wrong schema/request ID、contradictory envelope、wrong payload或 DTO decode failure：delivery uncertain；
- exact requested-schema error + exact request ID：trusted rejection；
- exact schema-v1 `unsupported_schema` + exact request ID：trusted incompatible daemon，frontend mutation未被旧endpoint dispatch；
- 永不 automatic resend。

同样的 post-send fault用于 read-only command时是 read/protocol failure，不建立 mutation uncertainty。Transport cancellation对 blocking POSIX I/O执行 shutdown/close；不会留下 MainActor blocking read。

## Frontend schemas v4 and v3

### Read-only commands

| Command | Payload | Meaning |
| --- | --- | --- |
| `browser_overview` | none | v4-only atomic browser phase/session/compatibility/coverage/settlement/catalog projection |
| `status` | none | public daemon projection、global capabilities、mutation authority |
| `history` | `limit` | bounded redacted durable timeline |
| `explain` | `incident_id` | exact redacted incident detail and capabilities |
| `incidents` | none | latest observation roster plus freshness |
| `mutation_status` | `MutationContext` | exact receipt reconciliation; no mutation |
| `export_diagnostics` | `incident_id` | public semantic-lossless diagnostics document |

### Ordinary mutations

| Command | Required data |
| --- | --- |
| `pause` | `context`, bounded `duration_millis` |
| `resume` | `context` |
| `retry_failed_cleanup` | `context`, exact `incident_id` |
| `protect_incident` | `context`, exact `incident_id` |
| `unprotect_incident` | `context`, exact `incident_id` |

V3不接受`browser_overview`并返回v3-framed typed `invalid_request`；它不会返回v4 payload或silent downgrade。V3/v4 command enum都没有`arm`、`disarm`、`begin_drain`、install、update或rollback。

## Namespace-aware durable mutation receipts

```text
MutationContext {
  namespace_token: canonical 32-byte opaque token representation
  mutation_id: canonical UUID
}
```

`status.mutation_authority`公开 current namespace和 `minimum_reconciliation_window_millis = 1209600000`（14 days）。Token只证明 ordinary receipt absence；不复用或泄露 activation generation、enforcement epoch、daemon instance、database identity/path。

For each new frontend mutation, ControlPlane lifecycle/status gate and store transaction form one serialization boundary:

1. lock shared status/policy gate；
2. `BEGIN IMMEDIATE`；
3. lookup exact `(namespace_token, mutation_id)`；
4. replay same canonical request，或 conflict on different request；
5. require current namespace for an absent receipt；
6. read exact durable facts and apply the shared action policy；
7. apply state change, advance durable cleanup-policy revision exactly once when state changes, and insert typed receipt；
8. commit；
9. mirror committed revision/state to volatile projection。

Receipt replay precedes lifecycle policy, so a stored result remains readable after draining/failed and never advances revision again. A newly absent request during draining/failed receives a durable typed rejection. Transaction failure changes neither durable state/receipt/revision nor volatile state。

Receipt outcome：

```text
applied { typed result }
no_change { reason_id }
rejected { reason_id }
```

`committed` means this outcome is durable. A committed retry only records that retry scheduling/policy state changed; it does not claim downstream cleanup success.

`mutation_status`：

- `committed`：returns exact stored receipt；
- `not_found`：only when supplied namespace remains current and exact receipt is absent；
- `authority_lost`：exact receipt absent and supplied namespace is no longer current。

Receipt pruning/deletion and namespace rotation share one transaction. No receipt may be deleted inside the 14-day window for a count cap; new mutations fail closed when capacity cannot retain their proof. Existing receipts under an old namespace remain replayable. Old namespace + missing receipt cannot be applied as a new request.

The Swift App writes its PendingMutation journal before connect/send. On restart it calls only `mutation_status`; it never resends the original command. Trusted committed/rejected or current-authority not-found may resolve the journal. Authority loss and untrusted reads retain the global unresolved lock. Visual dismissal is not reconciliation.

## Public status and readiness

V3/v4 `PublicStatus` includes:

- daemon version, healthy;
- `readiness = starting | ready | draining | failed | unknown`;
- effective `report_only | enforce | unknown` mode;
- scan/cleanup activity, pause deadline and last completed scan;
- confirmed/ambiguous counts;
- typed recent reclaim;
- event-source and durable storage/recovery health;
- bounded typed attention and protection summaries;
- global action capabilities;
- mutation authority.

It excludes daemon PID, instance ID, activation/armed generation, enforcement epoch, requested mode, database schema, service transaction/binary/path identity and raw last error.

Quiet UI requires healthy + ready + zero attention + current roster + no activity. Starting/draining/failed/unknown never become all-clear. Capabilities are projections of the same pure policy used immediately before transaction commit; a frontend affordance is never authorization.

## Observation roster and time

`incidents` returns:

```text
ObservationRoster {
  cycle_token: optional
  observed_at_unix_millis: optional
  freshness: never_observed | scan_in_progress | current | stale_after_failure
  items: up to 32 redacted observations
}
```

Never-observed has no sentinel token or timestamp. A replacement cycle keeps the last complete snapshot visible as `scan_in_progress`; if replacement fails before publish, that prior snapshot becomes `stale_after_failure`. The engine writes a complete observation batch atomically before it publishes the roster. A failed batch cannot produce partial history plus a fresh roster.

Cycle-start, actual snapshot observation and cycle-completion times are distinct. `last_scan_at_unix_millis` and managed successful-cycle acceptance use completion time. Roster time is the real snapshot observation time.

Roster is observability, not a queue. Presence does not authorize protect/retry/signal and no manual kill exists.

## Atomic browser product snapshot

V4 `browser_overview` replaces frontend composition of independently timed status/roster/history reads. `ControlPlane` first refreshes durable attention/protection, then captures status and the latest roster while holding the two in-memory locks as one source boundary; it performs no SQLite query under those locks. Projection contains generated/cycle/observation time, freshness, health, effective mode, pause deadline, phase, bounded sessions, typed compatibility and coverage, attention/protection, exact recent settlement and the rule-generated support catalog.

Positive phase requires healthy Ready state, no scan in progress, current roster and equal non-null status/roster observation time. Otherwise phase is `unknown`. Trusted precedence is `attention → reclaiming → confirmed → verifying → active → protected → clear`; Swift and CLI display it but do not recompute it.

Each session carries typed browser product, optional observed bundle version, compatibility decision and optional reason ID produced during deterministic rule analysis. Browser support is generated from the embedded packs with a readable `support_revision`; the App does not ship a second eligibility table.

Recent settlement starts from the exact public reclaim `event_token`, verifies that exact cleanup receipt/outcome, then selects the maximum earlier durable event ID containing an observation for the same incident. Timestamp equality is irrelevant, so multiple same-millisecond events cannot cross-wire the browser family. If any link is absent or contradictory, the settlement field is absent; the frontend does not scan a bounded history page or invent a fallback.

## Public event identity and outcomes

SQLite v6 assigns every retained event a unique opaque `public_token`. V1 internal event IDs remain unchanged and frontend schemas never expose them. History uses `event_token`; recent reclaim and attention carry it when they originate from that durable event. Swift action identity adds stable action sequence and receipt namespace where required, so repeated stage/kind and artifact-only rows remain distinct.

Cleanup projection keeps:

```text
process_outcome  = cleared | revived | failed | delivery_unknown | unknown
artifact_outcome = not_applicable | reconciled | residue | delivery_unknown | unknown
overall_outcome  = cleared | cleared_with_residue | revived | failed | unknown
attention_required
```

Typed stored outcome, not a reason-string prefix, determines attention/reclaim kind. Missing legacy outcome is unknown/omitted, never synthesized as cleared. Whole-plan `FAILED` may coexist with public `cleared_with_residue` when the process tree is proved gone and a known no-removal artifact disposition safely retains residue. Any delivery uncertainty or unproved post-side-effect result remains failed/fail-closed.

## Diagnostics and redaction

Frontend history/detail/diagnostics may include family/version, member/resource totals, role counts, evidence/gates, typed outcomes, signal stage/signal/disposition summaries, artifact kind/disposition and public event tokens. V4 current-session compatibility adds only typed browser product/version/decision/reason to current in-memory projections; those app-bundle facts are not persisted into history.

They exclude raw/survivor/source PIDs, internal event/attempt IDs, process/member/artifact fingerprints, tracking/session identity, full argv, executable/profile/artifact paths, frozen targets, lifecycle generation/epoch/instance and raw errors.

Diagnostics has required integer `document_schema_version`. The Swift decoder uses exact CodingKeys and export preserves unknown JSON fields semantically; it may reserialize whitespace/key order, so the contract is semantic-lossless, not byte-verbatim. Output uses owner-private crash-durable atomic file rules and remains an explicit local action.

## Notifications are an App projection

The daemon has no notification/network account surface. The App derives bounded local notifications only after a trusted full refresh:

- default `attention`: typed cleanup/storage/daemon attention and sustained local-daemon unreachable;
- `attention_and_reclaims`: additionally successful process reclaim;
- `off`: no schedules.

First trusted refresh baselines retained event tokens. Off/default-suppressed events are still marked seen; mode changes never replay backlog. The ledger durably claims an event/health episode before one schedule attempt, choosing duplicate-avoidance. Tokenless/unknown records do not create persistent event notifications. App ordinary-mutation unresolved is not a cleanup notification.

Delivery is best-effort over bounded polling, not gap-free. Permission truth comes from the OS. Notifications have no sound, remain quiet in foreground and carry only public-safe route data.

## Schema v1 operator/lifecycle compatibility

V1 preserves existing CLI status/history/explain/doctor/pause/resume/retry/protect/unprotect/export behavior and the generation/instance-bound lifecycle commands:

- `arm`;
- `disarm`;
- `begin_drain`.

These commands depend on same-user socket access plus exact managed generation/instance and durable lifecycle gates. “Operator-only” is API ownership, not a separate privileged socket. Frontend schemas cannot encode them.

V1 may expose bounded internal diagnostic identities needed by CLI/service transactions. Ordinary App code must never decode/stringify v1 `DaemonStatus`. The frontend receipt invariant is scoped to v3/v4 mutations; v1 compatibility behavior remains intentionally separate.

## Version skew and installed boundary

Current source and installed generation 13 accept schemas 1, 3 and 4. The installed App emits schema v4 only and treats a v3-only or v1-only rollback endpoint as incompatible rather than silently downgrading. Schema v3 remains a transition endpoint for its existing commands; schema v1 remains operator-only and returns a trusted `unsupported_schema` envelope to a frontend request.

The service retains the prior snapshot and exact generation identity after candidate readiness, blocks mode/install/uninstall mutations during that lease, and exposes explicit report-only restart, accept and rollback commands. A historical real rollback restored generation 9, whose exact old CLI/daemon opened its v5 store and returned healthy ReadyReportOnly before generation 12 was reinstalled. Generation 13 is now accepted on SQLite v6 and has no pending lease. Its own lease was not executed before acceptance, so the next candidate must prove rollback to generation 13 with the exact old CLI/daemon, then reinstall as a fresh generation before acceptance.

## Verification anchors

- [`crates/unlinger-protocol/src/lib.rs`](../crates/unlinger-protocol/src/lib.rs): v4 DTOs/commands, v3 compatibility responses, receipts, envelopes and fixture decoders;
- [`crates/unlinger-daemon/src/store.rs`](../crates/unlinger-daemon/src/store.rs): SQLite v6 migration, event tokens, namespace/receipt/revision transactions;
- [`crates/unlinger-daemon/src/public_action_policy.rs`](../crates/unlinger-daemon/src/public_action_policy.rs): shared policy matrix;
- [`crates/unlinger-daemon/tests/history_store.rs`](../crates/unlinger-daemon/tests/history_store.rs): migration, atomicity, namespace, replay and recovery tests;
- [`crates/unlinger-daemon/tests/ipc_roundtrip.rs`](../crates/unlinger-daemon/tests/ipc_roundtrip.rs): v1/v2/v3/v4 routing, atomic browser projection, exact settlement identity, lifecycle serialization and raw-socket behavior;
- [`apps/UnlingerApp/Contract/v4`](../apps/UnlingerApp/Contract/v4): active atomic browser-product fixtures;
- [`apps/UnlingerApp/Contract/v3`](../apps/UnlingerApp/Contract/v3): transitional shared-command fixtures;
- [`apps/UnlingerApp/Tests/UnlingerAppTests`](../apps/UnlingerApp/Tests/UnlingerAppTests): strict decode, phase-aware transport, journal/reconciliation, concurrency/detail/notification/routing behavior.

Changing wire shape, requiredness, limits, error discriminators, readiness/freshness, mutation authority, redaction, socket boundary or installed compatibility requires source tests and this document in the same change.
