# Unlinger frontend contract v3

这里是 native App 唯一消费的 backend wire surface。Rust authority 是 [`crates/unlinger-protocol`](../../../crates/unlinger-protocol)，daemon projection 位于 [`crates/unlinger-daemon/src/public_ipc.rs`](../../../crates/unlinger-daemon/src/public_ipc.rs)，active canonical fixtures 位于 [`v3/`](v3/) 并由 Rust 与 Swift tests 共同 decode。

协议边界：

- `schema_version: 3`：当前 App public contract；strict DTO、ordinary commands、durable mutation receipts；
- `schema_version: 1`：Rust CLI/service operator compatibility；包含 internal lifecycle facts，不供 App 使用；
- `schema_version: 2`：保留在 [`v2/`](v2/) 作为历史审计证据；它在安装前已被 v3 supersede，当前 server 对 v2 返回 v1-framed typed `unsupported_schema`；
- App 只发送 v3，不会 silent downgrade 或改走 v1 mutation/lifecycle path。

安装中的 generation 9 仍是 v1-only、report-only、unarmed。它不是 v3 live endpoint。Active App fixtures、isolated smoke 与 source tests 不改变 installed truth。

## Transport and trust

- per-user Unix-domain stream socket；无 TCP、account、telemetry 或 normal-operation network traffic；
- 每个 connection 恰好一个 LF-delimited JSON request 和一个 response；request 上限 64 KiB，response 上限 4 MiB；
- App request envelope 是 `{"schema_version":3,"request_id":17,"command":{"command":"status"}}`；
- response header 用 exact integer types 验证 `schema_version` 和 `request_id`，并验证 `ok/payload/error` 互斥；
- client 每个 request 只发送一次。Mutation request 任意字节可能写出后发生 timeout、EOF、reset、oversize、bad JSON、wrong schema/request ID/payload 或 DTO decode failure，都属于 delivery uncertain；绝不 automatic resend；
- encode/connect/明确 zero-byte write failure 是 failed-before-send；exact v3 error envelope 是 trusted rejection；仅 exact v1 `unsupported_schema` + exact request ID 可建立 incompatible-daemon truth。

## Commands

Read-only：

- `status`
- `history { limit }`
- `explain { incident_id }`
- `incidents`
- `mutation_status { context }`
- `export_diagnostics { incident_id }`

Ordinary mutations：

- `pause { context, duration_millis }`
- `resume { context }`
- `retry_failed_cleanup { context, incident_id }`
- `protect_incident { context, incident_id }`
- `unprotect_incident { context, incident_id }`

`arm`、`disarm`、`begin_drain`、install/update/rollback 不存在于 v3 `Command` enum。App 也不从 v1 response 或 shell command获得 daemon lifecycle authority。

## Durable mutation authority

每个新 v3 mutation 都携带：

```text
MutationContext {
  namespace_token
  mutation_id
}
```

`status.mutation_authority` 提供当前 public-safe namespace 和至少 14 天的 reconciliation window。Namespace 不复用 daemon instance、activation generation、enforcement epoch、database path 或 identity。

Backend 在同一 `BEGIN IMMEDIATE` transaction 内重读 exact policy facts、应用 state change、推进 durable policy revision并插入 typed receipt。Exact `(namespace_token, mutation_id)` replay 先于 current lifecycle policy：同一 canonical request返回 stored receipt且不重复 side effect/revision；不同 request返回 conflict。Receipt outcome 是 `applied | no_change | rejected`，`committed` 表示 outcome 已 durable，不表示 retry 后的 cleanup 已成功。

`mutation_status` 返回：

- `committed { receipt }`：stored outcome 是 authority；
- `not_found { context }`：只有 supplied namespace 仍是 current authority时，才证明本 namespace 下没有 commit；
- `authority_lost { context }`：receipt 缺失且 namespace 已失效；不能推断原 mutation 未发生。

Receipt pruning 与 namespace rotation在同一 transaction；14 天窗口内不得为 count cap提前删除。容量不足时拒绝新 mutation。Old namespace + missing receipt 的 mutation request必须被拒绝。

App 在 connect/send 前把 namespace、ID、canonical mutation、semantic lock、created time和 visual dismissal写入 owner-private crash-durable journal。Journal failure发送零请求；App restart只查 `mutation_status`，不重发原 mutation。Pre-v0.1 一次只允许一个 unresolved ordinary mutation；dismiss banner不清 journal或 lock，read-only commands继续工作。

## Status, readiness, and capabilities

`PublicStatus` 只给 App：daemon version、health、`starting | ready | draining | failed | unknown` readiness、effective mode、activity、pause、last completed scan、incident counts、recent reclaim、event/storage health、bounded attention/protection、global capabilities和 mutation authority。

它不发送 daemon PID、instance ID、activation/armed generation、enforcement epoch、requested mode、database schema、binary/path identity、service transaction state或 raw last error。

Capability projection和 mutation authorization调用同一 Rust public-action policy。UI 不从 lifecycle、incident stage、reason string或 score推导授权。Quiet UI 仅在 `healthy && readiness == ready && no attention && roster current && no activity` 成立。

## Observation roster

`incidents` 返回 `ObservationRoster`：

```text
cycle_token: optional
observed_at_unix_millis: optional
freshness: never_observed | scan_in_progress | current | stale_after_failure
items: bounded redacted observations
```

Never-observed 没有 fake timestamp/token。Replacement cycle完成前保留上一份 roster；失败后标 `stale_after_failure`。完整 observation batch 先 atomic commit history，再 publish fresh roster。Roster 是最近一次 observation projection，不是 work queue，不授权 action，也不提供 manual kill。

## Events, outcomes, and diagnostics

每个 retained public history event有 opaque、stable、unique `event_token`；recent reclaim和 attention在对应 durable event存在时携带同一个 token。Swift row identity与 notification dedupe使用 token，不使用 internal event/attempt IDs。Action row identity由 event token、receipt namespace和 stable sequence组成；artifact-only groups必须可渲染。

Cleanup 同时保留 process、artifact、overall outcome。`state: FAILED` 与 `overall_outcome: cleared_with_residue` 可以共存：process tree已证明清除，但 artifact安全保留。Typed stored outcome决定 attention/reclaim类别；reason string只负责 copy，缺失 outcome不能伪造成 cleared。

Diagnostics payload有 required integer `document_schema_version`。Swift 用 exact `CodingKeys`；导出结果属于 initiating view的 local state，不写共享 singleton。当前语义是 **semantic-lossless JSON**：未知 fields保留，允许重新序列化，不承诺原始 byte layout或 key order。

## Canonical fixtures and verification

[`v3/`](v3/) 包含 ready/report-only/enforce/starting/draining/failed/storage-recovered statuses，current/scanning/stale/never-observed rosters，history/detail/outcome/diagnostics，mutation committed/not-found/authority-lost，以及 App-local unavailable/incompatible/unresolved states。Bundled App 只包含 v3；bundle gate发现 active v2 fixture会失败。

```bash
cargo test -p unlinger-protocol
cargo test -p unlinger-daemon --test ipc_roundtrip frontend_schema_v3_
cd apps/UnlingerApp && swift test
```
