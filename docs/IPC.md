# 本地 IPC contract

Unlinger source接受四条exact local wire：

- `schema_version = 1`：Rust CLI、diagnosis 与 service lifecycle/operator compatibility；
- `schema_version = 3`：legacy-compatible frontend endpoint；
- `schema_version = 4`：transitional atomic browser-product endpoint；
- `schema_version = 5`：current native App contract，增加impact、storage residue与observation span facts。

`schema_version = 2`是从未安装或发布的历史frontend draft。其fixtures保留审计价值，但current server在dispatch前以schema-v1 framing返回typed `unsupported_schema`。没有schema negotiation、silent downgrade、App→v3 fallback或App→v1 fallback。

Rust authority：

- v1 envelope/commands/lifecycle：[`crates/unlinger-daemon/src/ipc.rs`](../crates/unlinger-daemon/src/ipc.rs)
- v5 DTO/envelope与v4/v3 compatibility contract：[`crates/unlinger-protocol/src/lib.rs`](../crates/unlinger-protocol/src/lib.rs)
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

V5 request example：

```json
{"schema_version":5,"request_id":17,"command":{"command":"browser_overview"}}
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

对frontend mutation（v3/v4/v5语义相同）：

- encode/connect/明确 zero-byte first write failure：`failed_before_send`；
- 任意 byte可能写出后发生 timeout、EOF、reset、oversize、bad JSON、wrong schema/request ID、contradictory envelope、wrong payload或 DTO decode failure：delivery uncertain；
- exact requested-schema error + exact request ID：trusted rejection；
- exact schema-v1 `unsupported_schema` + exact request ID：trusted incompatible daemon，frontend mutation未被旧endpoint dispatch；
- 永不 automatic resend。

同样的 post-send fault用于 read-only command时是 read/protocol failure，不建立 mutation uncertainty。Transport cancellation对 blocking POSIX I/O执行 shutdown/close；不会留下 MainActor blocking read。

## Frontend schemas v5, v4, and v3

### Read-only commands

| Command | Payload | Meaning |
| --- | --- | --- |
| `browser_overview` | none | v4/v5 atomic browser projection；v5另含impact与storage residue |
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

V3不接受`browser_overview`并返回v3-framed typed `invalid_request`；它不会返回v4/v5 payload或silent downgrade。V4保留原atomic overview及既有history/detail shape，不返回v5-only impact、storage residue或observation span。V3/v4/v5 command enum都没有`arm`、`disarm`、`begin_drain`、install、update或rollback。

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

V3/v4/v5 `PublicStatus` includes:

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

## Atomic browser product snapshot and impact

V4/v5 `browser_overview` replaces frontend composition of independently timed status/roster/history reads. `ControlPlane` first refreshes durable attention/protection, then captures status and the latest roster while holding the two in-memory locks as one source boundary; it performs no SQLite query under those locks. The shared projection contains generated/cycle/observation time, freshness, health, effective mode, pause deadline, phase, bounded sessions, typed compatibility and coverage, attention/protection, exact recent settlement and the rule-generated support catalog.

V5 additionally carries `impact` and optional `storage_residue`. Impact has tracking start, `complete | partial_backfill` history completeness, terminal cleanup count, proved process-reclaim count, and aggregate process/memory values only when every proved reclaim has that measurement. It comes from independent SQLite-v7 cleanup-impact authority, not bounded event history. Recent settlement resolves the exact event token directly against that authority; later failures cannot hide an earlier most-recent proved reclaim. Storage residue contains only typed kind/status/time/count/logical bytes/shape/reference/eligibility/reason IDs. Current `chrome_code_sign_clone` observations always have `automatic_cleanup_eligible: false`.

Positive phase requires healthy Ready state, no scan in progress, current roster and equal non-null status/roster observation time. Otherwise phase is `unknown`. Trusted precedence is `attention → reclaiming → confirmed → verifying → active → protected → clear`; Swift and CLI display it but do not recompute it.

Each session carries typed browser product, optional observed bundle version, compatibility decision and optional reason ID produced during deterministic rule analysis. Browser support is generated from the embedded packs with a readable `support_revision`; the App does not ship a second eligibility table.

Recent settlement starts from the exact public reclaim `event_token`, loads that exact impact row, and verifies incident/time/outcome agreement. Timestamp equality alone is never identity. If any link is absent or contradictory, the settlement field is absent; the frontend does not scan a bounded history page or invent a fallback.

## Public event identity, spans, impact, and outcomes

SQLite v7 assigns every retained event a unique opaque `public_token`. V1 internal event IDs remain unchanged and frontend schemas never expose them. History uses `event_token`; recent reclaim and attention carry it when they originate from that durable event. Swift action identity adds stable action sequence and receipt namespace where required, so repeated stage/kind and artifact-only rows remain distinct.

Equivalent consecutive observation rows coalesce under the latest event token. V5 observation events expose `observation_span { first_observed_at_unix_millis, observation_count }`; v4/v3 omit it and retain their earlier event shape. Cleanup events never become spans. Observation count/age retention cannot delete cleanup impacts or their at-least-14-day detail, while the lifetime aggregate remains after detail expiry. V6→v7 migration backfills retained terminal cleanup evidence and marks aggregate history `partial_backfill`; a fresh v7 database starts `complete`.

Cleanup projection keeps:

```text
process_outcome  = cleared | revived | failed | delivery_unknown | unknown
artifact_outcome = not_applicable | reconciled | residue | delivery_unknown | unknown
overall_outcome  = cleared | cleared_with_residue | revived | failed | unknown
attention_required
```

Typed stored outcome, not a reason-string prefix, determines attention/reclaim kind. Missing legacy outcome is unknown/omitted, never synthesized as cleared. Whole-plan `FAILED` may coexist with public `cleared_with_residue` when the process tree is proved gone and a known no-removal artifact disposition safely retains residue. Any delivery uncertainty or unproved post-side-effect result remains failed/fail-closed.

## Diagnostics and redaction

Frontend history/detail/diagnostics may include family/version, member/resource totals, role counts, evidence/gates, typed outcomes, signal stage/signal/disposition summaries, artifact kind/disposition, public event tokens and v5 observation spans. V4/v5 current-session compatibility adds only typed browser product/version/decision/reason to current in-memory projections; those app-bundle facts are not persisted into history. V5 storage residue contains no pathname or file content.

They exclude raw/survivor/source PIDs, internal event/attempt IDs, process/member/artifact fingerprints, tracking/session identity, full argv, executable/profile/artifact paths, frozen targets, lifecycle generation/epoch/instance and raw errors.

Diagnostics has required integer `document_schema_version`; the source App requires v5. The Swift decoder uses exact CodingKeys and export preserves unknown JSON fields semantically; it may reserialize whitespace/key order, so the contract is semantic-lossless, not byte-verbatim. Output uses owner-private crash-durable atomic file rules and remains an explicit local action.

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

V1 may expose bounded internal diagnostic identities needed by CLI/service transactions. Ordinary App code must never decode/stringify v1 `DaemonStatus`. The frontend receipt invariant is scoped to v3/v4/v5 mutations; v1 compatibility behavior remains intentionally separate.

## Version skew and installed boundary

Current source accepts schemas 1, 3, 4 and 5. The source App emits schema v5 only and treats a v4-, v3-, or v1-only endpoint as incompatible rather than silently downgrading. Schema v4 preserves its prior atomic overview and response shapes; schema v3 preserves its existing commands; schema v1 remains operator-only and returns a trusted `unsupported_schema` envelope to an unsupported frontend request.

Installed generation 19 accepts schemas 1, 3, 4 and 5, uses SQLite v7, and serves the installed schema-v5 App without downgrade. Schema v4 and v3 remain compatibility endpoints. The schema-v7 migration was previously verified by generation-16→15 rollback/open and fresh generation-17 installation; the observer repair separately verified generation-18→17 rollback/open on v7 before fresh generation-19 installation.

The service retains the prior snapshot and exact generation identity after candidate readiness, blocks mode/install/uninstall mutations during that lease, and exposes explicit report-only restart, accept and rollback commands. Historical real rollbacks restored generation 9 before generation 12 and generation 13 before generation 15. The v7 replacement separately installed candidate A as generation 16, restarted it report-only, rolled back to generation 15 and proved the exact old CLI/daemon reopened SQLite v6 healthy. The same candidate freshly reinstalled as generation 17, repeated restart/schema-v5 App checks, was accepted, and was then explicitly armed. The subsequent observer repair exercised generation-18 rollback to 17/v7 and fresh generation-19 install/restart/current App checks before acceptance and arm. Generation 19 has no pending lease.

## Verification anchors

- [`crates/unlinger-protocol/src/lib.rs`](../crates/unlinger-protocol/src/lib.rs): v5 DTOs/commands, v4/v3 compatibility responses, receipts, envelopes and fixture decoders;
- [`crates/unlinger-daemon/src/store.rs`](../crates/unlinger-daemon/src/store.rs): SQLite v7 migration, observation spans, independent impact authority, storage residue, event tokens, namespace/receipt/revision transactions;
- [`crates/unlinger-daemon/src/public_action_policy.rs`](../crates/unlinger-daemon/src/public_action_policy.rs): shared policy matrix;
- [`crates/unlinger-daemon/tests/history_store.rs`](../crates/unlinger-daemon/tests/history_store.rs): migration, atomicity, namespace, replay and recovery tests;
- [`crates/unlinger-daemon/tests/ipc_roundtrip.rs`](../crates/unlinger-daemon/tests/ipc_roundtrip.rs): v1/v2/v3/v4/v5 routing, atomic browser projection, impact/residue/span compatibility, exact settlement identity, lifecycle serialization and raw-socket behavior;
- [`apps/UnlingerApp/Contract/v5`](../apps/UnlingerApp/Contract/v5): current impact/residue/span fixtures;
- [`apps/UnlingerApp/Contract/v4`](../apps/UnlingerApp/Contract/v4): transitional atomic browser-product fixtures;
- [`apps/UnlingerApp/Contract/v3`](../apps/UnlingerApp/Contract/v3): legacy-compatible shared-command fixtures;
- [`apps/UnlingerApp/Tests/UnlingerAppTests`](../apps/UnlingerApp/Tests/UnlingerAppTests): strict decode, phase-aware transport, journal/reconciliation, concurrency/detail/notification/routing behavior.

Changing wire shape, requiredness, limits, error discriminators, readiness/freshness, mutation authority, redaction, socket boundary or installed compatibility requires source tests and this document in the same change.
