# 本地 IPC contract

本文锁定 Unlinger 当前 source 的两条本地 IPC wire surface：`schema_version = 2` 是 frontend-facing public contract，`schema_version = 1` 是现有 Rust CLI 与 service lifecycle 的 compatibility/operator contract。Rust DTO authority 分别是 [`crates/unlinger-protocol`](../crates/unlinger-protocol) 与 [`crates/unlinger-daemon/src/ipc.rs`](../crates/unlinger-daemon/src/ipc.rs)；v2 projection authority 是 [`crates/unlinger-daemon/src/public_ipc.rs`](../crates/unlinger-daemon/src/public_ipc.rs)。

`status`、`history`、`explain`、`pause`、`resume`、`retry_failed_cleanup`、`protect_incident`、`unprotect_incident` 和 `export_diagnostics` 是 v2 ordinary client surface。`arm`、`disarm` 和 `begin_drain` 只存在于 v1 service lifecycle protocol；它们不进入 v2 command enum。

## Transport 与 trust boundary

- Transport 是 per-user Unix-domain stream socket，不监听 TCP，也没有 normal-operation network traffic。
- 默认 socket 位于 effective user home 下的 `Library/Application Support/Unlinger/run/unlingerd.sock`。路径从进程的 effective UID 通过 account database 解析，不信任 `HOME` environment；root 被拒绝。
- CLI 的 ordinary IPC commands 可以用全局 `--socket <path>` 覆盖 socket。`service` lifecycle commands 不接受这个 override。
- socket parent 必须是当前 effective user 拥有、非 symlink、mode 恰为 `0700` 的 directory。socket 自身设为 `0600`。
- server 用 `getpeereid` 验证 peer UID 必须等于 daemon effective UID。协议的 trust model 是“同一 local account”，不是多租户 authorization；同一用户下的其他进程若能连接 socket，就处在该 trust boundary 内。
- 每次 connection 只处理一个 request 和一个 response；client 应为每个 request 建立新 connection，不要把它当作 persistent stream。
- 当前 ordinary/default Rust client 与 service lifecycle client 都对单次 request 使用 15 秒 read/write timeout；server 最多并发处理 8 个已经 accept 的 connection，每个 connection 使用 3 秒 read/write timeout。一个慢 `history` 或只写了部分 request 的 peer 因此不会 head-of-line block 所有后续 status/lifecycle traffic；超过 worker bound 的 connection 可以在没有 JSON envelope 的情况下被关闭。连接失败、timeout、peer UID 不匹配或 daemon 不在时，同样不保证存在 JSON error response。
- `IpcClient::request` 每次只发送一次，不做 automatic retry。`request_id` 只是 correlation ID，不是幂等或去重 key。任何 mutation 在 response 前 timeout/断线时都属于 uncertain delivery：daemon 可能尚未处理，也可能已经 commit 但 response 迟到；caller 必须先读取对应 durable/runtime state，不能盲目 resend。

## Frontend schema v2

Source server 按 request envelope 的 exact `schema_version` 路由，不做隐式 negotiation。v2 request/envelope 的 framing、limits、single-shot/uncertain-delivery 规则与 v1 相同，但 command 和 payload 由 `unlinger-protocol` 独立定义。

v2 ordinary status 是 `PublicStatus`，只含 version、health/readiness、effective mode、activity、pause、last scan、incident counts、recent reclaim、event-source/storage health、bounded attention/protection 与 explicit action capabilities。它不发送 daemon PID、instance ID、activation/armed generation、enforcement epoch、requested mode、database schema、binary/path identity、service transaction state、raw last error 或 recovery identity。

v2 history/explain 删除 event/attempt IDs、raw/survivor PIDs、source PID、process/member/artifact fingerprints、start identity 和 frozen targets；保留 family/version、member/resource totals、roles、typed evidence/gates、redacted incident ID、action stage/signal/disposition summaries 与独立 cleanup outcomes。Diagnostics v2 只组合 public status 与 public incident detail。

Cleanup projection 同时给出 whole-plan `state` 以及：

```text
process_outcome  = cleared | revived | failed | delivery_unknown
artifact_outcome = not_applicable | reconciled | residue | delivery_unknown
overall_outcome  = cleared | cleared_with_residue | revived | failed
attention_required = boolean
```

`state: FAILED` 与 `overall_outcome: cleared_with_residue` 可以并存：whole frozen plan 未全部完成，但 exact process liveness 和 revival checks 已证明 process tree 清除，artifact 在明确 refusal 后被安全保留。`delivery_unknown` 或 side effect 后无法证明 terminal state 的 failure 仍保持 overall failed 与 fail-close。

Canonical frontend contract、fixtures 与 source/live boundary 见 [`apps/UnlingerApp/Contract/README.md`](../apps/UnlingerApp/Contract/README.md)。当前 installed generation 9 仍是 v1-only report-only runtime；v2 是 source-complete、socket-tested、尚未 installed/activated 的 contract。

## Schema v1 framing 与 envelope

本节起记录 v1 compatibility/operator shape。它仍供 Rust CLI、service transaction、旧 installed runtime 与低层 diagnosis 使用，不是新 App 的 DTO surface。

每条 message 是一个 UTF-8 JSON value，后接单个 LF (`\n`)。JSON 不跨多行；一条 connection 上没有第二条 message。

Request line 上限是 64 KiB，serialized JSON 与 terminating LF 都计入；server 读取到第一处 LF，并拒绝超过上限、空 message 或读取失败的 request。Rust client 同样按包含 LF 的整行将 response 限制为 4 MiB。Frontend 不应依赖 EOF 代替 LF，即使当前 reader 在部分 EOF 情形可能接受已有 bytes。

### Request envelope

```json
{
  "schema_version": 1,
  "request_id": 17,
  "command": {
    "command": "resume"
  }
}
```

字段 contract：

| Field | Type | Meaning |
| --- | --- | --- |
| `schema_version` | `u32` JSON integer | 当前只接受 `1`。 |
| `request_id` | `u64` JSON integer | client 生成的 correlation ID；server 在可解析的 response 中原样回显。它不持久化，也不要求跨 process 唯一。JavaScript client 应限制在 safe integer 范围。 |
| `command` | object | `IpcCommand`。注意 envelope field 与内部 discriminator 都叫 `command`，所以 wire 是 `"command":{"command":"status"}`，不是扁平的 `"command":"status"`。 |

### Success envelope

```json
{
  "schema_version": 1,
  "request_id": 17,
  "ok": true,
  "payload": {
    "type": "resumed"
  }
}
```

Success 必须是 `ok: true`、存在 `payload`、不存在 `error`。`payload.type` 是 snake_case discriminator；有 data 的 variant 使用 `payload.data`，unit variant `resumed` 没有 `data`。

### Error envelope

```json
{
  "schema_version": 1,
  "request_id": 18,
  "ok": false,
  "error": {
    "code": "not_found",
    "message": "incident was not found"
  }
}
```

Failure 必须是 `ok: false`、存在 `error`、不存在 `payload`。Frontend 只应按 `error.code` 分支；`message` 是 bounded human/debug text，不是稳定的 machine discriminator。

若 request 在 envelope 解析前失败，server 使用 `request_id: 0`，因为它还无法信任 caller 提供的 ID。若只是不支持 `schema_version`，response 会回显已经解析出的 request ID。

## Ordinary commands

下表中的 request fields 都位于 nested `command` object 内。

| Wire command | Request fields | Success `payload.type` | `payload.data` | Semantics |
| --- | --- | --- | --- | --- |
| `status` | none | `status` | `DaemonStatus` object | 读取 lifecycle、health、activity、attention 与最近状态。读取时也会清除已经到期的 persisted pause。 |
| `history` | `limit: usize` | `history` | `HistoryEvent[]` | 最近事件，按 timestamp 与 event ID 倒序。Daemon wire 接受 `0..=1000`；`0` 返回空数组。CLI 把 ordinary input 限制为 `1..=1000`，默认 `50`；frontend 应沿用 `1..=1000`。 |
| `explain` | `incident_id: string` | `incident` | `IncidentDetail` object | 返回一个 incident 的 retained timeline；不存在时为 `not_found`。 |
| `pause` | `duration_millis: u64` | `pause` | `{ "until_unix_millis": u64 }` | 持久化暂停 automatic cleanup；observation 与 local history 继续。duration 必须为 `1..=2592000000` ms（30 天），deadline 还必须能用 `u64` 表示。 |
| `resume` | none | `resumed` | none | 清除 persisted pause。它不改变 requested/effective service mode。 |
| `retry_failed_cleanup` | `incident_id: string` | `retry_scheduled` | `{ "incident_id": string }` | 只对一个 durable failed/revived cleanup block 授权 retry，同时清除该 incident 的 cooling candidate 并推进 cleanup-policy revision；它不会立刻 signal，必须重新经过完整 cooling 与 safety gates。没有 block 时为 `not_found`。 |
| `protect_incident` | `incident_id: string` | `incident_protected` | `{ "protection": ProtectedIncidentSummary }` | 为一个已有 observation 的 exact incident 建立 owner override。相同 exact protection 的重复 request 是 idempotent；未观察过时为 `not_found`。它不是 family/profile/executable whitelist。 |
| `unprotect_incident` | `incident_id: string` | `incident_unprotected` | `{ "incident_id": string }` | 删除一个 exact protection override；不存在时为 `not_found`。删除 override 不跳过其他 safety/cooling gates。 |
| `export_diagnostics` | `incident_id: string` | `diagnostics` | `DiagnosticsBundle` object | 返回 `schema_version: 1`、generation timestamp、当前 status 与 retained incident detail。CLI 是否写 stdout 或一个 private file 是 IPC 之外的 client behavior。 |

所有接收 `incident_id` 的 command 都要求 UTF-8 string 的 encoded length 为 1 到 128 bytes。当前 wire 不另行限制字符集，也不 trim whitespace；frontend 不应自行改写从 daemon/history 获得的 ID。

### Ordinary request examples

```json
{"schema_version":1,"request_id":21,"command":{"command":"status"}}
```

```json
{"schema_version":1,"request_id":22,"command":{"command":"history","limit":50}}
```

```json
{"schema_version":1,"request_id":23,"command":{"command":"explain","incident_id":"inc-example"}}
```

```json
{"schema_version":1,"request_id":24,"command":{"command":"pause","duration_millis":7200000}}
```

```json
{"schema_version":1,"request_id":25,"command":{"command":"retry_failed_cleanup","incident_id":"inc-example"}}
```

```json
{"schema_version":1,"request_id":26,"command":{"command":"protect_incident","incident_id":"inc-example"}}
```

```json
{"schema_version":1,"request_id":27,"command":{"command":"unprotect_incident","incident_id":"inc-example"}}
```

```json
{"schema_version":1,"request_id":28,"command":{"command":"export_diagnostics","incident_id":"inc-example"}}
```

## Status contract

`payload.type = "status"` 与 internal lifecycle responses 中的 `payload.type = "lifecycle"` 都携带同一个 `DaemonStatus` shape。

### Lifecycle 与 mode

| Field | Type | Frontend interpretation |
| --- | --- | --- |
| `lifecycle_schema_version` | `u32` | 当前 lifecycle projection 为 `1`。`0` 表示 legacy projection。 |
| `ipc_schema_version` | `u32` | 当前为 `1`。 |
| `database_schema_version` | `u32` | 当前 opened history schema version；用于诊断，不是 frontend migration instruction。 |
| `daemon_version` | string | Runtime package version。 |
| `managed` | boolean | 是否由 activation generation 管理。 |
| `instance_id` | string | 当前 daemon instance 的 internal identity；不要作为 user-facing label 或 telemetry/log field。 |
| `activation_generation` | optional `u64` | 当前 managed generation；unmanaged 时省略。 |
| `armed_generation` | optional `u64` | 当前获准 enforce 的 generation；未 armed 时省略。 |
| `enforcement_epoch` | optional string | 当前 enforcement epoch；internal/sensitive，frontend 不显示、不记录。 |
| `startup_state` | enum string | `legacy`, `booting`, `recovering`, `first_scan_report_only`, `ready_report_only`, `ready_enforce`, `draining`, `failed`。 |
| `healthy` | boolean | reconciliation/control runtime 当前是否健康。它本身不等于 ready，也不证明 enforce 已 armed。 |
| `ready` | boolean | managed startup 是否通过 first-scan/lifecycle ready boundary。仍需结合 `healthy`、`draining` 与 `startup_state`。 |
| `draining` | boolean | service lifecycle 正在 drain；此时不得视为 ready 或 signal-authorized。 |
| `requested_mode` | `report_only` or `enforce` | durable/current lifecycle intent。它可以在 boot/recovery/re-arm transition 中暂时不同于 effective mode。 |
| `effective_mode` | `report_only` or `enforce` | 当前 runtime mode。Frontend 展示“现在会做什么”时使用它，不能用 requested mode 替代。 |
| `mode` | `report_only` or `enforce` | legacy compatibility alias；lifecycle schema 非零时不要把它当成第三种 mode truth。 |
| `recovered_cleanup_attempts` | `usize` JSON integer | startup recovery 投影出的 interrupted attempts 数量。 |

Managed daemon 的 ordinary readiness 判断应至少要求：

```text
healthy
&& ready
&& !draining
&& startup_state in {ready_report_only, ready_enforce}
```

`requested_mode = enforce` 绝不单独代表 signals 已获准。当前 managed signal gate 还要求 `effective_mode = enforce`、`armed_generation == activation_generation`、当前 `enforcement_epoch` 匹配，并且 daemon ready 且非 draining。Frontend 只展示这些事实，不自行重建或触发 signal authorization。

Service install/replacement 对“稳定 ready report-only”的接受比 ordinary display predicate 更严格：除 exact launchd/IPC PID、generation、binary、permissions 与上面的 lifecycle facts 外，还要求 `scan_in_progress = false` 且 `cleanup_in_progress = false`。因此 terminal receipt 刚出现但 cycle/global fail-close 仍在完成时，不会被 transaction 误读成可安全替换或完成的短暂 ReadyReportOnly。

Same-generation restart 可能携带 durable `requested_mode = enforce`，但 replacement instance 会先回到 `effective_mode = report_only`、unarmed、无 epoch、not ready。它的 first scan 始终是 report-only；只有 first scan 成功、generation/instance 仍精确匹配且没有 open cleanup attempt 或 delivery-unknown blocker 时，canonical completion transaction 才会进入 `ready_enforce` 并生成 fresh epoch。若 blocker 存在，transaction 会清除 carried request 并进入 durable `ready_report_only`，不会留下稍后自行生效的 latent enforce intent。New generation、explicit `disarm` / `begin_drain` 与 fatal startup failure 同样清除该 intent。

`startup_state = failed` 对当前 instance 是 terminal state，不是另一个 first-scan phase。后续 successful observation、`disarm` 或 stale durable ReadyEnforce 都不能把它原地 rehabilitate；`disarm` 只允许重试 durable fail-close并保持 unhealthy/not-ready/Failed。Service recovery 必须用 exact generation/instance response 进入 `begin_drain`，验证 Draining 后 bootout captured process，再由 fresh instance 从 report-only recovery/first scan 开始。

对于 `lifecycle_schema_version = 0` 的 legacy status，`mode` 是 effective-mode fallback；ordinary frontend 不应凭空构造缺失的 managed lifecycle facts。

### Activity、pause 与 scan projection

| Field | Type | Meaning |
| --- | --- | --- |
| `pid` | `u32` | 当前 daemon PID；local-sensitive，不作为 user-facing identity。 |
| `scan_in_progress` | boolean | 当前是否在 reconciliation scan。 |
| `cleanup_in_progress` | boolean | 当前是否在 cleanup executor。 |
| `paused_until_unix_millis` | optional `u64` | automatic cleanup pause deadline；未暂停时省略。Observation/history 不随 pause 停止。 |
| `last_scan_at_unix_millis` | optional `u64` | 最近完成或记录的 scan wall timestamp；尚无 scan 时省略。 |
| `confirmed_incidents` | `usize` JSON integer | 最新 projection 的 confirmed count。 |
| `ambiguous_incidents` | `usize` JSON integer | 最新 projection 的 ambiguous count。它本身不是 notification authorization。 |
| `most_recent_reclaim` | optional object | `{ incident_id, occurred_at_unix_millis, state }`；只来自 retained history 中最近的 `CLEARED` cleanup。 |

### Event source、storage 与 attention

| Field | Type | Meaning |
| --- | --- | --- |
| `event_source_healthy` | boolean | native scheduling hints 是否健康。`false` 时 periodic reconciliation 仍继续；它是 degraded 状态，不是 automatic cleanup eligibility。 |
| `last_event_source_error` | optional string | 当前实现只投影 bounded generic degraded message；健康时省略。 |
| `storage_recovery` | optional object | `recovery_id`, `occurred_at_unix_millis`, `reason`, `quarantined_sidecar_count`。`reason` 为 `integrity_check_failed` 或 `required_schema_invalid`。`recovery_id` 不显示、不记录。 |
| `attention` | object | `{ blocked_cleanup_count, items }`。count 是 durable blocked cleanup 总数；items 是 bounded projection。 |
| `protected_incident_count` | `usize` JSON integer | exact protection 总数。 |
| `protected_incidents` | array | 最近的 bounded `ProtectedIncidentSummary` projection。 |
| `last_error` | optional string | bounded runtime detail，可能含 local-sensitive diagnostic context；ordinary frontend 只显示有/无或受控 summary，不直接渲染或遥测。 |

`attention.items` 最多 16 项；`protected_incidents` 也最多 16 项。两者都没有独立 `truncated` boolean，应分别比较 `blocked_cleanup_count` / `protected_incident_count` 与 array length。不要把 bounded list 当成完整集合。

一个 attention item 的 shape 是：

```json
{
  "kind": "cleanup_failed",
  "reason_id": "cleanup.delivery_unknown",
  "incident_id": "inc-example",
  "state": "FAILED",
  "occurred_at_unix_millis": 1780000000000
}
```

`kind` 为 `cleanup_failed`、`cleanup_revived`、`daemon_unhealthy`、`event_source_degraded` 或 `storage_recovered`。`incident_id`、`state`、`occurred_at_unix_millis` 都是 optional，缺失时字段省略。Frontend 按 `kind` 与 `reason_id` 映射文案，不把 raw error string 当分类依据。

`ProtectedIncidentSummary` 的 shape 是：

```json
{
  "incident_id": "inc-example",
  "protected_at_unix_millis": 1780000000000,
  "last_exact_observed_at_unix_millis": 1780000001000
}
```

`last_exact_observed_at_unix_millis` 与 `exact_absence_since_unix_millis` 都是 optional；未知或不适用时字段省略。

## History 与 explain records

`history` 返回 `HistoryEvent[]`。`explain` 返回：

```json
{
  "incident_id": "inc-example",
  "events": []
}
```

实际存在的 incident 通常有非空 events；上例只展示 object shape。每个 `HistoryEvent` 为：

| Field | Type | Meaning |
| --- | --- | --- |
| `event_id` | `i64` JSON integer | SQLite event identity；只在本地 retained history 内有意义。 |
| `attempt_id` | optional `i64` | cleanup attempt identity；observation 通常省略。 |
| `incident_id` | string | redacted incident identity。 |
| `occurred_at_unix_millis` | `u64` | event wall timestamp。 |
| `kind` | `observation` or `cleanup` | Event category。 |
| `state` | `IncidentState` | `PROTECTED`, `ACTIVE`, `COOLING`, `CONFIRMED`, `AMBIGUOUS`, `RECLAIMING`, `CLEARED`, `REVIVED`, `FAILED`。注意这里是 uppercase。 |
| `payload` | object | `record_type` discriminator 为 `observation` 或 `cleanup`。 |

Observation payload 是 `{"record_type":"observation","report":{...}}`。Report 包含 typed/redacted fields：`incident_id`, signature pack/version, state, root summary, member/resource totals, member fingerprint, role counts, evidence IDs/families 与 hard-gate ledger。它不序列化 internal `tracking_key`、`session_fingerprint`、frozen process targets 或 runtime artifact paths。

Cleanup payload 是 `{"record_type":"cleanup","receipt":{...}}`。Receipt 包含 `incident_id`, terminal state, optional `reason_id`, signal actions, optional artifact actions, survivor PIDs, completed revival-check count 与 bounded resource snapshots。Enum values 的 casing 由 serialized Rust types 决定：cleanup stages/signals/dispositions、process roles、evidence families 和 artifact kinds/dispositions 都是 snake_case；incident states 始终 uppercase。

Frontend 应把 unknown `reason_id` 与 evidence ID 当作可显示的 opaque identifier，并提供 generic fallback。不要从 ID text 推导额外 authorization。

## Diagnostics payload

`payload.type = "diagnostics"` 的 `data` 为：

```text
DiagnosticsBundle {
  schema_version: 1,
  generated_at_unix_millis: u64,
  status: DaemonStatus,
  incident: IncidentDetail
}
```

Bundle 的 `schema_version` 是 diagnostics document schema，不替代 outer IPC schema。CLI 将 bundle 写文件时默认拒绝覆盖，只有显式 `--force` 才替换 exact path，并要求 current-user regular file、拒绝 symlink、最终 mode 为 `0600`。这些 filesystem rules 不是 server response fields。

## Error codes 与 limits

| `error.code` | Produced when |
| --- | --- |
| `invalid_request` | request line 为空、超过 64 KiB、read timeout 或其他 framing/read failure；`request_id` 为 `0`。 |
| `invalid_json` | line 不是可反序列化的 request envelope / command shape，包括未知 command variant 或缺失 required field；`request_id` 为 `0`。 |
| `unsupported_schema` | envelope 可解析，但 `schema_version != 1`；response 回显 request ID，并声明当前 supported schema。 |
| `invalid_argument` | command parameter 违反 bounds/identity precondition，或 store 拒绝一个 invalid/range input。 |
| `not_found` | incident、retry block 或 exact protection 不存在于该 command 所需状态。 |
| `store_error` | SQLite/persistence operation 未能完成。 |
| `unavailable` | daemon control/lifecycle state 无法提供该 operation，例如 managed identity 不可用。 |

Server 将 structured error `message` 限制为最多 512 Unicode scalar values。Frontend 不应假设 message language、punctuation 或完整底层 error 会稳定。

协议层还存在没有 JSON body 的 transport failures，例如 socket 不存在、peer 被拒绝、连接在 response 前关闭或 client-side 4 MiB limit 被触发。Frontend 应把它们与 daemon 返回的 structured `error.code` 分开建模。

## Internal-only lifecycle controls（v1 only）

下列 variants 被保留给 `unlinger service` transaction 与 daemon lifecycle tests。Ordinary frontend 不发送它们，也不提供等价按钮。它们尤其不能在 client timeout 后自动 resend；缺少 response 时必须重新读取 exact lifecycle identity/state，让 service transaction 进入既有 fail-closed recovery：

| Wire command | Fields | Success payload | Boundary |
| --- | --- | --- | --- |
| `arm` | `activation_generation: u64`, `instance_id: string` | `type: "lifecycle"`, data 为 `DaemonStatus` | 只对 exact active managed generation/instance；是否可 arm 还由 durable startup/recovery blockers 决定。 |
| `disarm` | same | `type: "lifecycle"` | 将 managed service durable/effective state 带回 report-only，并移除 signal authorization。 |
| `begin_drain` | same | `type: "lifecycle"` | 开始 exact instance drain，停止把 daemon 视为 ready/enforce-authorized。 |

这些 commands 没有独立的第二 socket 或 capability token；它们依赖 current-user socket boundary、exact generation/instance matching 和 service transaction contract。因此“internal-only”是产品/API ownership boundary，不是对同一账户内恶意进程的强隔离。Frontend 即使从 raw status 看到了 generation/instance，也不得重放 lifecycle commands。

## Redaction 与 frontend handling

IPC/history/diagnostics 使用 typed projection，明确不发送 raw argv、executable paths、profile paths、page contents、credentials、cookies、session fingerprint、tracking key 或 frozen signal target list。

但“redacted”不等于“可公开”：v1 raw IPC 仍可包含 daemon/incident/process PIDs、incident IDs、event/attempt IDs、redacted identity/member/artifact fingerprints、`instance_id`、`enforcement_epoch`、`recovery_id` 和 bounded `last_error`。这些值只用于 local correlation 与 diagnosis：

- 不放入 analytics、telemetry、crash reporting、remote logs 或 clipboard-by-default flows；
- ordinary UI 不展示 `instance_id`、`enforcement_epoch`、`recovery_id`、raw `last_error` 或 process identity fingerprints；
- human-readable status 可展示 lifecycle state、generation、health、requested/effective mode、counts 与 typed attention reasons；
- export 必须是 explicit user action，并继续使用 private local-file boundary。

当前 CLI `status` human output 会隐藏 PID、instance/recovery IDs、incident IDs in attention、private paths 和 raw last-error detail；`doctor` 还会生成更窄的 sanitized projection。Frontend 不读取或 stringify v1 `DaemonStatus`；它只消费 v2 public DTO。

## Compatibility rules

- 当前没有 schema negotiation：client 发送 exact schema。Source server 接受 v1/v2；installed generation 9 只接受 v1。Frontend 要求 v2，不 silent downgrade 到 v1。
- Client 必须验证 response `schema_version` 与 request ID；mismatch 是 protocol error，不是可接受的 stale response，也不是 retry authorization。
- Timeout、EOF 或 protocol error 不证明 request 未执行。对 pause/resume/retry/protect/unprotect 与 lifecycle mutation，缺失可信 response 时先 read back；不要用同一个或新 request ID 自动重发。
- `ok/payload/error` 必须成对一致。`ok: true` 只允许 payload；`ok: false` 只允许 error。
- JSON object field order 没有意义。当前 Rust deserializer 会忽略 struct 上未知 fields，但 unknown command/payload enum variants 会失败；frontend 不应把这一点当成未来 compatibility guarantee。
- Optional fields 在 `None` 时通常直接省略，不发送 `null`。Reader 应将“省略”解释为 absent/unknown，而不是 zero/empty。
- `usize`, `u64` 与 `i64` 都作为 JSON integers 发送。JavaScript frontend 对 event/attempt/request IDs 应避免 lossy arithmetic；当前现实值在 safe range 内仍应按 opaque local IDs 使用。
- `reason_id`、evidence IDs 和未来新增的 typed enum values 应有 generic UI fallback。未知值不能自动扩大 cleanup、notification 或 lifecycle authorization。

## Verification anchors

当前 contract 由以下 source/tests 覆盖：

- [`crates/unlinger-protocol/src/lib.rs`](../crates/unlinger-protocol/src/lib.rs)：v2 ordinary commands、public DTO、envelopes、outcome 与 canonical fixture decoders；
- [`crates/unlinger-daemon/src/public_ipc.rs`](../crates/unlinger-daemon/src/public_ipc.rs)：internal-to-public projection 与 field removal；
- [`crates/unlinger-daemon/src/ipc.rs`](../crates/unlinger-daemon/src/ipc.rs)：envelopes、commands、payloads、limits、socket/peer boundary 与 error codes；
- [`crates/unlinger-daemon/src/paths.rs`](../crates/unlinger-daemon/src/paths.rs)：effective-user path semantics；
- [`crates/unlinger-daemon/tests/ipc_roundtrip.rs`](../crates/unlinger-daemon/tests/ipc_roundtrip.rs)：socket mode、status/pause/resume、typed retry/protection errors、managed lifecycle identity、same-generation re-arm、bounded attention 与 slow-partial-peer concurrency；v2 tests 锁定 public status、history redaction 与 lifecycle-command absence，v1 golden test 保留 compatibility envelope；
- [`apps/UnlingerApp/Contract/v2`](../apps/UnlingerApp/Contract/v2)：Rust roundtrip 的 public wire/app-state fixtures；
- [`crates/unlinger-cli/src/main.rs`](../crates/unlinger-cli/src/main.rs)：ordinary CLI mapping、human-output redaction、doctor projection 与 private diagnostics output。

修改 command/payload shape、limit、error discriminator、mode/readiness semantics、redaction surface、socket trust boundary 或 default path 时，source tests 与本文必须在同一 change 中更新。
