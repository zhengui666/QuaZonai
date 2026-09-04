# QuaZonai CLI、Codex Harness 与 Mission Tool 合同

> 上位事实源：[DESIGN.md](DESIGN.md)。本文件只描述已发布的 CLI 语法和
> Codex/MCP 边界；它不创建产品事实，也不把规划当成实现证据。

## 1. 边界

QuaZonai 有三条彼此独立的通道：

```text
Web operator → FastAPI Core
local human / automation → quazonai CLI → loopback FastAPI Core
per-Mission Codex App Server → one mission-scoped stdio MCP server → Core
```

CLI 是本地薄客户端。它不访问 PostgreSQL、数据卷、Program worktree、Codex
profile 或下游运行时。Built-in Codex 也不 shell-out CLI；它只能通过受合同限制的
Mission MCP 操作研究事实。

QuaZonai 只拥有研究、Alpha 信号、目标权重、Package、Approval 与 Handoff 事实。
它不拥有 broker 凭据、订单、成交、仓位、账户、NAV 或下游运行控制。Candidate
Package 只包含 `TargetPortfolioFrame` 和受控证据，绝不包含执行代码、订单或运行时
控制指令。

## 2. 本地 CLI

`quazonai` 默认连接 `http://127.0.0.1:8000`，只接受 `127.0.0.1`、`localhost`
或 `::1` 的 HTTP(S) endpoint。启用 Operator Authentication 时，它从
`QUAZONAI_API_TOKEN` 读取 machine Bearer credential；它不会读取浏览器 TOTP
setup secret、session 或 trusted-browser cookie。

```bash
# Required when QUAZONAI_AUTH_ENABLED=true:
export QUAZONAI_API_TOKEN='<machine token configured on the API>'
quazonai readiness
```

所有 CLI mutation 自动发送新的 `Idempotency-Key`。Idea 答复、Idea Start 与
Program lifecycle mutation 明确要求 `--expected-revision`；先重新读取资源，再把
返回的 revision 用于写入。成功时 CLI 向 stdout 打印 Core API 返回的 JSON 值；
下游注册响应中的一次性 `service_token` 会被脱敏，失败时向 stderr 打印错误。

## 3. 已实现命令

### Idea Draft → Charter → Program

这是唯一的新 Program 创建路径。没有 preview 或直接创建 Program 的 CLI 路径。

```bash
quazonai idea create --text "<RESEARCH_IDEA>"
quazonai idea show <DRAFT_ID>
quazonai idea answer <DRAFT_ID> \
  --expected-revision <REVISION> \
  --answer market_scope="<SCOPE>" \
  --answer horizon="<HORIZON>" \
  --answer data_scope="<DATA_SCOPE>"
quazonai idea start <DRAFT_ID> --expected-revision <REVISION> [--title "<TITLE>"]
```

`idea answer` accepts one or more `KEY=VALUE` pairs. The server owns the
questions, validates their keys, and freezes the immutable Charter only after
all required answers exist. `idea start` creates the first bounded Research
Cycle and its fixed Mission DAG.

### Research and Mission inspection

```bash
quazonai research list
quazonai research show <PROGRAM_ID>
quazonai research cycles <PROGRAM_ID>
quazonai research graph <PROGRAM_ID>

quazonai mission show <MISSION_ID>
quazonai mission turns <MISSION_ID>
quazonai mission artifacts <MISSION_ID>
```

The graph and cycle resources replace the retired Program activity and Mission
list paths. A Mission failure is not automatically an Alpha failure; preserve
the returned category and evidence state.

### Research lifecycle

```bash
quazonai research pause <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research resume <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research archive <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
quazonai research wake <PROGRAM_ID> --expected-revision <REVISION> [--reason "<TEXT>"]
```

`research wake` 是显式生命周期 mutation；自动由反馈/degradation 生成 Wake/Replan
尚未验收。无论何种 Wake，这些命令都不会停止或改变独立下游 runtime。

### Fresh-install configuration

配置写入只走 `/api/v1/*` 的 canonical resource endpoint。每个 `--json` 必须是完整 JSON object，原样
交由 Core 做 schema、public-secret 与不可变版本校验：

```text
quazonai universe create --json '<UNIVERSE_CREATE_JSON>'
quazonai universe version <UNIVERSE_VERSION_ID> --json '<UNIVERSE_VERSION_JSON>'
quazonai data-source create --json '<DATA_SOURCE_JSON>'
quazonai data-source preflight <DATA_SOURCE_ID>
quazonai dataset materialize --json '<DATASET_MATERIALIZATION_JSON>'
quazonai dataset status <OPERATION_ID>
quazonai evaluation-dataset-selection create --json '<EVALUATION_DATASET_SELECTION_JSON>'
quazonai evaluation-design-version create --json '<EVALUATION_DESIGN_VERSION_JSON>'
quazonai promotion-policy-version create --json '<PROMOTION_POLICY_VERSION_JSON>'
quazonai mandate create --json '<MANDATE_CREATE_JSON>'
quazonai mandate version <MANDATE_ID> --json '<MANDATE_VERSION_JSON>'
quazonai downstream register --json '<DOWNSTREAM_JSON>'
```

`data-source preflight` 只提交 `{}`，只消费已登记 Source 的受治理事实并返回 durable
operation；用 `dataset status` 读取其状态。它不接受 URL、endpoint、plugin path 或 credential。
`dataset materialize` 返回 durable operation；它和新 Dataset Revision 初始都是
`PENDING`/non-promotable，不能把请求成功当作可研究、Paper 或 Live 的证据。`downstream
register` 的 `public_config` 不得放 credential，CLI 绝不打印返回的一次性 service token。

Trusted Alpha configuration 的三个 `create` 命令只转发完整 JSON object，由 Core 校验
Dataset Selection、统计设计和 Promotion Policy。CLI 不选择“latest” Dataset、不补阈值、gate、
downstream 或 mode，也不创建快捷 activation。它们是低频 Administration 写入，必须有明确授权。

### Existing read and human-only surfaces

```bash
quazonai alpha list
quazonai alpha show <QUALIFICATION_ID>
quazonai portfolio mandates
quazonai portfolio programs
quazonai portfolio candidate <CANDIDATE_ID>
quazonai approval list
quazonai approval show <APPROVAL_ID>
quazonai handoff list
quazonai data-source list
quazonai evaluation-dataset-selection list
quazonai evaluation-design-version list
quazonai promotion-policy-version list
quazonai datasets
quazonai universes
quazonai downstreams
```

`approval approve` and `approval reject` exist only for a human operator to
run after review. No Codex or other Agent profile may execute either command.
`handoff revoke` 和所有 configuration 写入都需要明确用户请求与最终 readback。

## 4. Mission runtime and MCP

Each Mission gets a finite App Server child, exclusive temporary worktree,
durable Codex Thread, and durable turn/activity records. After a worker crash,
the worker resumes the same Thread when possible; otherwise it records an
interruption before a new attempt. A Program is not an unbounded chat Thread.

The App Server uses stable stdio and one `quazonai_mission` MCP server. The
server filters tools using the immutable Mission Contract, revalidates state,
scope, revision and idempotency on every mutation, and returns only structured
facts. It never exposes Sealed raw data, database credentials, provider or
downstream secrets, Approval/Handoff mutation, admin mutation, or any
execution capability.

Agent output is not domain fact by itself. Domain validation accepts a typed
artifact or result before a Mission can advance. Persist observable tool calls,
file changes, tests and structured summaries; do not persist hidden reasoning.
Use UUIDs, explicit versions and revisions for business identity. Do not add a
SHA, hash, checksum, digest or fingerprint gate.

## 5. Contract checks

The release checks must prove the narrow boundaries rather than inferred
intent:

- CLI documentation and `--help` describe the same parser tree;
- Draft answers and lifecycle writes carry idempotency and expected revision;
- an interrupted Mission resumes its durable Thread without reusing a deleted
  worktree or silently starting execution work;
- MCP lists only contract-permitted tools and rejects direct hard-denied calls;
- Sealed data, secrets, database access and downstream control remain outside
  the Mission process;
- a Portfolio with fewer than two eligible Alpha qualifications is
  `INFEASIBLE`, never a single-Alpha 100% fallback.

Fresh-install E2E 只有在 Web 或 CLI 配置产生真实持久事实并通过独立验证后才能成立；
当前 CLI 命令、test seed 和文档本身都不是该证据。Package-before-Approval、Auto Live
与自动 Wake/Replan 仍需各自的 E2E 验收。
