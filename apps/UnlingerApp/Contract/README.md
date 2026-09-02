# Unlinger frontend contract v4

这里是native App消费的backend wire surface。Rust authority是[`crates/unlinger-protocol`](../../../crates/unlinger-protocol)，daemon projection位于[`crates/unlinger-daemon/src/public_ipc.rs`](../../../crates/unlinger-daemon/src/public_ipc.rs)。[`v4/`](v4/)保存active browser-product fixtures；[`v3/`](v3/)保存仍受source daemon支持的transitional shared-command fixtures。两组都由Rust与Swift tests共同decode。

协议边界：

- `schema_version: 4`：current source App contract；新增atomic `browser_overview`，其余public DTO、ordinary commands与durable mutation semantics继承v3；
- `schema_version: 3`：transitional compatibility endpoint；原有request必须保留v3 response schema与meaning，但`browser_overview`返回typed `invalid_request`，不得downgrade或拼装替代payload；
- `schema_version: 1`：Rust CLI/service operator compatibility；包含internal lifecycle facts，不供App使用；
- `schema_version: 2`：保留在[`v2/`](v2/)作为历史审计证据；当前server在dispatch前以schema-v1 framing返回typed `unsupported_schema`；
- source App只发送v4，不会silent downgrade到v3或改走v1 mutation/lifecycle path。

安装中的accepted generation 15提供schema-v4 App endpoint、transitional v3、operator v1与SQLite v6；当前是healthy `ReadyEnforce`、exact generation/epoch-bound、`0.3.0` process-only且没有pending lease。Matching v4 App已经安装。App仍只能投影backend activation truth，不能自行赋予或扩大signal authority。

## Transport and trust

- per-user Unix-domain stream socket；无TCP、account、telemetry或normal-operation network traffic；
- 每个connection恰好一个LF-delimited JSON request和一个response；request上限64 KiB，response上限4 MiB；
- source App request envelope是`{"schema_version":4,"request_id":17,"command":{"command":"browser_overview"}}`；
- response header用exact integer types验证`schema_version`和`request_id`，并验证`ok/payload/error`互斥；
- client每个request只发送一次。Mutation request任意字节可能写出后发生timeout、EOF、reset、oversize、bad JSON、wrong schema/request ID/payload或DTO decode failure，都属于delivery uncertain；绝不automatic resend；
- encode/connect/明确zero-byte write failure是failed-before-send；exact v4 error envelope是trusted rejection；仅exact v1 `unsupported_schema` + exact request ID可建立incompatible-daemon truth。

## Commands

Read-only：

- `browser_overview`（v4 only）
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

`arm`、`disarm`、`begin_drain`、install/update/rollback不存在于任何frontend `Command` enum。App也不从v1 response或shell command获得daemon lifecycle authority。

## Atomic browser overview

V4 `BrowserOverviewSnapshot`是browser-first surface的唯一product truth，包含：

- snapshot generation time、cycle token、observation time、freshness、health、effective mode与pause deadline；
- server-owned `unknown | clear | active | verifying | confirmed | reclaiming | protected | attention` phase；
- bounded current sessions及typed `product`、observed version、`automatic | observe_only | protected | unknown` compatibility、optional reason ID与capability；
- typed coverage summaries、attention、saved protections与exact recent settlement；
- 从embedded rule packs生成的family/product/admitted-version/automatic-action support catalog及`support_revision`。

Daemon在同一个in-memory status+roster lock boundary内capture source facts，释放锁后完成public projection。Positive phase要求healthy/ready、current roster、no scan和相等的non-null observation time；任何不可信或不一致状态都fail closed为`unknown`。可信状态使用一份server truth table：attention → reclaiming → confirmed → verifying → active → protected → clear。

Compatibility来自rule analysis的typed app-bundle facts，不从UI evidence strings重建。Recent settlement用exact durable cleanup `event_token`定位receipt，再按更小的event ID找该incident最近的observation；即使多个event timestamp相同也不会误配。缺失proof返回nil，不产生猜测或bounded-history fallback。Swift `BrowserOverviewMapper`只负责localization和presentation，不得重算上述事实。

## Durable mutation authority

每个新frontend mutation都携带：

```text
MutationContext {
  namespace_token
  mutation_id
}
```

`status.mutation_authority`提供当前public-safe namespace和至少14天的reconciliation window。Namespace不复用daemon instance、activation generation、enforcement epoch、database path或identity。

Backend在同一`BEGIN IMMEDIATE` transaction内重读exact policy facts、应用state change、推进durable policy revision并插入typed receipt。Exact `(namespace_token, mutation_id)` replay先于current lifecycle policy：同一canonical request返回stored receipt且不重复side effect/revision；不同request返回conflict。Receipt outcome是`applied | no_change | rejected`，`committed`表示outcome已durable，不表示retry后的cleanup已成功。

`mutation_status`返回：

- `committed { receipt }`：stored outcome是authority；
- `not_found { context }`：只有supplied namespace仍是current authority时，才证明本namespace下没有commit；
- `authority_lost { context }`：receipt缺失且namespace已失效；不能推断原mutation未发生。

Receipt pruning与namespace rotation在同一transaction；14天窗口内不得为count cap提前删除。容量不足时拒绝新mutation。Old namespace + missing receipt的mutation request必须被拒绝。

App在connect/send前把namespace、ID、canonical mutation、semantic lock、created time和visual dismissal写入owner-private crash-durable journal。Journal failure发送零请求；App restart只查`mutation_status`，不重发原mutation。Pre-v0.1一次只允许一个unresolved ordinary mutation；dismiss banner不清journal或lock，read-only commands继续工作。

## Shared status, roster, events, and diagnostics

V3/v4 `PublicStatus`只给frontend：daemon version、health、readiness、effective mode、activity、pause、last completed scan、incident counts、recent reclaim、event/storage health、bounded attention/protection、global capabilities和mutation authority。它不发送daemon PID、instance ID、activation/armed generation、enforcement epoch、requested mode、database schema、binary/path identity、service transaction state或raw last error。

Capability projection和mutation authorization调用同一Rust public-action policy。UI不从lifecycle、incident stage、reason string或score推导授权。

`incidents`继续返回transitional `ObservationRoster`，供compatibility、detail和diagnostic testing使用。Never-observed没有fake timestamp/token；replacement cycle完成前保留上一份roster，失败后标`stale_after_failure`。它不是browser screen的composition surface、work queue或manual-kill authority。

每个retained public history event有opaque、stable、unique `event_token`。Swift row identity与notification dedupe使用token，不使用internal event/attempt IDs。Cleanup同时保留process、artifact、overall outcome；`state: FAILED`与`overall_outcome: cleared_with_residue`可以共存。Diagnostics payload有required integer `document_schema_version`；v4 App要求value 4。Export是semantic-lossless JSON，不承诺原始byte layout或key order。

## Canonical fixtures and verification

[`v4/`](v4/)包含confirmed、protected和settled atomic browser-overview envelopes。[`v3/`](v3/)继续包含status、roster、history/detail/outcome/diagnostics、mutation receipts/status以及App-local historical scenarios。Bundled App包含v4与v3，不包含v2；fixture lookup必须指定schema version，不能把v3文件误当active browser payload。

```bash
cargo test -p unlinger-protocol
cargo test -p unlinger-daemon --test ipc_roundtrip
cd apps/UnlingerApp && swift test
scripts/bundle.sh
scripts/pre-v0.1-smoke.sh
```
