# Unlinger frontend contract v2

这里是 native frontend 唯一需要消费的 backend wire surface。Rust authority 是 [`crates/unlinger-protocol`](../../../crates/unlinger-protocol)，daemon projection 位于 [`crates/unlinger-daemon/src/public_ipc.rs`](../../../crates/unlinger-daemon/src/public_ipc.rs)，本目录 [`v2/`](v2/) 的 JSON 是由 Rust contract tests 逐份 decode/encode 的 canonical fixtures。

当前 source 同时接受两个 exact schema：

- `schema_version: 2`：frontend-only public DTO；ordinary commands only；Selen 使用这一层。
- `schema_version: 1`：现有 Rust CLI、service transaction 与 lifecycle compatibility surface；会携带 internal lifecycle facts，不用于 App。

当前安装中的 generation 9 仍是上一 source head 的 v1-only report-only runtime。v2 已在 source socket integration tests 中验证，但尚未 install/reload/activate。Frontend 开发先使用 fixtures；需要 live v2 时，启动一个 database/socket/instance-lock 全部隔离的 source report-only daemon，或等待未来明确安装的新 generation。不要把 v1 status 当作 v2 fallback。

## Transport

- per-user Unix-domain stream socket；无 TCP、account、telemetry 或 normal-operation network traffic；
- 每个 connection 恰好一个 LF-delimited JSON request 和一个 response；
- request 上限 64 KiB，response 上限 4 MiB；
- request envelope 是 `{"schema_version":2,"request_id":17,"command":{"command":"status"}}`；
- response 必须验证 exact `schema_version` 与 `request_id`，并验证 `ok/payload/error` 互斥；
- mutation 在可信 response 前 timeout、EOF 或 disconnect 时属于 delivery uncertain。绝不 automatic retry；先发 read-only command 读取 durable/runtime state。

## Ordinary commands

v2 只定义：

- `status`
- `history { limit }`
- `explain { incident_id }`
- `pause { duration_millis }`
- `resume`
- `retry_failed_cleanup { incident_id }`
- `protect_incident { incident_id }`
- `unprotect_incident { incident_id }`
- `export_diagnostics { incident_id }`

`arm`、`disarm`、`begin_drain` 不只是“不要显示”：它们在 v2 `Command` enum 中不存在。带这些 discriminator 的 v2 request 会得到 typed `invalid_json`。

## Status 与 capabilities

`PublicStatus` 只给 App 使用：daemon version、health/readiness、effective mode、scan/cleanup activity、pause、last scan、incident counts、recent reclaim、event-source/storage health、bounded attention、exact protections 与 explicit capabilities。

它不发送 daemon PID、instance ID、activation/armed generation、enforcement epoch、requested mode、database schema、binary/path identity、service transaction state、raw last error 或 recovery identity。

Frontend 不从 lifecycle facts 猜 action availability。使用 `capabilities.*.available`；disabled state 显示或映射 `unavailable_reason_id`。Incident detail 还会给出 exact incident 的 retry/protect/unprotect/export capabilities。

## Cleanup outcome

Cleanup timeline 同时保留 whole-plan `state` 和四个 frontend outcome fields：

- `process_outcome`: `cleared | revived | failed | delivery_unknown`
- `artifact_outcome`: `not_applicable | reconciled | residue | delivery_unknown`
- `overall_outcome`: `cleared | cleared_with_residue | revived | failed`
- `attention_required`: boolean

`state: FAILED` 与 `overall_outcome: cleared_with_residue` 可以同时存在：前者表示 frozen process+artifact plan 没有全部完成，后者表示 process tree 已经由 exact liveness + revival checks 证明清除，而 artifact 在明确、无不确定副作用的 refusal 后被安全保留。UI 必须说清“进程已处理，低风险 residue 被保留”，不能说 process cleanup failed。

任何 process/artifact `delivery_unknown`，以及已发生 side effect 后无法证明 terminal state 的 failure，仍是 `overall_outcome: failed` 并保留 fail-close/retry block。

## Public history redaction

v2 history/explain 保留 family/version、member/resource totals、role counts、evidence IDs/families、hard gates、typed outcome、signal stage/signal/disposition summaries、artifact kind/disposition 和 resource receipt。

它不发送 raw PID/survivor PID、event/attempt ID、process/artifact/member fingerprint、source PID、start identity、tracking/session identity、full argv、executable/profile path 或 frozen target。Diagnostics v2 也只组合 public status 与 public incident detail。

## Canonical fixtures

Status：

- `status-all-clear.json`
- `status-report-only.json`
- `status-scanning.json`
- `status-paused.json`
- `status-recently-reclaimed.json`
- `status-needs-attention.json`

Timeline/detail：

- `history-cleared.json`
- `history-cleared-with-residue.json`
- `incident-protected.json`
- `incident-revived.json`
- `incident-failed.json`

App-local transport states（不是 daemon response envelope）：

- `app-daemon-unavailable.json`
- `app-mutation-delivery-uncertain.json`

Contract verification：

```bash
cargo test -p unlinger-protocol
cargo test -p unlinger-daemon --test ipc_roundtrip frontend_schema_v2_
```
