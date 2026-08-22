# QuaZonai CLI、Codex Harness 与 Mission Tool 技术设计

> 上位事实源：[`DESIGN.md`](DESIGN.md)。本文件只展开实现合同，不创造新的产品事实。

## 1. 结论

QuaZonai 有三条明确操作通道：

```text
Human Web
  → FastAPI Core

Local human / automation
  → quazonai CLI
  → loopback FastAPI Core

Built-in Codex Runtime
  → codex app-server (stdio)
  → mission-scoped stdio MCP Tool Server
  → QuaZonai Domain API / services
```

Built-in Codex **不通过 CLI 作为 RPC**。CLI 是人类与自动化薄客户端；Mission Tool Server 才是 Codex 的结构化研究接口。

## 2. CLI 原则

- 可执行名：`quazonai`；
- 默认 API：`http://127.0.0.1:8000`；
- CLI 不访问 PostgreSQL、Program repo、Dataset volume、CODEX_HOME 或 plugin runtime；
- 所有 mutation 发送 `Idempotency-Key`；
- 更新类操作发送 `expected_revision/state/version`；
- Secret 只通过安全 stdin/prompt 输入，不打印；
- `--json` 输出稳定机器可读 envelope；
- CLI 不复制领域状态机。

统一输出：

```json
{
  "ok": true,
  "data": {},
  "request_id": "..."
}
```

错误：

```json
{
  "ok": false,
  "error": {
    "code": "APPROVAL_STALE",
    "message": "...",
    "details": {}
  },
  "request_id": "..."
}
```

## 3. CLI 命令面

### 3.1 System / readiness

```bash
quazonai status
quazonai readiness
quazonai events watch [--after EVENT_ID]
```

`status` 返回：API、DB、worker、agent-worker、evaluator、storage、Codex 摘要。

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

`idea preview` 只预览 Charter/overlap，不创建正式 Program。

`research start` 只有在 Charter 完整时才冻结并创建 Program。

### 3.3 Alpha

```bash
quazonai alpha list [--role PRIMARY_ALPHA] [--state ACTIVE] [--universe ID]
quazonai alpha show QUALIFICATION_ID
quazonai alpha lineage QUALIFICATION_ID
```

CLI 不提供 `activate-alpha` / `restore-alpha` 人工命令。

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

不提供人工 `set-weight`、`add-alpha` 或 `patch-candidate`。

### 3.5 Approval

```bash
quazonai approval list [--state PENDING]
quazonai approval show APPROVAL_ID
quazonai approval approve APPROVAL_ID --downstream DOWNSTREAM_ID
quazonai approval reject APPROVAL_ID --reason REASON_CODE [--note TEXT]
```

`approve` 前 CLI 必须重新读取当前 snapshot；服务端仍做最终 freshness/precondition 校验。

### 3.6 Handoff / Feedback

```bash
quazonai handoff list
quazonai handoff show HANDOFF_ID
quazonai handoff revoke HANDOFF_ID --reason REASON_CODE [--note TEXT]
quazonai feedback show HANDOFF_ID
```

CLI 不提供 claimed downstream 的 stop/undeploy/cancel-live。

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

## 4. Codex App Server 集成

### 4.1 固定版本与 schema

实现时固定一个经过验收的 Codex CLI/App Server 版本。CI 执行：

```bash
codex app-server generate-json-schema --out build/codex-schema
```

生成物用于协议合同测试，不作为产品业务事实。

生产主传输：

```bash
codex app-server --listen stdio://
```

不得把 experimental WebSocket 作为核心依赖。

### 4.2 Process lifecycle

`agent-worker` 对每个 Mission：

1. claim Mission lease；
2. 创建/恢复 Mission worktree；
3. 启动独立 `codex app-server` child；
4. `initialize` + `initialized`；
5. 新 Mission 调用 `thread/start`；已有 durable thread 调用 `thread/resume`；
6. `turn/start` 提交 Mission prompt；
7. 流式消费 item/turn notifications；
8. 投影安全 activity；
9. 等待 `turn/completed`；
10. 验证 outputs；
11. 让 Domain Validator 接受或拒绝结果；
12. 终止 child，收口 worktree。

如果 App Server 进程异常退出：

- Mission attempt 标记 `INTERRUPTED`；
- 未发生业务 side effect 的可安全重试；
- 已通过 MCP 创建 durable operation 的，先按 idempotency key 查询结果；
- 重新启动 child 后 `thread/resume`；
- 不重复提交已确认完成的 Domain mutation。

### 4.3 `thread/start` 基线参数

概念配置：

```json
{
  "model": "<AgentProfileVersion.model>",
  "cwd": "/worktrees/<mission>",
  "runtimeWorkspaceRoots": ["/worktrees/<mission>"],
  "developerInstructions": "<mission role + contract instructions>",
  "approvalPolicy": "never"
}
```

V1 不依赖 experimental `dynamicTools`、project assignment、environments 或 selected capability roots。若未来使用，先更新 `DESIGN.md` 并增加版本/回退测试。

### 4.4 Turn input

Mission prompt 必须是结构化摘要，不把数据库 dump 直接塞进 prompt：

```text
Mission ID
Role
Objective
Research Charter summary
Allowed scope
Input artifact references
Required outputs
Success criteria
Failure conditions
Available MCP tools
```

## 5. Codex sandbox

默认：

```text
sandbox: workspace-write
network: disabled
approvalPolicy: never
cwd: mission worktree
runtime roots: mission worktree only
```

Agent 不能通过 interactive approval 请求人类开放网络/系统目录。

Codex shell 可做：

- 读写 Mission worktree；
- 运行本地 Python/test；
- 编译 Mission 产物；
- 调用已连接的 stdio MCP tools。

不能直接：

- curl/wget canonical data；
- 读 QZ DB；
- 读 Secret；
- 读 Sealed raw data；
- 操作 Git branch/commit/worktree；
- 调用下游 runtime。

## 6. AgentProfileVersion

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

默认 profiles：

| Role | 主要输出 |
|---|---|
| `RESEARCH_DIRECTOR` | Mission Graph / replan proposal |
| `DATA_RESEARCHER` | Data Requirement / quality analysis / feature input |
| `ALPHA_RESEARCHER` | Feature/Alpha/Calibration candidate |
| `VALIDATOR` | robustness report / promotion recommendation |
| `PORTFOLIO_ARCHITECT` | portfolio candidate proposal |
| `REVIEWER` | contract/completeness review，不能 approve |
| `DEGRADATION_ANALYST` | degradation diagnosis / new hypothesis |

角色不等于业务权限；server-side Mission Contract 才是最终 capability authority。

## 7. Mission-scoped stdio MCP

### 7.1 Why stdio MCP

选择稳定标准接口，避免把 Codex CLI shell parsing 或 experimental `dynamicTools` 变成核心 RPC。

App Server 启动的 Mission 会连接一个 QZ-owned stdio MCP server。该 server 不访问用户聊天历史，只根据 `mission_id` 从 Core 读取 Mission Contract。

### 7.2 Tool envelope

每个 mutation 输入：

```json
{
  "mission_id": "uuid",
  "idempotency_key": "uuid",
  "expected_revision": 12,
  "payload": {}
}
```

Tool 返回：

```json
{
  "operation_id": "uuid",
  "state": "QUEUED|RUNNING|SUCCEEDED|FAILED",
  "resource_refs": [],
  "summary": {}
}
```

### 7.3 Read tools

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

### 7.4 Mutation tools

按 Mission capability 动态过滤：

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

### 7.5 Permanent hard deny

永不暴露：

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

即使 Codex 猜到 tool name，server 也必须 hard deny 且无副作用。

## 8. Tool validation

每个调用顺序：

```text
parse schema
→ resolve mission
→ mission RUNNING?
→ capability allowed?
→ resource inside scope?
→ expected revision/state valid?
→ idempotency check
→ domain validation
→ durable operation/event
→ result
```

MCP Tool annotations/description 不替代 server-side 校验。

## 9. Large artifacts

大型 Arrow/Parquet/wheel 不放进 MCP JSON。

Codex 写 Mission worktree artifact 后：

1. `artifact.register` 提交相对路径、kind、media type、size；
2. Tool Server 校验路径必须位于 Mission worktree；
3. Core 将文件移动/复制到正式 Artifact store；
4. 生成 artifact UUID；
5. 后续业务对象只引用 artifact ID。

不计算 QZ 应用级 content hash。

## 10. App Server event mapping

至少处理：

```text
thread/started
turn/started
turn/completed
item/started
item/completed
item/* delta needed for UI
turn/diff/updated
turn/plan/updated
thread/tokenUsage/updated
```

QZ projection：

| App Server | QZ activity |
|---|---|
| Thread start/resume | `AGENT_SESSION_STARTED` |
| Turn started | `MISSION_TURN_STARTED` |
| command item | `COMMAND_ACTIVITY` |
| file change | `FILE_CHANGE_ACTIVITY` |
| MCP call | `TOOL_ACTIVITY` |
| plan update | `PLAN_ACTIVITY` |
| turn diff | `WORKSPACE_DIFF_UPDATED` |
| turn complete | `MISSION_TURN_COMPLETED` |
| runtime failure | `AGENT_RUNTIME_ERROR` |

Token usage只做容量/成本观测，不作为 Research Quality 或停止预算。

## 11. Reasoning handling

App Server Item 可能包含 reasoning 类型。QZ：

- 不把 hidden reasoning content 持久化为产品事实；
- 不在普通 UI 展示 chain-of-thought；
- 可以记录 `item_type=reasoning`, start/end time、是否完成等无内容 metadata；
- Research Summary 必须来自结构化 Mission Result / Artifact / Tool evidence，而不是从隐藏推理抽取。

## 12. Mission result contract

Mission 完成前必须调用：

```text
mission.report_result
```

概念结构：

```json
{
  "status": "SUCCEEDED|NO_PROGRESS|BLOCKED|FAILED",
  "summary": "...",
  "output_artifact_ids": [],
  "created_resource_ids": [],
  "new_hypotheses": [],
  "blocking_requirements": [],
  "recommended_next_mission_types": []
}
```

Codex 推荐的 next mission 不自动创建；Orchestrator 根据 Domain Policy 决定。

## 13. Workspace Manager

Program repo 存放的是该研究的可执行研究代码/配置/说明，不是 QZ Core source。

QZ 操作：

```text
create bare repo
create branch
create temp worktree
lease branch
launch Mission
validate changes
commit accepted revision
increment workspace_revision_no
remove worktree
release lease
```

Codex 不管理 Git 元数据。

Mission failure：

- 业务 artifacts/events 保留；
- 未接受 workspace changes 可以丢弃；
- Branch 不因失败被删除；
- retry 使用新 Mission attempt/worktree。

## 14. Sealed Evaluation 与 Agent

Codex Tool Surface 永远没有 raw Sealed data。

Promotion 流程：

```text
Candidate request promotion
→ Core assigns Sealed Episode
→ evaluator executes independently
→ full result private
→ deterministic disclosure mapper
→ Level 1 classification to Codex lineage
→ Episode consumed when disclosed
```

Agent 不能选择具体 Sealed Episode。

## 15. Optional external Skill

`skills/quazonai/SKILL.md` 用于开发者手工让外部 Codex/Agent 理解 QuaZonai，不是 built-in Runtime 的必需组件。

Skill：

- 先读 `DESIGN.md` / 当前 API manifest；
- 优先 read；
- 不猜 tool；
- 不处理 Secret；
- 不批准 Candidate；
- 不控制 downstream；
- 遇到 human-only 节点生成 handoff 提示。

外部远程 MCP OAuth Gateway 不属于 V1 Core；如未来需要公网远程 Agent，再单独设计，不复活旧 SSH/JSONL/通用 proxy 方案。

## 16. Contract tests

必须真实验证：

- 固定 Codex version `initialize` handshake；
- stdio message framing；
- thread/start/resume；
- turn lifecycle；
- process crash + resume；
- app-server overload/retry 分类；
- MCP tools/list 与 schemas；
- capability hard deny；
- idempotency/precondition；
- artifact path escape 拒绝；
- Mission network disabled；
- Sealed root 不可达；
- event projection reconnect；
- reasoning content 不进入产品持久化；
- duplicate `mission.report_result` 幂等。

## 17. CLI completion criteria

- 所有 CLI mutation 与 Web 使用相同 Core API/Domain behavior；
- CLI 不复制业务状态机；
- built-in Codex 不 shell-out 到 CLI 做 RPC；
- Agent tools 不出现 human-only mutation；
- 所有资源引用使用业务 UUID/version/revision；
- 不引入应用级 SHA/checksum/digest/fingerprint Gate；
- 文档中的命令必须与实现 `--help`/OpenAPI contract tests 对齐。