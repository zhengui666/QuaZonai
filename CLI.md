# QuaZonai CLI、Codex Harness 与 Mission Tool 技术设计

> 上位事实源：[`DESIGN.md`](DESIGN.md)。本文件只展开实现合同，不创造新的产品事实。

## 1. 结论

QuaZonai 有三条操作通道：

```text
Human Web
  → direct access, or TOTP / trusted-browser credential when auth is enabled
  → FastAPI Core

Local human / automation
  → quazonai CLI
  → QUAZONAI_API_TOKEN Bearer credential when auth is enabled
  → loopback FastAPI Core

Built-in Codex Runtime
  → per-Mission codex app-server (stdio)
  → one mission-scoped stdio MCP server
  → QuaZonai Domain/API services
```

Built-in Codex **不通过 CLI 作为 RPC**。CLI 是人类与自动化薄客户端；Mission Tool Server 才是 Codex 的结构化研究接口。Web Operator credential、CLI machine credential 和 downstream service credential 是三个独立身份边界，不能互相替代。

## 2. CLI 原则

- 可执行名：`quazonai`；
- 默认 API：`http://127.0.0.1:8000`；
- CLI 不直接访问 PostgreSQL、Program repo、Dataset volume、CODEX_HOME 或 plugin runtime；
- `QUAZONAI_AUTH_ENABLED=true` 时，CLI 从环境读取 `QUAZONAI_API_TOKEN` 并以 `Authorization: Bearer` 调用 Operator API；认证关闭时该 token 不是必需项；
- CLI 不读取或存储浏览器 TOTP setup secret、session/trusted-browser cookie，也不使用已废弃的 `QUAZONAI_AUTH_USERNAME` / `QUAZONAI_AUTH_PASSWORD`；
- machine token 不授予 downstream-owned Handoff claim/accept/reject/package/feedback 权限；这些端点继续使用对应 Downstream System 的 service token；
- 所有 mutation 发送 `Idempotency-Key`；
- 更新类操作发送 `expected_revision/state/version`；
- Secret 只通过安全 stdin/prompt/环境注入，不打印；
- `--json` 输出稳定机器可读 envelope；
- CLI 不复制领域状态机。

典型本地启动：

```bash
# Required when QUAZONAI_AUTH_ENABLED=true:
export QUAZONAI_API_TOKEN='<same machine token configured on the API>'
quazonai readiness
```

`QUAZONAI_API_TOKEN` 是本地 automation credential，不是网页登录 token。它必须是 32–4096 字符 RFC 6750 `b64token`；CLI 不转义空白、CR/LF、控制字符、非 ASCII 或其他非法 header 字符。轮换该值后需要同步更新调用 CLI 的 shell/secret manager；不需要重新配置 Google Authenticator。

统一成功输出：

```json
{
  "ok": true,
  "data": {},
  "request_id": "uuid"
}
```

错误：

```json
{
  "ok": false,
  "error": {
    "code": "APPROVAL_STALE",
    "message": "The approval snapshot is no longer current.",
    "details": {}
  },
  "request_id": "uuid"
}
```

## 3. CLI 命令面

### 3.1 System / readiness

```bash
quazonai status
quazonai readiness
quazonai events watch [--after EVENT_ID]
```

`status` 返回 API、DB、worker、agent-worker、evaluator、storage、Codex 摘要。

### 3.2 Idea / Research

```bash
quazonai idea preview --text TEXT
quazonai research start --idea TEXT [--answer KEY=VALUE ...]
quazonai research list [--state ACTIVE]
quazonai research show PROGRAM_ID
quazonai research activity PROGRAM_ID
quazonai research missions PROGRAM_ID
quazonai research pause PROGRAM_ID --reason TEXT
quazonai research resume PROGRAM_ID
quazonai research archive PROGRAM_ID --reason TEXT
quazonai research restore PROGRAM_ID
```

`idea preview` 只预览 Charter/overlap；不创建正式 Program。

`research start` 只有在 Charter 完整时才冻结 Charter 并创建 Program。

### 3.3 Alpha

```bash
quazonai alpha list [--role PRIMARY_ALPHA] [--state ACTIVE] [--universe ID]
quazonai alpha show QUALIFICATION_ID
quazonai alpha lineage QUALIFICATION_ID
```

CLI 不提供人工 `activate-alpha` / `restore-alpha`。

### 3.4 Portfolio

```bash
quazonai mandate list
quazonai mandate show MANDATE_ID
quazonai mandate enable MANDATE_ID
quazonai mandate disable MANDATE_ID
quazonai portfolio list [--mandate ID]
quazonai portfolio show PORTFOLIO_PROGRAM_ID
quazonai candidate show CANDIDATE_ID
```

不提供 `set-weight`、`add-alpha`、`patch-candidate`。

### 3.5 Approval

```bash
quazonai approval list [--state PENDING]
quazonai approval show APPROVAL_ID
quazonai approval approve APPROVAL_ID --downstream DOWNSTREAM_ID
quazonai approval reject APPROVAL_ID --reason REASON_CODE [--note TEXT]
```

`approve` 前客户端重新读取 Snapshot；服务端仍做最终 freshness/precondition 校验。

### 3.6 Handoff / Feedback

```bash
quazonai handoff list
quazonai handoff show HANDOFF_ID
quazonai handoff revoke HANDOFF_ID --reason REASON_CODE [--note TEXT]
quazonai feedback show HANDOFF_ID
```

不存在 claimed downstream 的 `stop` / `undeploy` / `cancel-live` 命令。

### 3.7 Administration

```bash
quazonai codex status
quazonai codex preflight

quazonai universe list
quazonai universe show ID

quazonai data-source list
quazonai data-source create --file CONFIG.json
quazonai data-source test ID
quazonai dataset list
quazonai dataset show ID

quazonai capital-context list
quazonai capital-context create --currency USD --capital 100000 --valid-until ISO8601

quazonai downstream list
quazonai downstream create --file CONFIG.json
quazonai downstream preflight ID

quazonai plugin list
quazonai plugin install PRIMARY.whl [DEPENDENCY.whl ...]
quazonai plugin activate RELEASE_ID
quazonai plugin deactivate RELEASE_ID
quazonai plugin remove RELEASE_ID [--force]
```

Plugin command 只管理 DATA/RESEARCH/HANDOFF capability。

## Remote Nautilus Quant Runtime Contract

Core API 的远程量化运行接口为：

```text
POST /api/v1/quant-runtime/catalogs/ingest
GET  /api/v1/quant-runtime/catalogs
POST /api/v1/quant-runtime/catalogs/{catalog_id}/validate
POST /api/v1/quant-runtime/archive-manifests/inspect
GET  /api/v1/quant-runtime/archive-manifests
GET  /api/v1/quant-runtime/archive-manifests/{manifest_id}/shards
POST /api/v1/quant-runtime/archive-manifests/{manifest_id}/materialize
GET  /api/v1/research-programs/{program_id}/quant-runs
GET  /api/v1/research-programs/{program_id}/search-ledger
```

`ALPHA_DISCOVERY` Mission 在自己的 worktree 中写入 `EXPERIMENTS.json`。受信 finite-worker 只接受 `catalog://` 引用和严格的 `StrategyArtifact`/`ExperimentSpec` 合同，然后调用 pinned NautilusTrader `1.231.0` Remote Research Runtime；每次成功或失败都写入 `QuantRuntimeRun` 与 `SearchLedgerEntry`。Sealed Evaluation 使用独立 endpoint/token/catalog，Agent 只能看到受控结果，不能读取 Sealed raw evidence。

`StrategyArtifact.source_files` 必须提供可导入且可执行的完整 strategy/config 实现，并与 `strategy_path`、`config_path` 及配置字段一致；不得用 placeholder、TODO 或“由 parent runtime 实现”替代实验逻辑。Parent 只执行已提交 artifact，不会代补缺失的因子实现；不可运行的 artifact 会作为失败尝试记录。

批准后的 Candidate Bundle 固定 Strategy wheel、config、runtime pin、data requirements、TargetPortfolioFrame conformance fixture、aggregate evidence 和 lineage。Bundle 不含 broker/provider/runtime secret，也不输出订单命令；下游负责 Paper/Live runtime 的启动、停止、撤单、平仓和恢复。

---

# Part I — Codex App Server Adapter

## 4. Version pinning and transport

实现固定一个经过验收的 Codex CLI/App Server 版本。CI 由同一二进制生成协议 schema：

```bash
codex app-server generate-json-schema --out build/codex-schema
codex app-server generate-ts --out build/codex-ts
```

这些 schema 是 adapter contract-test 输入，不是业务事实。

V1 主传输：

```bash
codex app-server --listen stdio://
```

WebSocket、Project API、Environment API、`dynamicTools`、`runtimeWorkspaceRoots`、`selectedCapabilityRoots` 等 experimental surface 不作为 V1 必需依赖。

## 5. QuaZonai-owned Codex runtime profile

Agent Worker 不复用操作者日常 Codex CLI 的任意全局配置。QuaZonai 使用独立、受控的 Codex runtime profile：

```text
/var/lib/quazonai/codex-runtime/
```

它只承担：

- QuaZonai 专用 Codex authentication；
- Codex thread/session persistence；
- QZ 允许的最小运行配置。

不得把个人全局 MCP servers、marketplaces、plugins 或任意 Skills 自动带入自治 Research Mission。

管理员通过 QuaZonai Administration 完成该专用 runtime profile 的 Codex 登录；Secret/token 不进入 QZ 数据库、Mission prompt、Mission worktree 或普通 UI。

## 6. Per-Mission App Server process

每个 `RUNNING` Mission 使用一个独立 App Server child：

1. Agent Worker claim Mission lease；
2. 创建/恢复 Mission worktree；
3. 生成该 Mission 的 runtime launch spec；
4. 启动 `codex app-server` child；
5. `initialize` + `initialized`；
6. 新 Mission `thread/start`；已有 durable thread `thread/resume`；
7. `turn/start` 提交 Mission input；
8. 流式消费 notifications；
9. 投影允许的 Agent Activity；
10. 等待 `turn/completed`；
11. 验证 `mission.report_result` 和 artifacts；
12. Domain Validator 接受/拒绝输出；
13. 终止 App Server child；
14. 收口 worktree / Branch lease。

一个 Research Program **不**使用无限长 Codex Thread。一个 Mission 对应一个 durable Codex Thread；同 Mission retry/resume 才复用该 Thread。

## 7. Mission-specific Codex config override

Codex CLI 支持全局 `-c key=value` / `--config key=value`，其中 key 可为 dotted path、value 按 TOML 解析。

Agent Worker 必须在 App Server 启动时把有效 MCP 配置收窄为 **唯一的 QuaZonai Mission server**，不能继承任意用户 MCP：

```text
mcp_servers = {
  quazonai_mission = {
    command = "quazonai-mission-mcp",
    args = ["--mission-id", "<MISSION_ID>"]
  }
}
```

具体 CLI quoting/serialization 由 pinned Codex version 的 config schema 驱动并有 integration test；实现不得依赖手写 shell 字符串拼接。

Mission MCP 所需的短期 capability credential 只通过该 MCP server 的专用 process env/IPC 注入：

- 不进入 Turn input；
- 不进入 general shell environment；
- 不进入 worktree；
- scope 只覆盖一个 `mission_id`；
- Mission 终态立即失效。

即使 capability 泄漏，Core 仍按 Mission Contract、resource scope、expected revision/state 和 idempotency 做最终授权；capability 本身不能扩大权限。

## 8. Thread/Turn baseline

V1 不依赖 experimental `runtimeWorkspaceRoots`。Workspace hard boundary 由 QZ Workspace Manager + OS/container mount policy + Codex stable `cwd` / legacy `sandbox` policy共同实现。

`thread/start` / first turn 概念输入：

```json
{
  "model": "<AgentProfileVersion.model>",
  "cwd": "/worktrees/<mission>",
  "developerInstructions": "<mission role + contract instructions>",
  "sandbox": "workspace-write",
  "approvalPolicy": "never"
}
```

精确字段归属（thread vs turn）以 pinned App Server schema 为准；adapter 必须通过 generated schema 生成/校验 payload，不在业务代码里散落协议 JSON。

`runtimeWorkspaceRoots`、`permissions` profile、Project/Environment 等 experimental 字段只允许作为未来可替换 hardening/optimization；不能成为业务正确性的前提。

系统级 Runtime Configuration 为新 Mission 提供默认 Codex thread controls：`reasoning_effort` 只接受 `minimal`、`low`、`medium`、`high`、`xhigh`，`null` 时不发送 `model_reasoning_effort`；独立 Fast mode 开启时发送 `service_tier="fast"`，关闭时不发送该 override。未来 `AgentProfileVersion` / Mission 显式值优先于该系统默认值，系统默认值又优先于 Codex/模型默认值；修改只影响之后启动的 Mission Thread，provider 不支持时不得静默降级。

## 9. Sandbox and OS isolation

V1 hard baseline：

```text
cwd: Mission worktree
Codex sandbox: workspace-write
network: restricted/disabled
approval policy: never
outer filesystem view: only Mission-required paths
```

QZ 外层 Mission isolation 必须保证 Codex command subprocess 无法读取：

- QuaZonai Core source tree；
- other Program repos/worktrees；
- Sealed Dataset roots；
- provider/downstream Secret stores；
- PostgreSQL credentials/socket；
- Docker socket；
- QuaZonai Codex auth material。

不得只依赖 developer instruction 来保护这些路径。具体 Linux/macOS enforcement 可以随平台实现，但 `codex preflight` 必须用真实 command tool 验证 forbidden paths 不可达。

Mission 默认无任意网络。Agent 不通过 interactive approval 请求用户开放网络；数据和外部能力走 QZ Tool Server。

在 Linux 容器中，`finite-worker` 使用专用 seccomp profile 允许 Codex bundled bubblewrap 创建用户、挂载和网络 namespace。该 profile 只解决 namespace bootstrap；worker 仍保持 `no-new-privileges`、`cap_drop: ALL`、只读根文件系统和 Mission 沙箱。不得改用 `privileged`、`CAP_SYS_ADMIN` 或 `--dangerously-bypass-approvals-and-sandbox`。Worker 的 `--check` 会执行真实 sandbox preflight；preflight 失败时不能领取 Mission。

Codex command 可以：

- 读写 Mission worktree；
- 运行本地 Python / tests；
- 构建 Mission artifact；
- 通过 Codex MCP client 调用 `quazonai_mission` tools。

## 10. Crash / retry / resume

App Server 进程异常退出：

- 当前 Mission attempt → `INTERRUPTED`；
- 未产生 durable domain side effect 的步骤可新 attempt 重试；
- 已发出的 MCP mutation 先按 `idempotency_key` 查询 operation receipt；
- 启动新 App Server child 后 `thread/resume` 同一 Mission Thread；
- 不重复提交已完成 mutation；
- 若 Thread store 损坏或不可 resume，创建新 Mission attempt/thread，但保留前一次 Search Ledger / Activity / durable artifacts，并由 Orchestrator决定是否继续。

App Server ingress 返回 overloaded `-32001` 时按 adapter policy 指数退避 + jitter；不能把 retryable transport failure解释为 Research failure。

---

# Part II — Agent Profiles and Mission Contract

## 11. AgentProfileVersion

字段：

```text
id
role
version_no
model_preference
reasoning_effort
developer_instructions
allowed_capabilities[]
allowed_output_kinds[]
max_turn_runtime
state
created_at
```

Profiles：

| Role | 主要输出 |
|---|---|
| `RESEARCH_DIRECTOR` | Mission Graph / replan proposal |
| `DATA_RESEARCHER` | Data Requirement / quality analysis / feature inputs |
| `ALPHA_RESEARCHER` | Feature / Alpha / Calibration candidate |
| `VALIDATOR` | robustness / completeness / promotion recommendation |
| `PORTFOLIO_ARCHITECT` | Portfolio Candidate proposal |
| `REVIEWER` | contract/evidence completeness review；不能 approve |
| `DEGRADATION_ANALYST` | degradation diagnosis / new hypothesis |

Role 只是执行 profile；最终权限来自 immutable Mission Contract + Tool Server 校验。

## 12. Mission prompt

Turn input 只提供结构化、最小上下文：

```text
Mission ID
Role
Objective
Research Charter summary
Universe/Horizon scope
Input artifact references
Allowed capabilities
Required output kinds
Success criteria
Failure conditions
Disclosure level
```

不把 PostgreSQL row dump、Secret、Sealed raw result 或其他 Program history 全量塞入 prompt。

## 13. Mission result

Mission 完成前必须调用：

```text
mission.report_result
```

结构：

```json
{
  "status": "SUCCEEDED|NO_PROGRESS|BLOCKED|FAILED",
  "summary": "verifiable result summary",
  "output_artifact_ids": [],
  "created_resource_ids": [],
  "new_hypotheses": [],
  "blocking_requirements": [],
  "recommended_next_mission_types": []
}
```

`recommended_next_mission_types` 只是建议；Orchestrator/Domain Policy 决定是否真正创建节点。

---

# Part III — Mission-scoped MCP

## 14. Why stdio MCP

V1 使用稳定的 stdio MCP，而不是：

- shell-out `quazonai` CLI 解析文本；
- App Server experimental `dynamicTools`；
- 自定义 JSONL/RPC；
- 公网 Agent Gateway。

每个 App Server 只配置一个 QZ-owned `quazonai_mission` server。

## 15. Tool envelope

Mutation 概念输入：

```json
{
  "mission_id": "uuid",
  "idempotency_key": "uuid",
  "expected_revision": 12,
  "payload": {}
}
```

结果：

```json
{
  "operation_id": "uuid",
  "state": "QUEUED|RUNNING|SUCCEEDED|FAILED",
  "resource_refs": [],
  "summary": {}
}
```

大型 Arrow/Parquet/wheel 不内嵌 JSON。

## 16. Read tools

按 Contract 暴露：

```text
mission.get_contract
evidence.read_allowed
artifact.describe
dataset.list
dataset.describe
dataset.query_sample
experiment.status
alpha.library_search
alpha.inspect
portfolio.inspect_mandate
portfolio.inspect_program
```

## 17. Mutation tools

仅在当前 Mission capability 存在时进入 `tools/list`：

```text
data.requirement_submit
experiment.submit
artifact.register
feature.submit_version
alpha.submit_model
alpha.submit_calibration
alpha.submit_qualification_candidate
portfolio.submit_candidate
mission.submit_plan
mission.report_result
```

## 18. Permanent hard deny

永不提供：

```text
approval.approve
approval.reject
handoff.publish
handoff.revoke
secret.read
secret.write
plugin.activate
plugin.remove
mandate.mutate
universe.mutate
sealed.read_raw
downstream.stop
downstream.order
```

即使模型猜到名字，服务端也必须 hard deny 且无副作用。

## 19. Tool call authorization

每次调用：

```text
validate MCP schema
→ authenticate mission capability
→ resolve Mission
→ Mission RUNNING?
→ capability allowed?
→ target resource in scope?
→ expected revision/state/version valid?
→ idempotency lookup
→ Domain validation
→ durable operation + event
→ structured result
```

Tool annotation、developer instruction 和 Codex Tool UI 都不能替代 server-side authorization。

## 20. Artifact registration

Codex 写 Mission worktree 文件后：

1. `artifact.register(relative_path, kind, media_type, size_bytes, ...)`；
2. Tool Server canonicalize path；必须仍位于 Mission worktree；
3. 拒绝 symlink/path escape 到 protected roots；
4. QZ 将文件复制/移动到正式 Artifact Store；
5. 创建 artifact UUID；
6. 下游资源只引用 artifact ID/version。

不创建应用级 content hash/checksum/fingerprint。

---

# Part IV — Codex Event Projection

## 21. App Server events

Adapter 至少消费稳定 lifecycle/event surface：

```text
thread/started
turn/started
turn/completed
item/started
item/completed
item/agentMessage/delta (UI only, optional persistence policy)
turn/diff/updated
turn/plan/updated
thread/tokenUsage/updated
```

具体 method set 由 pinned schema 锁定；不对未知 experimental event 建业务依赖。

## 22. QZ activity projection

| Codex event | QZ activity |
|---|---|
| thread start/resume | `AGENT_SESSION_STARTED` |
| turn started | `MISSION_TURN_STARTED` |
| command item | `COMMAND_ACTIVITY` |
| file change | `FILE_CHANGE_ACTIVITY` |
| MCP call | `TOOL_ACTIVITY` |
| plan update | `PLAN_ACTIVITY` |
| turn diff | `WORKSPACE_DIFF_UPDATED` |
| turn completed | `MISSION_TURN_COMPLETED` |
| process/protocol failure | `AGENT_RUNTIME_ERROR` |

Activity 不是 Research state transition。Codex 声称“candidate ready”也必须经过 artifact/domain validation。

Token usage用于容量与成本观察，不是 Research Quality，也不触发 Program 累计预算停止。

## 23. Reasoning handling

App Server 可产生 reasoning Item。QZ：

- 不持久化 hidden reasoning text 作为产品事实；
- 不在普通 UI 展示 chain-of-thought；
- 最多记录 item type、start/end/status 等无内容 metadata；
- Research Summary 来自 Mission Result、Artifact、Tool output 和 Domain Evidence。

---

# Part V — Workspace Manager

## 24. Program / Branch / Mission Git model

```text
Program bare repo
  ├─ Research Branch A
  │   ├─ Mission A1 temp worktree
  │   └─ Mission A2 temp worktree
  └─ Research Branch B
      └─ Mission B1 temp worktree
```

QZ owns：

```text
create bare repo
create branch
branch lease
create worktree
validate accepted changes
commit accepted revision
increment workspace_revision_no
remove worktree
release lease
```

Codex 只编辑普通文件；不得 branch/commit/merge/rebase/worktree 管理。

Git object IDs 不作为 Candidate/Approval/Artifact/Workspace 业务 identity。

## 25. Workspace failure

Mission 失败：

- durable QZ artifacts/events/search ledger 保留；
- 未接受 worktree changes 可丢弃；
- Branch 不删除；
- retry 创建新 Mission attempt/worktree；
- 同 Branch 同时只有一个持写 lease 的 Mission，除非未来设计显式支持 merge semantics。

---

# Part VI — Sealed Evaluation and Agent Boundary

## 26. Sealed flow

```text
Candidate requests promotion
→ Core selects/assigns eligible Sealed Episode
→ evaluator executes outside Codex runtime
→ full result stored private
→ deterministic disclosure policy
→ Level 1 classification exposed to allowed research lineage
→ disclosure creates Evidence Exposure
→ Episode becomes CONSUMED for that lineage
```

Codex：

- 不能选 Episode；
- 不能读 raw Sealed Dataset；
- 不能读 Level 0 result；
- 不能通过 MCP 反查具体 date/instrument/metric gap。

---

# Part VII — Optional External Skill

## 27. Skill role

`skills/quazonai/SKILL.md` 只用于开发者手工调用外部 Codex/Agent 时理解 QuaZonai。

它不是 built-in Runtime 的依赖，也不是权限事实源。

外部 Skill：

- 先读 `AGENTS.md` / `DESIGN.md`；
- 优先 read current state；
- 使用官方 CLI/API；
- Operator Authentication 启用时依赖运行环境提供 `QUAZONAI_API_TOKEN`，不读取/推断 Web 密码或 TOTP secret；
- 不猜资源/状态；
- 不处理 Secret；
- 不替用户批准资本 handoff；
- 不控制 downstream runtime；
- 不绕过 Evidence/Approval state。

V1 不建设旧式远程 OAuth MCP Gateway、SSH transport、JSONL 隧道或通用 HTTP proxy。若未来产品需要远程多客户端 Agent，再以独立设计扩展。

---

# Part VIII — Required Contract Tests

## 28. Codex protocol

真实 pinned binary 必测：

- `initialize` / `initialized`；
- generated schema 与 adapter payload；
- stdio framing；
- `thread/start` / `thread/resume`；
- `turn/start` / `turn/completed` / interrupt；
- overload `-32001` retry；
- process crash + resume；
- unknown/experimental fields 不影响 V1 correctness。

## 29. Mission isolation

真实 command tool 必测：

- worktree 可读写；
- other Program 不可达；
- QZ source 不可达；
- Codex auth material 不可达；
- Secret roots 不可达；
- Sealed root 不可达；
- DB credential/socket 不可达；
- Docker socket 不可达；
- arbitrary outbound network 不可用；
- MCP Tool 仍可通过批准路径工作。

任何一项失败，`RESEARCH_READY=false`。

## 30. MCP contract

真实 MCP session 必测：

- only one mission server configured；
- `tools/list` 与 Mission Contract 一致；
- hard-denied tool 不存在且 direct call 无副作用；
- mission capability 过期/跨 Mission 调用拒绝；
- expected revision/state/version；
- mutation idempotency；
- duplicate `mission.report_result` 幂等；
- artifact path traversal/symlink escape 拒绝；
- large artifact 不进入 JSON body。

## 31. Event / privacy

- event ordering/reconnect；
- command/file/MCP/result projection；
- hidden reasoning text 不进入 PostgreSQL/standard logs/UI；
- Token usage 不影响 Research state；
- App Server log Secret redaction。

## 32. CLI completion criteria

- Web/CLI mutation 使用同一 Core API/Domain logic；
- auth-enabled CLI 必须发送正确 `QUAZONAI_API_TOKEN`；缺失/错误 machine token 的 Operator API 请求失败；
- CLI 不读取/存储 browser TOTP setup secret、session/trusted-browser cookies 或已废弃的 browser username/password；
- CLI machine token 不能替代 downstream service token；
- CLI 不访问 DB/volume；
- built-in Codex 不 shell-out CLI 做业务 RPC；
- Agent Tool surface 不含 human-only mutation；
- 所有身份使用 QZ UUID/version/revision；
- 不引入应用级 SHA/checksum/digest/fingerprint Gate；
- 文档命令与实现 `--help`/OpenAPI contract test 对齐。
