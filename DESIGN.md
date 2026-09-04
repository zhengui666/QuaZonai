# QuaZonai 产品需求与技术架构设计

> 架构基线：2026-09-03（Issue #58）
> 文档地位：**QuaZonai 唯一完整的产品与架构事实源**  
> 目标：Codex Harness 驱动的单用户、自托管、持续自治量化研究与策略组合工作台  
> 当前状态：**Issue #58 迁移中；已落地的领域/合同必须与尚未验证的端到端闭环分开表述。合并与 release-ready 仍以 CI、Review 与独立复核为准。**

`OPERATIONS.md` 只展开用户运行视图；`CLI.md` 只展开 CLI、Codex Runtime 和 Agent Tool 合同；`README.md` 只做入口与当前状态摘要；代码、测试、聊天记录和临时决策文件不得静默改写本文。

---

## 0.1 Issue #58 当前事实与迁移边界（2026-09-03，优先于下文冲突表述）

本节同时区分已提交的实现事实、必须保持的边界和仍待验收的目标。下文任何把可下单
Strategy、单成员 Candidate、动态第三方插件、下游节点控制或一次 Agent 运行描述为
当前 V1 主链的文字，均为待删除的历史说明，不是兼容目标。

### 规范性产品授权：继续实现可信生产链

Issue #58 的产品授权明确要求继续实现可信的
`Alpha → Portfolio → Promotion / Auto-Live` 决定性生产链。已落地的 fail-closed
边界只能在同一 PR Checklist 标记为 `Foundation Complete`；它们不是 Issue 完成条件、
唯一 PR 的合并条件、七项核心业务能力已交付的证据，也不是拆分第二个 PR 的理由。
本授权优先于任何把 fail-closed 基础设施解释为独立完成范围的歧义；只有本节所列的
端到端生产事实、验证与独立复核全部成立后，才可宣称 Issue #58 完成。

### 已实现的基础事实

- `IdeaDraft → ClarificationQuestion/Answer → ResearchCharter` 是唯一创建 Program
  的实现路径。它持久化一轮、三项边界澄清；答案进入冻结 Charter 后才可 Start。
- Start 持久化一个 `ResearchCycle` 和六节点依赖图：`PLAN_RESEARCH → DATA_QUALITY
  → ALPHA_DISCOVERY → ROBUSTNESS → PORTFOLIO_ASSEMBLY →
  SEALED_PROMOTION_REVIEW`。Mission、AgentSession、AgentTurn、Artifact、lease
  interruption 与同 Thread resume 都有持久化合同。
- `AlphaModelV1` / `AlphaSignalFrameV1` 已限制为有限、UTC、PIT-valid 的
  `event_time`、`available_time`、`instrument_id`、`score` 与可选已校准
  `expected_return`/`uncertainty` 信号；它没有 order、broker、account 或 position
  surface。PIT/质量、honest evaluation、Feature/Alpha/Calibration/Qualification 的
  不可变持久化基础也已存在。
- Portfolio 的纯引擎只产生 target weights；默认少于两个合格 Alpha 时返回明确
  `INFEASIBLE`，不会产生单 Alpha 100% 候选。target-only Package builder 只写
  manifest 与 `TargetPortfolioFrame`，并拒绝 Secret、订单、成交、仓位和运行控制字段。
- Promotion 仍只有 fail-closed 的纯 policy/contract；Degradation 已有从 completed
  Forward Evidence 到受限 Wake/Replan Cycle 的数据库/API 闭环，但两者都不等同于完整
  Paper→Live 闭环。

QZ 只生成 Alpha 信号、目标权重、研究和交付事实。它不生成或执行订单，不保存
broker/account/position/NAV，不启动、停止、撤单、平仓或恢复任何下游节点。

### 不可变身份与无 hash gate

所有业务身份使用显式 UUID、关系和有意义的版本/revision。Candidate 是不可变 UUID
事实，**不**伪造 `candidate_revision=1`。Package 有明确 `package_revision`；Approval
绑定 `candidate_id + candidate_package_id + package_revision`。替换 Package 创建新的
Package/revision，并使旧 Approval stale。

不得为业务身份、审批、幂等、发布、Package 或数据有效性新增 SHA、hash、checksum、
digest、fingerprint 或任何等价内容寻址 Gate。Package 的 schema/reference-fixture
conformance 与显式对象引用是业务验证；底层存储字节完整性属于存储运维职责。

### 尚未被本轮实现证据证明的闭环

以下是验收目标，不得描述为已交付：从空 PostgreSQL 仅经 Web/CLI 配置后完成多角色
研究、至少两个真实 Qualification、Paper、Manual/Auto Live，以及从空库只经 Web/CLI
配置完成的全链路 Forward Evidence/Wake/Replan；Package-before-Approval 的持久化事务；
以及自动 Paper→Live 产生真实 Approval/Handoff。已验证的 bounded Degradation 写者不替代
这些 E2E 证据，测试 seed 或手工插表也不能。

任何仍存在的生成式执行 artifact、旧远程 runtime 或 execution-control 路径只可视为
删除中的遗留代码；它们不得接收新的 Mission、Alpha、Candidate、Package 或 Promotion
调用。

---

## 0. 执行摘要

QuaZonai（QZ）只拥有两个核心领域：

1. **Research Intelligence**：从自然语言 Idea 到可验证 Alpha；
2. **Portfolio Construction**：把已验证 Alpha 映射到明确 Portfolio Mandate，形成可交付 Portfolio Candidate。

QuaZonai **不拥有交易执行**。NautilusTrader、LEAN 或任何自定义执行系统均是独立下游 Consumer。QZ 不启动、停止、监控或恢复交易节点，不保存 broker credential，不提交订单，不维护订单、成交、仓位、账户或 NAV，不提供中央执行风险，不把下游状态伪装为自己的 Deployment 状态。

正常 Research Program 生命周期中，人类只有两类常规操作：

1. **提出 Research Idea**；
2. **审批系统推荐的 Paper / Live Candidate Handoff**。

首次安装、可选 Operator 登录、数据授权、Codex 登录与 Runtime Configuration、Mandate/Universe/下游配置、插件管理和故障处理属于低频 Administration，不计入正常研究旅程。

目标闭环（验收目标，不表示已交付）：

```text
Idea
  → Research Charter
  → Autonomous Research Program
  → Alpha Qualification / Alpha Library
  → Portfolio Mandate
  → Portfolio Program
  → Portfolio Candidate
  → Independent Evaluation
  → Human Approval
  → Candidate Package
  → Handoff Registry
  → Independent Downstream Runtime
  → Forward Evidence
  → Degradation Monitoring
  → Research Wake-up
```

---

# Part I — PRD

## 1. 产品定位与成功标准

### 1.1 目标用户

V1 是 **单用户、自托管、私有工作台**。不建设 tenant、organization、workspace、团队协作、RBAC、多人会签或 SaaS 计费。

用户可以懂投资问题但不必手工完成传统量化研究中的数据工程、特征实验、参数搜索、稳健性验证、Alpha 组合和候选筛选。

### 1.2 核心产品承诺

- 用户用自然语言提出 Idea；
- 系统在必要时只进行一次、最多 1–3 个高价值澄清；
- Research Charter 冻结后系统长期自治研究；
- 用户无需管理 Codex Thread、Mission、参数搜索或模型选择；
- 系统只在出现有实际决策价值的唯一推荐 Candidate 时打扰用户；
- Paper 和 Live 分开审批；Paper 批准绝不预授权 Live；
- 所有研究、候选、审批、Package 和证据均可追溯且历史不可重写；
- 下游运行完全独立，QZ 只交付标准 Candidate Package 并接收反馈。

### 1.3 非目标

V1 不建设：

- broker / exchange order client；
- TradingNode、Paper scheduler、Live supervisor、Recovery、Heartbeat、Position/NAV ledger；
- QZ 自研 OMS/EMS；
- 真实账户 credential、wallet/private key 管理；
- 通用网页爬虫或 Codex 任意网络抓取；
- 公共插件市场、自动下载未知插件、任意 Git URL 安装；
- 多租户、业务用户权限系统、协作审批；
- 模型训练平台；
- 隐藏思维链展示或持久化；
- 应用级 SHA、checksum、digest、fingerprint、内容寻址身份或以此为 Gate 的完整性流程。

Git 自身对象 ID、Python wheel `RECORD` 等工具链内部机制可以存在，但 QZ 的业务身份、审批、发布、幂等和验证不得依赖它们。

## 2. 人类旅程

### 2.1 Idea Draft

```text
自然语言 Idea
→ 创建 IdeaDraft
→ 一轮、三项 server-owned 边界澄清
→ 人类提交不可变 Answer
→ 所有 Answer 完成后 Start
→ ResearchCharter 冻结并创建 Program/Cycle/fixed DAG
```

当前实现的三项问题覆盖 market scope、horizon 和 data scope。只追问会改变 Charter
边界的问题；模型、特征、CV、优化器、参数、算力和技术实现不属于人类澄清输入。

Start 会把 Charter 绑定到已启用的 Universe Version：只有一个启用版本时可自动采用；
零个或多个版本时必须阻断或由请求显式选择。不得创建 `universe_version_ids=[]` 的新
Charter。

### 2.2 重叠关系的实现状态

`IdeaContribution` 与 `ProgramRelationship` 的持久化模型存在，但自动语义重叠检测、
复用、Branch 推荐和 Exposure 继承尚未接入 Draft Start。当前每个完成的 Draft 创建其
自身 Program；不得把尚未实现的自动重叠处理描述为用户可依赖的行为。

### 2.3 Candidate Approval

每张 Approval 只展示 **一个系统推荐的不可变 Candidate**。用户可以查看只读比较和其他候选淘汰原因，但不能在审批页选择第二名、改权重、改 Mandate 或修改约束。

审批动作：

```text
Approve
Reject(reason_code, optional_note)
```

拒绝不自动递补第二名。未来再次审批必须有新证据或实质改善。

### 2.4 低频 Program 管理

允许：

- Pause；
- Resume；
- Archive；
- 受 policy 限制的 Wake。

正式开始的 Program 不提供业务层物理删除。只有未提交 Idea Draft 可以删除。

Pause/Archive 只影响 QZ 研究，不停止任何已领取 Package 的外部 Paper/Live 系统。
`PAUSED`/`ARCHIVED` 的 Wake 保持待处理，不能绕过人工状态自动恢复。

## 3. 首页与信息架构

V1 主导航：

```text
Home
Idea Composer
Research Observatory
Alpha Library
Portfolio Lab
Approval Inbox
Handoff & Feedback
Administration
```

Home 使用 **Action Center + Research Pulse**：

1. Action Center：待审批、即将过期审批、未完成 Idea 澄清、必须人工处置的关键 Admin 事件；
2. Research Pulse：ACTIVE/COOLING/PAUSED/BLOCKED Program、最近晋级 Alpha、Portfolio readiness、近期实质证据变化；
3. 主要动作固定为 `Propose new idea` 与 `Review approvals`；
4. token、命令数、trial 数、文件修改数不作为产品进展指标。

研究透明度分三层：

- Level 1：用户友好的研究摘要；
- Level 2：Charter、Branch lineage、Mission、Evaluation、Search Ledger、Evidence Exposure；
- Level 3：文件 diff、测试、Tool 调用、命令退出状态和 Codex Item 时间线。

不展示或保存模型隐藏思维链。

## 4. Readiness

能力状态相互独立：

```text
SYSTEM_READY
RESEARCH_READY
PAPER_HANDOFF_READY
LIVE_HANDOFF_READY
```

### 4.1 RESEARCH_READY

至少要求：

- PostgreSQL 与持久卷可用；
- Codex App Server 可启动且认证有效；
- Program repo / Mission worktree 可创建；
- 至少一份 `quality_state=VALID`、`point_in_time_state=VALID` 且
  `promotability=PROMOTABLE` 的真实 Dataset Revision 可用；仅登记或
  preflight-ready 的 Data Source、PENDING Dataset Revision 都不构成研究就绪；
- Canonical Research Engine 最小 preflight 通过；
- Sealed Evaluator 与 Codex 的访问隔离成立；
- Scheduler 有可用 Mission slot。

达到后即可提出 Idea，不要求先配置 Paper/Live 下游。

### 4.2 PAPER_HANDOFF_READY / LIVE_HANDOFF_READY

Paper readiness 要求至少一个可用 Paper downstream、Package/Feedback Contract 兼容、claim/feedback preflight 成功。

Live readiness 除独立 Live downstream preflight 外，还要求完整、有效、已重新评估的 Paper Forward Evidence。

缺少下游 readiness 时，研究继续。候选进入 `PAPER_CONFIGURATION_REQUIRED` 或 `LIVE_CONFIGURATION_REQUIRED`，不创建无法行动的 Approval。

### 4.2.1 Downstream service preflight

Operator 注册 DownstreamSystem 后，其初始 `revision=1`、`preflight_state=PENDING`；注册本身不构成
Paper 或 Live readiness。仅该 downstream 持有的 per-service Bearer credential 可以调用
`POST /api/v1/downstream-systems/{id}/preflight`。请求只回显已登记的
`package_contract_version`、`feedback_contract_version` 与完整 `compatibility`，并给出一个 UTC、未来的
`valid_until`；它不接受 URL、provider/downstream secret、运行命令或任何 execution control 输入。

Core 必须严格解析已登记的非空 `feedback_contract`（required fields、observation/sample minimum、接受的
package/Arrow contract 与 disclosure policy），并将回显值与登记值逐项精确比较。成功时追加一条不可变
`PreflightReceipt`，其中绑定 DownstreamSystem 的当前 `revision`、feedback contract、compatibility 与
`valid_until`，然后才把该系统置为 `READY`。`PreflightReceipt` 是唯一的 preflight 有效性事实源；readiness
和 Approval 在各自判定时重新要求一条未过期、`READY`、且匹配当前 system revision/contracts 的 receipt。

service token rotation 必须使 DownstreamSystem `revision += 1` 并回到 `PENDING`；历史 receipt 保留但不能
为新 revision 提供 readiness。该握手只证明独立 consumer 的声明兼容性，QZ 不连接、启动、停止或控制
downstream runtime。

---

# Part II — Domain Model

## 5. 核心对象与业务身份

业务对象只使用显式 UUID、业务版本号和关系，不使用内容 hash 身份。

```text
IdeaDraft
ResearchCharter
ResearchProgram
IdeaContribution
ProgramRelationship
ResearchBranch
ResearchMission
MissionArtifact
MarketUniverseVersion
DatasetRevision
FeaturePipelineVersion
AlphaModelVersion
AlphaCalibrationVersion
AlphaQualification
EvaluationEpisode
EvidenceExposure
PortfolioMandateVersion
CapitalContextVersion
PortfolioProgram
PortfolioCandidate
ApprovalSnapshot
CandidatePackage
DownstreamSystem
DownstreamConnectionVersion
HandoffOffer
FeedbackContractVersion
ForwardEvidenceEpisode
DegradationObservation
```

## 6. Research Charter / Program / Branch / Mission

### 6.1 Research Charter

Charter 冻结字段：

```text
charter_id
original_idea_text
research_question
market_scope
universe_version_ids[]
prediction_horizon
allowed_data_domains[]
explicit_exclusions[]
material_assumptions[]
system_assumptions[]
created_at
```

冻结后不原地修改。需要突破范围时创建关联新 Program。

### 6.2 Research Program

状态：

```text
ACTIVE
COOLING
APPROVAL_PENDING
WAITING_FOR_FEEDBACK
BLOCKED
PAUSED
ARCHIVED
```

`COOLING` 由系统控制，等待新信息；`PAUSED` 是人工 override，任何 wake event 都不能自动恢复；`ARCHIVED` 退出活跃池但保留全部事实。

不设置 Program 累计 token、CPU-hour、Mission 数或 Experiment 数预算上限。仍保留物理并发、fair scheduling、重复拒绝和 finite Mission。

### 6.3 Research Branch

Branch 记录：

```text
branch_id
program_id
parent_branch_id nullable
derivation_type
hypothesis
changed_assumptions
preserved_constraints
state
created_at
```

允许 Codex 在 Charter 内派生子假设、反假设和分段研究；不能静默扩大 Universe/Data Domain。

### 6.4 Research Mission

Mission 是有限执行单元：

```text
PLANNED → READY → RUNNING → SUCCEEDED
                    ├→ AWAITING_VALIDATION → SUCCEEDED | FAILED | CANCELLED
                    ├→ FAILED
                    ├→ INTERRUPTED
                    └→ CANCELLED
```

`AWAITING_VALIDATION` 只用于 Codex child 已退出、但同一 Mission 的 Core-owned durable
validation/evaluator job 尚未形成可信结果的短暂状态；它不会重新派发 Codex child，也不会
解锁 DAG。只有该冻结验证 job 可以把它推进为终态。

MissionContract 至少包含：

```text
mission_id
mission_type
role_profile_version_id
objective
input_artifact_ids[]
allowed_capabilities[]
expected_output_kinds[]
success_criteria
failure_conditions
evidence_scope
disclosure_level
branch_id
workspace_revision_no
```

除 `ALPHA_PROPOSAL` 外，Agent artifact 只能提交固定的公开 V1 envelope：`{kind: {
summary, items, facts}}`。`summary` 和 `items` 必须非空且有界，`facts` 必须与 artifact kind 的
固定字段集合精确一致，并通过 Core 的类型/UUID/非空值校验；只有该 kind-specific validator
可以把 artifact 标为 `VALIDATED`。固定 facts 集合为：`RESEARCH_PLAN(objective,hypotheses)`、
`DATA_REQUIREMENT(dataset_scope,requirements)`、`DATA_QUALITY_REPORT(dataset_revision_id,
quality_state,pit_state)`、`FEATURE_PROPOSAL(family,input_contract)`、
`CALIBRATION_PROPOSAL(model_version_id,method)`、`ROBUSTNESS_REPORT(checks,outcome)`、
`PROMOTION_REVIEW(candidate_id,decision)`、`PORTFOLIO_PROPOSAL(candidate_id,weights)`、
`PAPER_EVIDENCE_REVIEW/LIVE_PROMOTION_REVIEW(evidence_episode_id,decision)`、
`DEGRADATION_REPORT(subject_id,state)`、`REPLAN_PROPOSAL(cause_event_id,changes)`、
`MISSION_GRAPH_PROPOSAL(nodes)`。通用摘要或空 JSON 不能推进 Mission DAG。

`DATA_QUALITY_REPORT` 只有 `quality_state=VALID` 且 `pit_state=VALID` 才能成为
`VALIDATED` 输出；明确报告失败状态不会解锁后续 Mission。

Codex Thread 是 Mission 执行上下文，不是业务状态。

## 7. Mission Graph 与自治调度

Research Program 使用持久化 Mission DAG，不使用“让 Agent 一直继续研究”的无限聊天。

当前 Start 固定持久化的节点：

```text
PLAN_RESEARCH
DATA_QUALITY
ALPHA_DISCOVERY
ROBUSTNESS
PORTFOLIO_ASSEMBLY
SEALED_PROMOTION_REVIEW
```

Start 图只包含这六个节点。当前 Orchestrator 只按已持久化依赖解锁下一个固定节点，
并受并发/turn/tool-call budget 约束；`DATA_REQUIREMENT`、`FEATURE_RESEARCH`、
`CALIBRATION` 等 MissionType 不会由 Agent 任意扩张到 Start 图。

唯一的受限例外是持久化 Degradation Wake：Domain Validator 只能从有效的
`ForwardEvidenceEpisode` 创建一个新的 Cycle/Branch，且图固定为：

```text
DEGRADATION_DIAGNOSIS → REPLAN
```

它不是 Start 图的第七节点，也不允许 Codex 自己创建、改变依赖或推进领域状态。
Wake 的因果 Observation、WakeEvent、Cycle、Branch 和 Mission 必须在同一领域事务中
持久化；失败不得留下可运行的半张图。

## 8. Search Ledger 与 Evidence Exposure

### 8.1 Search Ledger

必须记录所有重要尝试，而不仅是赢家：

- Feature family；
- Alpha family / model；
- Calibration；
- 参数区域；
- 数据切分；
- Portfolio Alpha subset；
- Policy family；
- Constraint set；
- Promotion 尝试。

用于 multi-testing 调整、重复检测和 Material Improvement 判断。

### 8.2 Evidence Exposure Graph

任何被 Codex、人类或后代候选看到的独立评估信息都会形成 Exposure。Exposure 沿 lineage 传播：

```text
Evaluation → Mission/Branch → Alpha → Portfolio Candidate → descendants
```

不能通过新 Thread、新 Branch、复制 Program、重命名模型或重新提交相同 Idea 清零。

---

# Part III — Data & Research Engine

## 9. Market Universe

`MarketUniverseVersion` 是一等领域对象，例如：

```text
US Equities
US Options
Crypto Spot
Prediction Markets
FX
Futures
```

版本定义：

```text
instrument_schema
calendar/session semantics
membership rules
data requirements
cost_model_family
capacity_model_family
risk_model_compatibility
currency semantics
allowed alpha roles
downstream compatibility
```

Research Charter 必须明确绑定一个或多个 Universe Version。Alpha Qualification 始终是 `Alpha + Universe + Horizon` 范围内的结论。

## 10. Governed Autonomous Data Acquisition

### 10.1 Data Source Registry

管理员批准并版本化 Data Source / Connector：

```text
data_source_id
plugin_release_id
provider
source_type
universe_scope
fields
license_classification
allowed_usage
credential_requirement
availability_semantics
revision_policy
update_cadence
state
```

Codex 可以自主声明 Data Requirement、查询已批准能力和请求 ingestion，但不能：

- 任意 `curl/wget` 抓取 canonical data；
- 自行接受数据许可；
- 读取 provider secrets；
- 通过安装下载器绕过 Registry；
- 把网页、搜索 snippet 或模型记忆直接作为 canonical market data。

互联网检索只能用于允许的定性假设形成；进入 Research Engine 的定量数据必须通过批准 Connector。

### 10.1.1 Canonical Data Source preflight

登记 Data Source 不是连接成功。Operator 只能通过 canonical
`POST /api/v1/data-sources/{data_source_id}/preflight` 请求一次异步 preflight；请求体为
`{}`，不接受 URL、host、endpoint、plugin path、`source_spec` 或任何 credential。它只消费
已登记 Data Source 的公开 plugin manifest contract、provider/license、唯一 active Universe
scope，以及受治理的 `plugin_release_id` / `plugin_runtime_bundle_id` binding。

Core 在 worker 中确认 release 为 `ACTIVE`、bundle 为 `READY` 且含该 release 的
`IMPORTER` member，然后通过独立 runtime 的 manifest scanner 取得真实
`ArchiveManifestDescriptor`。本地 plugin construction/preflight 不能把 Data Source 置为
`READY`。只有 descriptor 与请求事实一致且没有 `PROBE_ERROR` 时，Core 才原子地保存
materializable immutable `ACTIVE ArchiveManifest` / shard evidence，固定
`archive_manifest_id`，并将 Data Source 从 `PENDING` 置为 `READY`。`MISSING` 是已知数据
间隔而非虚构成功，必须保留；后续 materialization 仍按其范围产生 quality evidence。
`PROBE_ERROR` 只保存不可 materialize 的 immutable `INCONCLUSIVE ArchiveManifest` / shard
audit evidence，让 Source 保持 `PENDING` 且不固定 Manifest。任何其他
runtime/binding/contract failure 也让 Source 保持 `PENDING`，不固定 Manifest，也不能绕过
Dataset 的 quality/PIT gates。

扫描 URL 只能由 approved plugin 从其受限公开 contract 生成；preflight 不接受或构造用户提供的
远端 locator，且不把 provider/downstream secret 写入 Job 或 Event。

### 10.1.2 Trusted sealed catalog provision

Sealed raw Catalog 先由独立的受信 sealed runtime provisioning/import path 写入；Core 不创建、
上传、下载或读取该 raw data。Operator 仍使用 canonical
`POST /api/v1/datasets/materializations`，但 `partition=SEALED` 时只能提交一个不透明、受限格式的
`catalog://` reference，不能提交 URL、`source_spec`、shard、plugin binding 或 credential。

该请求只在已登记且 preflight-`READY` 的 Data Source / Universe 范围内创建 PENDING immutable
Dataset Revision 和 `SEALED_CATALOG_PROVISION` job。该 job 使用独立 sealed runtime profile 调用
`validate_catalog`，只接收受控 descriptor，并校验 reference、sealed flag、provider、scope、
source license、时间范围、质量与 PIT。它不 import plugin、不调用 ingest，也不把 reference 或
raw data 放进 Job/Event payload；quality/PIT 只持久化 validated `valid` aggregate 和本地 reason
code。验证成功才原子写入 `DataQualityResult`、Dataset Revision 与不可变
`NautilusCatalogBinding(sealed=true)`；任何本地或 descriptor 不一致都成为 NON_PROMOTABLE data
evidence，runtime 不可达则保持 PENDING。Codex/MCP 从不获得该 reference 或 sealed raw data。

### 10.1.3 PMXT Archive historical connector plugin

PMXT Archive 以 `DATA_CONNECTOR` / `HISTORICAL_IMPORT` runtime plugin 形式提供只读历史市场数据能力，不是交易 venue、broker 或 downstream runtime。PMXT 的 primary wheel 为 `quazonai-pmxt-archive`，通过唯一 `quazonai.plugins` entry point 注册 `pmxt_archive`；Core 不包含 PMXT-specific dispatch 分支。

通用 Catalog ingest 使用已验证并激活的 plugin release/runtime bundle，`source_spec` 只保存 connector 的公开配置：

```json
{
  "kind": "plugin",
  "config": {
    "venue": "polymarket_v2 | kalshi",
    "archive_url": "https://r2v2.pmxt.dev/polymarket_orderbook_YYYY-MM-DDTHH.parquet",
    "instrument": "<asset_id 或 market_ticker>",
    "instrument_symbol": "<可选的本地 BinaryOption symbol>"
  }
}
```

当研究范围是整个市场和整个归档历史时，使用通用 `ArchiveManifest`，而不是把所有小时文件拼成一个本地 Catalog：

```json
{
  "kind": "plugin",
  "config": {
    "venue": "polymarket_v2",
    "selection": "all_markets",
    "archive_start": "2026-04-13T19:00:00Z",
    "archive_end": "2026-08-31T03:00:00Z"
  }
}
```

Manifest 只登记插件根据固定官方 URL 规则探测到的小时分片（URL、UTC 小时范围、大小、存在/缺失/探测错误状态和探测时间），不下载原始 Parquet。缺失小时必须保留在清单中并作为研究数据间隔；清单本身是不可变的，重新扫描或范围变化创建新的 Manifest。Research runtime 按研究请求选择分片和 instrument，在有界缓存中按需物化临时 Nautilus Catalog；缓存淘汰不改变 Manifest 或 Dataset Revision，也不使用应用级 hash/checksum 身份。

Manifest 的按需物化使用同一通用 `CatalogIngestSpec`，增加可选的 `source_shards` 列表和 `source_spec.materialization` 描述。Core 只允许选择单一 instrument、UTC 小时对齐的 `[start, end)` 范围和 Manifest 中状态为 `AVAILABLE` 的固定分片；单次请求最多 168 个小时、估算源文件最多 20 GiB。缺失/探测错误小时留在 materialization quality evidence 中，不被当成 Alpha failure。Core 不下载、不解析 provider 数据，也不为 PMXT 增加特殊分支；独立 runtime 将选定分片传给对应 `DATA_CONNECTOR` plugin child，由插件逐个下载、以最多 16,384 行和 64 MiB 解码批次按 instrument 过滤，累计解码输入最多 4 GiB，按 `timestamp_received` 合并状态并写出新的 Catalog。runtime 对生成的 Catalog 只用最多 16,384 行和 64 MiB 的 Arrow 批次扫描时间列完成边界校验，不把整库 materialize 到内存；plugin child 还必须继承 6 GiB address-space 上限，reference Research/Sealed runtime 容器各自设置 10 GiB memory 上限。子进程地址空间与暂存输出配额合计低于容器上限并留有 runtime headroom。每个 materialization 都创建新的 immutable Dataset Revision，不能向既有 Catalog 原地追加。Reference runtime 拒绝 plugin 直接写入 `sealed=true` Catalog；sealed raw data 必须先由受信 provisioning/import path 写入独立 sealed Catalog，再供 evaluator 只读验证。

`plugin_release_id` 与 `plugin_runtime_bundle_id` 是 Operator Catalog ingest 请求的受治理绑定；Core 校验 release 为 ACTIVE、bundle 为 READY 且包含该 release 的 `IMPORTER` member，再向独立 Nautilus runtime 传递不含 secret 的 plugin id/version 和 bundle path。runtime 只通过通用 connector-runner child 调用 plugin entry point；API、worker、agent-worker、evaluator 长进程不 import plugin。

PMXT plugin 只接受 PMXT Archive 公布的、与 venue 匹配的固定小时 Parquet URL，并拒绝重定向、凭据、任意 host、查询参数和非 Parquet 路径；Manifest 扫描只从受约束的 UTC 小时范围生成这些 URL，不从网页抓取链接。每次“一个小时文件 + 一个 instrument”导入都创建新的 immutable Dataset Revision，不能把后续文件原地追加到旧 Revision；Manifest 物化出的每个研究切片同样必须创建新的 immutable Dataset Revision。

PMXT plugin 支持 Polymarket v2 与 Kalshi orderbook 到 Nautilus `QuoteTick` 的历史转换，instrument 以 PMXT `BinaryOption` 表示，导入结果只用于 Research/Sealed Catalog。PMXT 的 `timestamp_received` 作为 point-in-time `available_at`；当源文件缺失事件时间时使用接收时间作为事件时间，并在 quality evidence 中记录 fallback 计数；事件时钟晚于接收时钟也只记录异常，不得改写可用时间。跨 Manifest 缺口时必须重置重建盘口状态。源数据的 bids/asks、排序、交叉报价、缺失和转换跳过行必须进入 Dataset quality evidence。

PMXT plugin 不保存或请求 provider secret，不调用 PMXT 交易接口，不输出 order、fill、position、account 或 NAV，也不授予 QZ 启停、撤单、平仓或恢复任何 downstream 的能力。未来其他历史数据源必须复用同一通用 plugin/importer contract，不得在 Core 或 Nautilus runtime 增加 provider-specific 分支。

每个 materialization 使用与 immutable Catalog 分离、每实例 3 GiB 配额的 tmpfs 暂存区；单次导入最多发布 2 GiB、10,000 个常规 Parquet 文件，插件 child 继承 6 GiB address-space 上限，runtime 只以有界 Arrow 批次校验已发布字段，插件 stdout/stderr 通过流式有界读取并拒绝超限，避免第三方 importer 把持久卷或 runtime 内存耗尽。上述限制属于通用 plugin runner 边界，不是 PMXT 特例。

### 10.2 Dataset Revision

每份 Dataset Revision 显式记录：

```text
dataset_revision_id
data_source_id
connector_release_id
revision_no
universe_version_id
schema_version
event_time_range
available_time_range
ingested_at
field_definitions
units
revision_policy
license_classification
quality_state
point_in_time_state
```

必须区分：

- event / observation time；
- 现实世界信息可获得时间；
- QZ ingestion time。

质量检查至少覆盖 schema、排序、重复、缺失、coverage、单位/币种、survivorship、revision/restatement 和 look-ahead。

数据质量失败是 Data Evidence，不得被解释为 Alpha 失败。

### 10.3 Evaluation Dataset Selection

`EvaluationDatasetSelection` 是 Administration 创建的不可变版本事实：它为一个
Universe 显式绑定一个 `DISCOVERY`、一个 `VALIDATION` 和一个 `SEALED` Dataset Revision。
三者都必须是 `VALID/PIT-VALID/PROMOTABLE`，Sealed binding 还必须是独立 provisioned
sealed Catalog。Start 或 Alpha Artifact Validator 只能使用唯一启用的 Selection；缺失或
歧义时返回 `EVALUATION_DATASET_SELECTION_REQUIRED`，不选择最新 Revision。Discovery/
Validation 引用可按冻结 Mission Contract 最小授权给对应 Mission；Sealed 引用只能进入
Core Assignment 与独立 Evaluator。

## 11. Independent evaluation boundary

QZ owns the governed input and disclosed result of research/evaluation, not an
execution runtime. The only accepted research output is a typed Alpha signal;
the only Package payload is a validated `TargetPortfolioFrame`. A separately
operated evaluator may consume those bounded inputs for simulation or reference
fixture conformance, but it receives no QZ database credential, Codex secret,
broker credential, or downstream-control authority.

```text
governed Dataset / AlphaSignalFrame / TargetPortfolioFrame
→ trusted independent evaluator
→ controlled aggregate evidence or conformance result
→ QZ domain validation
```

QZ persists governed identifiers, PIT/quality results and permitted aggregate
evidence. It does not persist or expose orders, fills, positions, accounts,
NAV, node configuration, or runtime-control reports. Sealed raw data stays
outside the Mission process and is not a Tool/API payload.

### 11.1 Current implementation boundary

The checked-in target-only Package builder can ask an independently configured
runtime to validate its archive. That is package conformance only; it is not
evidence that a full Discovery/Sealed/Portfolio/Paper/Live pipeline has run.
Likewise, the Alpha signal contracts and honest-evaluation primitives do not by
themselves create a qualified Alpha or a Promotion decision in the database.

Any remaining generated-strategy or remote execution implementation is legacy
removal work. It is not a V1 Mission artifact contract, may not receive new
Mission input, and may not become a compatibility path for Package or Handoff.

### 11.2 Mission isolation

Mission `workspace-write` is bounded by the Mission worktree, network policy
and outer filesystem isolation. The worker must fail closed if its sandbox
preflight cannot establish those boundaries; it must not use `privileged`,
`CAP_SYS_ADMIN`, or a sandbox bypass. Mission processes never receive Sealed
data, database credentials, provider/downstream secrets, or execution access.

### 11.3 Ownership

```text
Alpha selection / Mandate / target-weight optimization -> QuaZonai
simulation internals and execution state               -> independent downstream/evaluator
Research / evidence / promotion policy                 -> QuaZonai
```

No QZ API starts, stops, recovers, cancels, replaces or otherwise controls a
Paper/Live runtime. A successful target-frame conformance result cannot be
reported as a Handoff, Portfolio, or promotion success without its separate
persisted gates.

## 12. Alpha Contract

标准链路：

```text
Feature Pipeline
→ Alpha Model
→ RawAlphaFrame
→ Alpha Calibration
→ CalibratedAlphaFrame
→ Risk / Cost / Capacity Models
→ Portfolio Policy
→ TargetPortfolioFrame
→ Canonical Evaluator
```

`RawAlphaFrame.mode`：

- `RELATIVE_SCORE`：只承诺排序；
- `CALIBRATED_RETURN`：必须经独立 Calibration 后才可被当作经济量纲 expected return。

`CalibratedAlphaFrame` 至少：

```text
as_of_time
valid_from
valid_until
universe_version_id
instrument_id
prediction_horizon
expected_return nullable
relative_score nullable
uncertainty
confidence
alpha_model_version_id
calibration_version_id
```

未校准 relative score 禁止直接输入要求 expected return 的 Mean-Variance / Mean-CVaR policy。

## 13. Alpha Library

### 13.1 双通道准入

1. Standalone Quality Gate → `PRIMARY_ALPHA`；
2. Portfolio Contribution Gate → `DIVERSIFIER_ALPHA` / `HEDGE_ALPHA` / `REGIME_SIGNAL` / `RISK_MODULATOR`。

### 13.2 Shadow Alpha

`SHADOW_ALPHA` 保存已具研究价值、但尚未证明独立预测能力的资产。它可以参加受限 Portfolio Contribution research，但不能：

- 单独形成 Handoff；
- 直接进入 Live Portfolio；
- 宣称 Standalone Alpha 已通过。

### 13.3 Qualification 生命周期

Qualification Version 不可变，状态只前进：

```text
ACTIVE → WATCH → QUARANTINED → RETIRED
SHADOW → QUARANTINED → RETIRED
```

旧 Qualification 绝不从 `QUARANTINED` 恢复为 `ACTIVE`。市场条件恢复时必须产生新的 Model / Calibration / Qualification Version。

---

# Part IV — Independent Evaluation

## 14. 三层证据区

1. **Discovery Zone**：Codex 可见；用于研究与调参；
2. **Sealed Promotion Zone**：Codex 无原始数据访问；独立 evaluator 执行；
3. **Forward Evidence**：候选形成后的新市场数据与下游 Paper/Live feedback。

Evaluation Episode：

```text
PLANNED → SEALED → ASSIGNED → EVALUATING → EVALUATED → DISCLOSED → CONSUMED
                    └→ FAILED             └→ INVALIDATED
```

一旦披露任何会影响后续研究的信息，该 Episode 对该 lineage 永久 `CONSUMED`，不能重新充当独立证据。

### 14.1 Trusted Alpha Evaluation Assignment

`AlphaEvaluationAssignment` 是独立于 `AlphaEvaluationEpisode` 的不可变、Core-owned
输入事实。Assignment 冻结一个已经 `VALIDATED` 的 `ALPHA_PROPOSAL` Mission Artifact
（ID + revision）、其 `VALID` AlphaDiscoveryEvaluation、Alpha Model/Calibration Version、明确的
Discovery/Validation/Sealed Dataset Revision、绑定的 Promotion Policy Version 和 evaluator
contract version。它不保存
raw bar、raw return、signal frame、secret、任意 URL 或内容 hash。

`EvaluationDesignVersion` 是 server-owned、版本化的统计设计：它定义允许的 Model/Role、
walk-forward split、annualization、multiple-testing、qualification gate 和 Level-1 disclosure
mapping。Assignment 冻结其 ID；Agent 不能修改阈值、trial count、role 或 disclosure policy。

Core 的 Artifact Validator 是唯一 Assignment 写者：它验证 Proposal 属于已完成的
`ALPHA_DISCOVERY` Mission，所有 Discovery evidence 均属于同一 Program/Branch 和冻结的
Discovery Dataset；再从已启用的、显式 Dataset Selection 取得 Validation/Sealed Dataset
及 Policy。零个或多个可用 Selection 都是 `INCONCLUSIVE`，不能猜测“最新”数据。Sealed
Dataset ID 永不进入 Mission Contract 或 MCP。无效 Proposal 标为 `REJECTED`，不能解锁
后续 Mission。

Validator 只接收一个 worktree 内的常规 Python source file，复制到 QZ-owned artifact
storage 前按字节流上限 1 MiB 拒绝；它不跟随 link、目录、归档或任意外部 URI。该固定 V1
边界防止 Mission 输出耗尽持久卷，不引入可由 Agent 改写的容量配置。

`AlphaDiscoveryEvaluation` 是这条前置 evidence 的 Core-owned、不变输入/结果事实：它冻结
已验证 Proposal（ID + revision）、其尚为 `DRAFT` 的 Alpha Model Version、
Program/Cycle/Branch/Mission、唯一启用的 `EvaluationDatasetSelection`、其 Discovery Dataset、
`EvaluationDesignVersion`、cause Event 与 discovery evaluator contract。`DISCOVERY_EVALUATION` durable job 只引用该
事实且 payload 为空；隔离的 Discovery Evaluator 才可加载受限的模型 artifact 和 Discovery
数据，并只回传受 schema 限制的 aggregate/gate outcome。只有 `VALID` Discovery Evaluation
才能成为最终 `AlphaEvaluationAssignment` 的证据；`INCONCLUSIVE`/`INVALID` 记录尝试但不能
创建 Assignment。它不复用已退役的 Quant Runtime Run，也不接受 Agent 声称的 run ID、raw
数据、任意 URI 或 hash。

每个终态 `AlphaDiscoveryEvaluation` 自身必须冻结私有 result UUID、有限 aggregate、固定
gate 与 outcome；不为同一条已经终态的 Evaluation 再造平行 Result 身份。`VALID`
raw-score 模型可由同一独立 Evaluator 产生一个受信 Calibration artifact。Core 只在该
Discovery Evaluation 与其冻结 Discovery Dataset 均有效时创建 `AlphaCalibrationVersion`，
并把其 provenance 固定到该 Evaluation；Calibration artifact 的位置由 evaluator-private
UUID 映射，不由 Agent/Job/API 提供 URI。没有有效 Calibration 的模型仍可保留为
relative-score 研究事实，但不得进入需要 expected return 的 Portfolio input。无需为这条
同一冻结 Discovery 数据上的确定性计算另造 Agent Mission、任意 JSON training evidence
或第二个可选 job。

`RELATIVE_SCORE` 的 source Model Version 与这个受信 Calibration Version 共同构成
`CalibratedAlphaFrame`；Assignment 必须冻结两者，不能为此复制或重标一个
`CALIBRATED_RETURN` Model Version。没有该受信 Calibration 的 Assignment 可以保留
研究结果，但不能产生可进入 mean-variance Portfolio 的 Qualification。

`ALPHA_EVALUATION` durable job 只引用 Assignment ID，Job payload 必须为空。独立 Sealed
Evaluator 只在自己的隔离进程中读取已冻结 Model Artifact 与 Sealed Catalog；它向 Core
返回受 schema 限制的 result，Core 事务性写入 `AlphaEvaluationEpisode`、
`AlphaSignalArtifact`、确定性 Level-1 Disclosure / Evidence Exposure，并且仅在所有
统计、PIT、Calibration、Policy gate 通过时创建不可变 Qualification。Agent、API、CLI、
Job payload 和旧 Nautilus runtime 都不能提供或读取 evaluator 输入/结果的 raw 值。
为填充既有 Signal Artifact 的不可变元数据，Alpha result 只可另带 `row_count` 与 UTC
event/available interval 的 typed Signal Summary；Core 校验 PIT 顺序、固定 schema/mode，
并只由 evaluator-private result UUID 派生内部 locator。result 不携带 URI、frame 或 raw rows。

若一个通过的 Alpha 可参与 V1 Portfolio，Sealed Evaluator 还必须为它写入恰好一个
`AlphaEvaluationForecast`：冻结 `evaluation_result_id`、`AlphaSignalArtifact`、instrument、
as-of / effective interval、有限的 expected return、uncertainty、confidence 以及有限的
capacity envelope（最大 trade/position notional、participation、liquidation days、stressed
capacity）。这是只供 Core Portfolio Input Evaluator 消费的受限 aggregate，不是 raw signal
frame、URI 或 Mission disclosure。一个可进入 V1 mean-variance 的 Qualification 必须有且
只有一个这样的 Forecast；多 instrument 或缺少 forecast 的结果可以保留研究/披露事实，
但不得产生 Portfolio input、Candidate 或 Approval。

`AlphaEvaluationEpisode` 是结果生命周期/披露事实，Assignment 是输入事实；二者一一
绑定但不能互相替代。重试必须复用同一 Assignment、Episode 和引用，不得重新选数据或
以 hash 判断输入是否相同。

Discovery 与 Sealed Evaluator 都经同一个固定的 operator-owned executable 边界运行：
`QUAZONAI_TRUSTED_EVALUATOR_COMMAND` 只能是一个绝对可执行文件路径，Core 为每个 Job
创建只含 `kind` 与受 schema 限制 UUID/revision 引用的临时 descriptor，并将该 descriptor
路径作为唯一参数传入。Evaluator 以自己的受信任配置和只读挂载解析这些引用，stdout
只能返回一个 typed result；Core 在自己的事务中验证、持久化或拒绝它。该 child 不继承
QZ database URL、Codex credential、downstream credential 或 Mission workspace；不存在
该 executable、descriptor 或可验证 result 时 Job 失败，不创建 Alpha 事实。它不是插件
注册表、通用 shell hook、API 或 raw-data transport。

## 15. Disclosure Policy

- Level 0：Evaluator 私有完整结果；
- Level 1：Codex 仅接收确定性分类反馈；
- Level 2：人类 Approval 聚合报告；
- Level 3：Program/Branch/Episode 永久退出后的 postmortem。

Codex Level 1 不返回：具体日期、具体 instrument、精确失败指标、阈值差距、逐期收益或明确参数修改方向。

分类示例：

```text
INSUFFICIENT_NET_EDGE
TEMPORAL_INSTABILITY
REGIME_INSTABILITY
CALIBRATION_FAILURE
SIGNAL_DECAY
COST_SENSITIVITY
CAPACITY_FAILURE
CONCENTRATION_FAILURE
TURNOVER_FAILURE
SEARCH_ADJUSTED_FAILURE
DATA_QUALITY_FAILURE
REDUNDANCY_FAILURE
MARGINAL_CONTRIBUTION_FAILURE
TAIL_DEPENDENCE_FAILURE
WEIGHT_INSTABILITY
POLICY_SENSITIVITY
```

Level 1 由 deterministic mapping 产生，不由 LLM 自由总结 Sealed 明细。

---

# Part V — Portfolio Construction

## 16. Portfolio Mandate

QZ 支持多个长期、命名、版本化 Portfolio Mandate，例如：

```text
Core Growth
Conservative
Market Neutral
Tail Protection
```

首次安装只启用一个默认 Mandate，其余模板按需启用。Codex 不创建资本目标。

Mandate Version 至少：

```text
mandate_version_id
name
objective
risk_preferences
target_behavior
concentration_constraints
turnover_preference
capacity_requirements
allowed_alpha_roles[]
allowed_policy_families[]
allowed_universe_versions[]
rebalance_philosophy
downstream_compatibility
validity_conditions
state
```

任何实质变化创建新 Version，旧 Portfolio Candidate 不漂移。

## 17. Capital Context

QZ 不连接 broker 账户，但 Portfolio Promotion 必须有现实资金尺度。使用版本化 `CapitalContextVersion`：

```text
capital_context_version_id
source_type        # ADMIN | DOWNSTREAM_FEEDBACK
source_downstream_system_id nullable
base_currency
deployable_capital
observed_at
valid_until
notes
```

它是**研究输入快照**，不是账户、仓位或可用资金事实账本。QZ 不读取账户 credential，也不推断订单级可用资金。

Discovery 可以运行多个 capital scenario；Paper/Live Promotion 必须冻结当前 Capital Context，并验证 Candidate 的 capacity envelope 覆盖该金额。

### 17.1 V1 typed mandate and capital configuration

`LONG_ONLY_MEAN_VARIANCE_V1` 是唯一可写的 V1 Mandate policy。Mandate Version 以 typed
fields 固化单一 Universe、合格 role、minimum/maximum weight、gross/net exposure、turnover
与 variance limit、risk/cost/uncertainty aversion、commission/half-spread/slippage/impact
rates 与 impact breakpoint；`cash_reserve` 必须为零。未知 policy key、group/custom
constraint、自由 JSON 解释或多个 Universe 一律在配置时拒绝。

Capital Context 只能由 Operator Administration 创建新的不可变 Version，显式填写 currency、
deployable capital、observed_at 和 valid_until；它不是从 Mandate JSON、下游账户或缺省值
推导的。初始 Candidate 的 previous target weight 明确定义为零；之后只可读取同一
Portfolio Program 的前一 QZ TargetPortfolioFrame，绝不读取下游 position。

新的 V1 Capital Context 固化 `configuration_contract_version=CAPITAL_CONTEXT_V1`。迁移前的
Capital Context 即使 `source_type=ADMIN` 也没有这个版本化 typed contract，必须标为 legacy
unavailable，不能被 Input Evaluator 当作现实资金快照。

## 18. Portfolio Program 自动创建

满足以下条件时自动创建 Portfolio Program：

```text
Enabled Mandate
+ Qualified Alpha Pool
+ 可证明的组合机会
+ 不存在等价活跃 Portfolio Program
```

没有合格 Alpha 时是 `WAITING_FOR_ALPHA`，不是失败。

V1 对每个 Mandate Version 只创建或复用一个等价的 Portfolio Program，且数据库以
`mandate_version_id` 唯一约束强制此身份，Program 只处理一个 Universe。Core 只可在先验证
至少两个完整、互异的 forecast axis 后创建或复用其
`WAITING_FOR_ALPHA` Program，作为 `PortfolioInputEvaluationAssignment` 的稳定锚点；这不创建
Candidate，也不表示 covariance/Capital/Policy 已就绪。只有唯一的系统 Portfolio Input
Evaluator 从可信、版本化事实记录完整不可变 `PortfolioAssemblyInput` 后，才可将 Program
唤醒为 active assembly。缺少任一输入时保持 `WAITING_FOR_ALPHA` 或记录
`INCONCLUSIVE`，不得补零、猜测或开放人工 assemble API。Codex 只能提出候选 Alpha/rationale，
不能写 Input、权重、Candidate 或 Approval。

V1 的真实自动 trigger 只生产每个 Program/Family 的首个 Candidate：在同一 trusted Alpha
acceptance transaction 中，新建 `ACTIVE PRIMARY_ALPHA` Qualification 后，Core 可枚举全部
enabled、typed V1、且单一 Universe/role 匹配的 Mandate Version；对每个 Mandate 先确定性
收集完整 eligible `ACTIVE` pool，绝不先创建空 Program/Family。只有至少两个完整 axis 时才
锁定/创建其唯一 Mandate Program；且该
Program/Family 尚无 Candidate 时，Core 才创建 `PORTFOLIO_MANDATE` cause Event、冻结
`previous_candidate_id=NULL` 的 Assignment，并排队空 payload
`PORTFOLIO_INPUT_EVALUATION` job。它绝不读取或写入 `current_candidate_id`，也不从已存在
Candidate 推断 successor；非首个 Candidate 必须等待未来显式的、冻结 predecessor 的因果事实。
同一 Program/Family 已有首个 in-flight Assignment 或 `PENDING` Input 时也不得再创建初始
Assignment；同一 cause/retry 必须收敛到已有的冻结事实，而不是并发地产生竞争的首个 Input。

Portfolio Program 永久绑定一个 Mandate Version，并在 Promotion 时冻结 Capital Context
Version。V1 的 Mandate 与 Input 都只接受单一 `universe_version_id`；multi-universe 是
§20 的后续显式能力，在 V1 必须 fail closed。

## 19. Staged Portfolio Assembly

```text
Alpha Library
→ Eligibility / Role Pools
→ Redundancy / Common-source Clustering
→ Portfolio Skeletons
→ Approved Policy Families
→ complete PortfolioAssemblyInput
→ deterministic Portfolio Assembly
→ Discovery Evaluation
→ Robustness / Marginal Contribution
→ Candidate Family
→ Portfolio-level Sealed Evaluation
```

Eligibility 是选择阶段，不再单独持久化为可漂移的 Snapshot。
`PortfolioAssemblyInput` 是该选择的唯一冻结事实，也是 deterministic Portfolio Engine
的唯一生产输入；它只由系统 Portfolio Input Evaluator 一次性写入，不能 patch。它至少
绑定：

- `portfolio_program_id`、`mandate_version_id`、有效的 `capital_context_version_id`、单一
  `universe_version_id`、因果 `cause_event_id`、`snapshot_no` 与 `as_of_time`；
- 每个已选 Alpha Qualification 的显式 ID、Signal Artifact ID、instrument、expected
  return、uncertainty、confidence、previous target weight 与 capacity estimate；
- 以 Alpha axis 明确索引的 covariance 上三角，以及其 method、observations、decay 和
  shrinkage；
- V1 `LONG_ONLY_MEAN_VARIANCE_V1` 的显式约束、risk/cost/uncertainty aversion 和
  commission、spread、slippage、impact 参数。

这些字段是 typed columns/relations，不是 `alpha_set_json`、matrix JSON、`risk_config`、
`cost_config`、`capacity_config` 或 `constraint_config` 的自由解释。V1 只支持当前明确定义
的 long-only scalar constraints；未映射的 group/custom constraint、未知 policy key、
`cash_reserve != 0` 或多个 Universe 一律拒绝，且不产生 Candidate。Cash/position/order
语义仍不属于 QZ；previous weights 只能来自先前 QZ Candidate 的 TargetPortfolioFrame，
不是下游 position。

Input 必须完整后才可持久化和排队；缺失 expected return、uncertainty、confidence、
covariance、capacity 或有效 Capital Context 时，在 Portfolio Search Ledger 记录
`INCONCLUSIVE` reason，不能写半成品 Input。Input 的语义字段不可更新；只有其有限的
处理结果 `PENDING | ASSEMBLED | INFEASIBLE | STALE | INVALID` 与完成时间可由 worker
推进。Input UUID、Candidate UUID、Package UUID/revision 和 cause event 是显式身份；不用
digest、hash 或伪造 Candidate revision。

V1 唯一 approved policy family 是 Constrained Mean-Variance
(`LONG_ONLY_MEAN_VARIANCE_V1`)。Equal Weight、Volatility Scaling、Risk Parity、
Hierarchical Risk Parity 和 Mean-CVaR 只有在各自的 typed input contract、独立 evidence
和验证落地后才可成为新的显式 policy family；不能作为未知 JSON 的 fallback。

`PortfolioSearchLedger` 保存 Alpha subset、role、policy、constraint、rebalance 和结果。

一个 `PORTFOLIO_ASSEMBLY` job 只引用已冻结的 Input，重试必须收敛到同一 Input 与
Candidate。它先验证所有引用仍有效，再产生 Candidate。后续独立的
`CANDIDATE_PACKAGE_BUILD` job 只引用该 Candidate，预留一个 target-only Package revision；
Package 变为 `AVAILABLE` 后才可被后续流程读取。文件归档与数据库不是同一原子资源，
因此 package worker 必须以 package ID/revision 和 manifest 中的显式引用恢复或验收
`BUILDING` Package，不能以 hash 判断重试结果。

V1 Candidate 在 assembly transaction 中一次性成为 `ASSEMBLED`；它不因 Package 文件写入而
改写。Package 是独立的 `BUILDING → AVAILABLE` 处理状态，Promotion 同时要求
`ASSEMBLED` Candidate 与 `AVAILABLE` Package。TargetPortfolioFrame 冻结
`portfolio_state='ASSEMBLED'`，不是未来可变的 handoff/readiness 标记。

Assembly 成功也不得直接创建 Approval。只有独立 Portfolio evidence 已完成、
material-improvement/policy gate 通过，且已有 `AVAILABLE` Package 时，后续 promotion 流程才可创建
绑定该 Candidate ID + Package ID/revision 的 Paper Approval；Paper 与 Live 仍分开。

`PortfolioInputEvaluationAssignment` 是 `PORTFOLIO_INPUT_EVALUATION` 的不可变、Core-owned
输入事实。它冻结 Program、typed Mandate、有效 Capital Context、明确的 Sealed Dataset
Selection、`PORTFOLIO_TO_PAPER` Policy、cause Event、明确的 `previous_candidate_id`（首个
Candidate 时为 NULL），以及按 axis 排列的合格 Alpha
Qualification、其 Sealed Result / Signal Artifact / 唯一 Forecast。只要同一 as-of 时点没有
至少两个完整且不重复的 axis、或缺少有效 Capital/Policy，Core 只写
`PortfolioSearchLedger(INCONCLUSIVE)`，不能写半成品 Assignment 或猜测“latest”。
`PORTFOLIO_INPUT_EVALUATION` job 只引用这个 Assignment 且 payload 为空；同一受信 evaluator
边界只回传该固定 axis 的有限 covariance 上三角及其 method、observations、decay、shrinkage
和 evaluator-private result UUID。它不能接收或输出 raw returns、matrix JSON、URI、secret 或
Job payload。Core 验证轴完整、时间一致和每个有限值后，才一次性写完整
`PortfolioAssemblyInput`/Member/Covariance 关系并排队空 payload 的 `PORTFOLIO_ASSEMBLY` job。
V1 的 covariance method 唯一且严格为 `EWMA_SHRINKAGE`；其他 method label（包括未知或
自由字符串）一律拒绝，不能被当作等价 fallback。
previous weights 只从 Assignment 已冻结 predecessor 的 relational Candidate Members 读取；
不得在 evaluator 返回后读取 `current_candidate_id` 或按时间挑选“latest”。

因此 `PORTFOLIO_INPUT_EVALUATION` 是唯一 `PortfolioAssemblyInput` 写者。它只消费已完成的
Alpha Assignment/Episode/Qualification、上述冻结 Assignment、typed Mandate、有效 Capital
Context 和因果 Event；缺任何一项时只写 `PortfolioSearchLedger(INCONCLUSIVE)`。它不能从
`Qualification.metrics`、Mission Artifact、任意 JSON、下游 position 或 Job payload 补值。
Assembly job 从关系行重建 `OptimizationInput`，只复用确定性 target-weight engine，且只在
`OPTIMAL` 时创建 Candidate；Package 只由后续独立 build job 预留。

`PORTFOLIO_ASSEMBLY` 仅在 `OPTIMAL` 时创建 `ASSEMBLED` Candidate、Candidate Member 与因果
Event；同一成功事务随即创建唯一的 `PortfolioEvaluationAssignment` 和 `ASSIGNED`
`PortfolioEvaluationEpisode`，并排队空 payload 的 `PORTFOLIO_EVALUATION` job。Assignment
冻结 Candidate、Candidate Family、nullable 的 `previous_candidate_id`、Assembly Input、Sealed
Dataset Selection/revision、`PORTFOLIO_TO_PAPER` policy 与 cause Event。独立 evaluator 只接收
这些显式引用，返回受限的 Portfolio evidence、Level-1 disclosure、有限 typed metrics/gates 和
evaluator-private result UUID；它不调用旧 Nautilus runtime，不接受 raw Job payload，不创建
Approval。Core 是唯一的 Episode/metric/result writer。

Portfolio evidence 没有 `CANDIDATE_CURRENT`、`current_candidate_id` 或 latest 语义。非首个
Candidate 的 evaluator 只能与 Assignment 已冻结的 predecessor 比较。首个 Family baseline
明确为 `previous_candidate_id=NULL`：只有 evaluator 同时返回
`MATERIAL_IMPROVEMENT=0` 与 `MATERIAL_IMPROVEMENT_VALID=PASS` 时才可通过；相应的
`PORTFOLIO_TO_PAPER` policy 必须明确要求 `MATERIAL_IMPROVEMENT` 且其 minimum 不大于零。
没有通过该 Assignment 的 Candidate 不能进入 Promotion。

## 20. Multi-Universe Portfolio

支持明确的 `PortfolioUniverseSet`。不同 Universe 的 Alpha 先保持各自 native semantics，再在 Portfolio 层统一。

Cross-universe 需要：

- common base currency；
- calendar/time alignment；
- horizon-aware Alpha normalization；
- cross-universe covariance/factor model；
- tail dependence / drawdown overlap；
- liquidity/capacity aggregation；
- universe-specific cost model；
- currency exposure；
- regime correlation。

Universe 增加或删除是实质变化，必须产生新的 Mandate/Universe Set 版本或新的 Portfolio Candidate；不得自动修改已批准 Candidate。

## 21. Portfolio Candidate

Portfolio Candidate 永久不可变。任何下列变化都产生新 Candidate：

- constituent Alpha Qualification；
- Alpha 增删；
- Portfolio Policy；
- 权重规则；
- Mandate Version；
- Capital Context；
- Risk/Cost/Capacity Model；
- Constraint Set；
- Rebalance policy；
- Candidate Package contract。

V1 `PortfolioCandidateFamily` 是 Core-owned 的稳定比较 lineage：每个
`PortfolioProgram` 恰有一个 Family，按 Program/Mandate 创建或复用，不能由 Candidate UUID、
latest query 或 Agent 指定。每个 Candidate 必须冻结这个非空 Family ID；Material Improvement
只在同一 Family 内比较。

Candidate 至少冻结：

```text
portfolio_candidate_id
candidate_family_id
portfolio_program_id
mandate_version_id
capital_context_version_id
universe_set_version_id
policy_version_id
risk_model_version_id
cost_model_version_id
capacity_model_version_id
constraint_set_version_id
created_at
```

`TargetPortfolioFrame`：

```text
as_of_time
effective_from
effective_until
universe_version_id
instrument_id
target_weight
confidence
portfolio_state
portfolio_candidate_id
```

它表达权重，不表达 BUY/SELL/order type/TIF/limit/stop。

---

# Part VI — Promotion, Approval & Handoff

## 22. Material Improvement Gate 与 Approval Throttling

Research Pool 可以产生大量内部 Candidate，但 Approval Inbox 只接受满足全部条件的唯一推荐：

```text
Promotion Gates passed
+ Candidate Family unique recommendation
+ Material Improvement
+ Evidence maturity
+ 无重复 pending Approval
+ 同 Program 无未处理 Approval
+ downstream compatibility preflight
```

V1 不新增只有一个消费者的 `MaterialImprovementPolicyVersion` 平行表。Material Improvement
是 `PORTFOLIO_TO_PAPER` 的不可变 typed `PromotionPolicyVersion` 中明确的
`MATERIAL_IMPROVEMENT` metric gate，并由 frozen Family/predecessor 的独立 Portfolio evaluator
产出。它综合：

- search-adjusted evidence；
- portfolio marginal contribution；
- independent stability；
- drawdown/tail risk；
- calibration；
- turnover/cost；
- capacity；
- interpretability；
- Alpha Library novelty；
- 对当前已批准 Portfolio 的替代/互补价值。

同 Candidate Family 只维护一个当前内部推荐。新版本没有实质改善时不替换，也不打扰用户。

### 22.1 Promotion policy and deterministic writers

`PromotionPolicyVersion` 是不可变、typed 的 Paper/Live policy：新生产版本固定
`policy_contract_version = PROMOTION_POLICY_V1`，它冻结 purpose、明确的
Paper/Live logical downstream、对应的 `DownstreamConnectionVersion`、
`FeedbackContractVersion` 与具体 `PreflightReceipt`，以及每个必需 metric 的名称、比较符
与有限阈值。Alpha policy 的所有 downstream/connection/contract/receipt refs 均为 NULL；
`PORTFOLIO_TO_PAPER` 只冻结非空的 Paper tuple，Live tuple 为 NULL，且必须是
`MANUAL_APPROVAL` 并含 `MATERIAL_IMPROVEMENT` gate；`PAPER_TO_LIVE` 的 Paper/Live tuple
均非空，才可明确选择 `MANUAL_APPROVAL` 或 `AUTO_HANDOFF`。writer 只复核该固定 receipt
仍 `READY` 且未过期；过期即 stale/ineligible，绝不选择更新的 receipt。自由
`promotion_policy` JSON、任意 Feedback JSON 或全局 readiness bool 不能成为 Promotion
输入；production writer 也不读取 mutable `DownstreamSystem` 的 current/latest 字段。
历史 policy 的 `policy_contract_version` 只可为 NULL，保留只读审计；它不满足任何生产
writer 的 typed eligibility。数据库对 `PROMOTION_POLICY_V1` 强制 purpose tuple XOR，不能把
缺失 tuple 的新 policy 伪装为 legacy。

`PORTFOLIO_TO_PAPER` policy 还必须冻结一个 `paper_to_live_policy_version_id`，它只能指向
一个 `PAPER_TO_LIVE` policy，并且二者的 Paper tuple 必须逐字段相同。Alpha 与
`PAPER_TO_LIVE` policy 的该 self-FK 均为 NULL。P2P writer 将这个已验证的 P2L policy ID
预绑定到其 Paper Approval 和其后唯一的 Handoff；P2L 只沿该 immutable lineage 读取 policy，
仍需复核其冻结 tuples 可用，绝不在 Feedback 或 Promotion 时选择 active/current/latest policy。

系统 `PORTFOLIO_TO_PAPER_PROMOTION` writer 只在冻结 Candidate 有 `PASS` 的
Portfolio Evaluation、所有 typed P2P gates（含 Material Improvement）通过、Package 已
`AVAILABLE`，且该 policy 冻结的 Paper connection/contract/receipt 仍有效时，原子创建一个
带同一 `paper_to_live_policy_version_id` 的 `PromotionEvaluation`、每个 gate result 和冻结的
PENDING Paper Approval。该 ID 必须等于 P2P policy 的已验证 self-FK，并与明确的
`promotion_purpose` 一起纳入 Promotion Evaluation、Approval 与 Handoff 的 frozen lineage；它不推断 current dependencies、
Candidate、Policy 或 downstream。人工动作只 `Approve` / `Reject` 已
绑定 downstream 的 Snapshot；不再在审批时选择或修改 downstream。

完整、contract-valid 的 Paper Feedback 必须先持久化它的有限 typed metric rows，才可为同一
Candidate/Package lineage 排队一个
`PAPER_TO_LIVE_PROMOTION` job。Feedback Contract 的 complete 判定来自 typed scalar
requirements 和 `feedback_contract_metric_requirements`；`spec_json`、summary 或 optional
artifact path 只供展示，绝不参与 gate。该 worker 事务内从持久化的 Candidate/Package/Policy/
Dataset/Capital/connection/receipt、typed Forward Evidence metrics 与冻结的 lineage 重建
deterministic request、持久化每个 gate/action，并以同一 Candidate/成员任一
`DEGRADING|FAILED` Observation 保守阻断；它不作 latest/current 恢复推断。
`MANUAL_APPROVAL` 创建 PENDING Live Approval；`AUTO_HANDOFF` 原子创建审计用
SYSTEM-approved Live Approval 与 AVAILABLE Live Handoff。二者都只是 target-only offer，
仍由独立 downstream claim/accept，绝不控制其运行时。

## 23. Approval Snapshot

类型：

```text
PAPER_HANDOFF_APPROVAL
LIVE_HANDOFF_APPROVAL
```

状态：

```text
PENDING → APPROVED
        → REJECTED
        → STALE
        → EXPIRED
```

终态不可恢复。

Snapshot 冻结：不可变 `candidate_id + candidate_package_id + package_revision`、Evidence
Set、Alpha/Calibration、Mandate、Capital Context、Portfolio Policy、Risk/Cost/Capacity/
Constraint、目标 downstream、downstream connection version、Package/Feedback Contract、
compatibility preflight、validity policy。三项 Package/Candidate 显式身份任一变化都会使
Snapshot stale；这不是内容 hash 比较。

`ApprovalSnapshot.promotion_evaluation_id` 是一对一冻结关系。Approve 请求只携带
`expected_state`；它不能携带、选择、替换或覆盖 downstream、connection、contract、receipt、
Candidate 或 Package。批准时仅重验 Snapshot 自己的显式绑定，随后创建同一绑定的 target-only
Handoff；`AUTO_HANDOFF` 不经过人工 endpoint。

### 23.1 STALE / EXPIRED

`STALE`：已知依赖实质变化，例如 Alpha Qualification 隔离、新 Forward Evidence 推翻结论、Capital Context 超出 capacity envelope、下游连接版本变化、preflight 失效。

`EXPIRED`：达到 policy 定义的最大有效时间。

二者都不能简单刷新时间；必须基于当前事实重新评估并创建新 Snapshot。

### 23.2 Reject

固定 reason code + optional note，例如：

```text
RESEARCH_EVIDENCE_INSUFFICIENT
RISK_PROFILE_UNACCEPTABLE
DRAWDOWN_TOO_HIGH
TURNOVER_TOO_HIGH
CAPACITY_TOO_LOW
COMPLEXITY_TOO_HIGH
INTERPRETABILITY_INSUFFICIENT
MARKET_SCOPE_UNACCEPTABLE
PAPER_EVIDENCE_INSUFFICIENT
LIVE_READINESS_INSUFFICIENT
NOT_ALIGNED_WITH_ORIGINAL_IDEA
OTHER
```

Codex 只能看到 policy 允许的 reason code 和用户 note，不能看到 Level 2 Sealed 明细。

## 24. Target-only Candidate Package

Package 是不可变交付事实，不是下游 runtime 的安装包。其唯一 payload 是
`TargetPortfolioFrame`；当前 builder 的归档最小形状为：

```text
candidate-package/
  manifest.json
  validation/
    target-portfolio-frame.json
```

`TargetPortfolioFrame` 只包含 Candidate/Universe、有效时间和每个 instrument 的目标
权重/置信度。它不包含 executable strategy、wheel、依赖锁、node template、broker URL、
API key、private key、account、订单、成交、仓位、recovery、heartbeat 或 execution
retry。下游自行决定如何解释目标，QZ 不提供启动、停止、撤单、平仓或升级指令。

Package 使用 `candidate_package_id + package_revision` 作为显式身份；Approval 绑定
`candidate_id + candidate_package_id + package_revision`。Candidate 本身是不可变 UUID
事实，不添加无意义的 Candidate revision。替换 Package 必须创建新 Package/revision，
旧 Approval 必须 stale。

业务验证只使用 schema、显式 ID/revision、禁止字段检查和受信 reference-fixture
conformance。不得为 Package 增加 SHA、hash、checksum、digest、fingerprint、内容清单
或任何等价 gate；存储字节完整性由基础设施负责。

当前 target-only archive builder 和其禁止字段验证已实现。把 revised Package 预先持久化、
验证后再绑定 Approval 的事务仍是 §0.1 所列的待验收闭环，不能由现有 archive 生成
行为冒充。

Package worker 先以显式 Candidate ID + Package revision 预留 `BUILDING` row，再写入和
验证 archive；只有验证成功才推进 `AVAILABLE`。lease/retry 只恢复或验证同一预留 row，
不能创建重复 Package、以文件 hash 识别重试，或让 `BUILDING` Package 进入 Approval。

## 25. Handoff Registry

Promotion Policy 在创建 Snapshot 前冻结**逻辑下游系统**，例如 `Nautilus Paper Lab`、
`Nautilus Live Primary`、`External Validator`，以及该系统的 concrete connection、feedback
contract 与 preflight receipt，而不是机器、容器或节点。Approval 页面只展示该绑定；用户只
Approve / Reject，不选择或替换 downstream。

状态：

```text
APPROVED
→ PUBLISHING
→ AVAILABLE
→ CLAIMED
→ DOWNSTREAM_ACCEPTED | DOWNSTREAM_REJECTED
→ FEEDBACK_PENDING
→ FEEDBACK_IN_PROGRESS
→ FEEDBACK_PARTIAL
→ FEEDBACK_COMPLETE
```

异常：

```text
FEEDBACK_STALE
FEEDBACK_INCOMPLETE
FEEDBACK_INVALID
CONSUMER_UNREACHABLE
EXPIRED
REVOKED
```

`DOWNSTREAM_ACCEPTED` 只表示 Consumer 接受 Package Contract，不表示已经运行或交易。

### 25.1 Revocation boundary

未领取：

```text
PUBLISHING → REVOKED
AVAILABLE  → REVOKED
```

`CLAIMED` 后 QZ 不再拥有 revoke/stop/undeploy 权限。只能产生 `WithdrawalAdvisory` / `DegradationAdvisory`，由用户在下游系统自行处置。

## 26. Feedback Contract

每个 Handoff Snapshot 冻结：

```text
feedback_contract_version_id
purpose
minimum_observation_duration
minimum_valid_sample_size
required_metric_codes[] (ordered relational rows)
first_status_deadline
complete_feedback_deadline
grace_period
accepted_package_contracts
accepted_arrow_contracts
disclosure_policy
```

完整 Feedback 用 observation header 加与 frozen `required_metric_codes[]` 精确相同的有限
typed metric rows 表达；`NOT_AVAILABLE` 是显式无值状态，不能伪造数值。自由
evidence JSON、summary 和 optional artifact path 只能展示，不能影响 contract validity 或
Promotion gate。缺失、迟到、部分或 invalid feedback 是运营/证据质量问题，不等于 Candidate
失败。

只有 `FEEDBACK_COMPLETE` 且 contract-valid 的 Paper feedback 才能成为 `ForwardEvidenceEpisode` 并参与 Live Promotion。

---

# Part VII — Degradation & Continuous Learning

## 27. Degradation Monitoring

QZ 只监控研究有效性和 Portfolio Health，不监控订单或执行运行。

Alpha Health：

- predictive decay；
- ranking/directional deterioration；
- calibration drift；
- feature drift；
- regime compatibility；
- validity condition violation。

Portfolio Health：

- mandate behavior drift；
- correlation/tail structure change；
- drawdown overlap；
- cost assumption failure；
- capacity pressure；
- marginal contribution decay；
- concentration change。

状态：

```text
HEALTHY → WATCH → DEGRADING → FAILED → RECOVERED
```

`DegradationPolicyVersion` 判断持续时间、严重程度、统计置信度、多 Episode 一致性、Mandate 影响和是否存在可研究的新信息。

V1 Wake 只接受一个已完成 Handoff 的 `ForwardEvidenceEpisode` 作为来源；Operator 将其
转换为受 schema 约束的 Alpha 或 Portfolio Observation。Core 必须验证该 subject 属于
该 Handoff 的 immutable Candidate，并能唯一映射到一个 Research Program；不完整、
不属于 Candidate、跨多个 Program 或无法映射的证据一律拒绝，不能猜测目标 Program。
同一 Program/subject 的新 Episode 必须具有严格更晚的 `observation_end`；早于或相同
时间的 evaluated Evidence 一律 fail-closed，不能由接收顺序改变状态。

本期只使用 server-owned、显式版本的 `degradation-v1` policy；每个 Observation 固化
完整 policy snapshot、输入 severity/confidence、结果 state 与 reason code。它不是通用
的可配置 policy API；未来改变 policy 必须使用新的显式版本，不能重写既有 Observation。

每个 active-state crossing 以 `(program_id, subject_type, subject_id,
forward_evidence_episode_id, policy_revision, reason_code)` 的结构化唯一约束创建一个
`WakeEvent`，不使用 hash。Program 为 `ACTIVE` 时，Wake 在同一事务创建上述两节点
Diagnostic/Replan Cycle 后才标记为已消费，并由既有 Mission scheduler 排队。有效 Wake
在 `COOLING` 或 `WAITING_FOR_FEEDBACK` 时同一事务转回 `ACTIVE` 再消费；`PAUSED`、
`ARCHIVED`、`BLOCKED` 与 `APPROVAL_PENDING` 只保留 `PENDING` Wake。`Resume` 在
`PAUSED → ACTIVE` 的同一事务中消费 pending Wake；`BLOCKED`/`APPROVAL_PENDING` 只能由
既有低频 Wake 管理动作显式转为 `ACTIVE` 后消费。`ARCHIVED` 不会由 Wake 恢复，且本期
不新增 restore 语义。

Wake 只创建受限 Research work；它不改变 Handoff、Package、Approval 或 Candidate，
不调用下游，也绝不自动停止下游、自动换仓或替换 Live Candidate。

---

# Part VIII — Codex Harness Runtime

## 28. 基本原则

QZ 是业务 Orchestrator；`codex app-server` 是内置 Agent Runtime。

```text
QuaZonai Domain State
        ↓ Mission Contract
Agent Worker
        ↓
codex app-server (stdio)
        ↓ Thread / Turn / Item
Codex
        ↕ mission-scoped stdio MCP
QuaZonai Research Tool Server
```

PostgreSQL 是业务事实源。Codex Thread/Turn/Item 只能作为执行上下文与工程可观测证据。

## 29. App Server 基线

- 使用官方 `codex app-server`；
- 稳定主传输使用 **stdio**；
- 不以 experimental WebSocket 作为生产依赖；
- App Server 版本必须精确固定，并在构建时生成对应 JSON Schema/TypeScript schema 作为协议测试输入；
- CI 必须通过真实 pinned `openai-codex==0.144.4` 的 stdio App Server contract test，至少覆盖带 `model_reasoning_effort` + `service_tier=fast` 与省略这两个 override 的 `thread/start`；
- 初始化连接后使用 `thread/start` / `thread/resume`、`turn/start`、`turn/interrupt` 和 item/turn notifications；
- `runtimeWorkspaceRoots`、project API、environments、dynamicTools 等 experimental 字段不作为 V1 必需依赖；需要时必须先在设计中升级为批准能力。

## 30. Codex Process Model

每个 RUNNING Mission 使用独立 Codex App Server child process：

- 共享只读/受控 `CODEX_HOME` thread persistence volume；官方 ChatGPT 认证不再以该目录为事实源；
- 一个 Mission 对应一个 durable Codex Thread；
- Mission crash 后可由新 child `thread/resume`；
- child 退出即释放 shell、MCP 和文件句柄；
- 不在一个无限长 Thread 中承载整个 Research Program。

这提供清晰的权限、workspace、tool 和失败边界。

### 30.1 Runtime Configuration ownership

Codex provider 配置、Codex thread controls 与 Worker limits 属于**运行时管理配置**，由本地 Administrator 在 Web Administration 中维护，并持久化到 PostgreSQL 单例 `runtime_configurations`。它们不是 Compose/bootstrap 环境变量。

`.env` / process environment 只负责启动级基础设施：

```text
QUAZONAI_ENV
PostgreSQL database/user/password + DATABASE_URL/ALEMBIC_URL
QUAZONAI_MASTER_KEY
QUAZONAI_AUTH_ENABLED
QUAZONAI_AUTH_TOTP_SECRET                    # optional legacy binding importer only
QUAZONAI_AUTH_COOKIE_KEY / QUAZONAI_API_TOKEN / QUAZONAI_AUTH_PUBLIC_ORIGIN
plugin/package/mission storage roots
HTTP port
fixed CODEX_HOME / frontend dist deployment paths
```

Runtime Configuration 至少包含：

```text
revision
codex_model nullable
codex_reasoning_effort nullable       # null | minimal | low | medium | high | xhigh
codex_fast_mode boolean               # default false; maps to service_tier=fast
codex_use_default_model_settings boolean  # new singleton default true; migrated rows false
codex_base_url nullable
codex_api_key encrypted/write-only nullable
max_plugin_wheel_bytes
plugin_validation_timeout_seconds
bundle_build_timeout_seconds
plugin_job_timeout_seconds
mission_job_timeout_seconds
job_poll_seconds
job_lease_seconds
```

Codex provider 规则：

- `codex_model` 为空时使用 Codex 默认模型选择；
- `codex_use_default_model_settings=true` 时，新 Mission 的 effective runtime 会屏蔽已保存的 `codex_model`、`codex_reasoning_effort` 与 `codex_fast_mode`，从而让 Codex/当前模型选择自身默认值；这些 QuaZonai override 原值保留在 Runtime Configuration 中，切回 `false` 后恢复，不因切换而清空；
- `codex_use_default_model_settings=false` 时，按保存的 `codex_model`、`codex_reasoning_effort` 与 `codex_fast_mode` 启动新 Mission；provider/Base URL/API key routing 独立于该模型默认选择开关；
- 新建 singleton 默认 `codex_use_default_model_settings=true`；迁移既有行写入 `false` 以保持既有显式模型控制。兼容旧客户端的首次保存若显式给出任一模型控制，也推断为 override mode，避免旧请求被静默忽略；
- `codex_reasoning_effort` 为 `null` 时不向 `thread/start` 发送 `model_reasoning_effort`；显式值原样发送。它是公开运行元数据，不是隐藏思维链；不支持的模型/provider 值必须显式失败，不能自动降档；
- `codex_fast_mode=false` 不发送 Fast service-tier override；为 `true` 时向新 Mission 的 `thread/start` 发送原生 `service_tier="fast"`。Fast 与 reasoning effort 正交，provider 拒绝时沿现有失败链路处理，不自动切回 Standard；
- Runtime Configuration 的 reasoning/Fast 值优先级为：未来 AgentProfileVersion 或 Mission 显式值 > 系统级 Runtime Configuration 默认值 > Codex/模型默认值；本期只实现系统级默认值；
- `codex_base_url` 支持自定义 OpenAI-compatible API root，必须是绝对 HTTP(S) URL；
- Base URL 不允许内嵌 username/password、query token 或 fragment；
- 配置 custom Base URL 或 API key 时，App Server 使用显式 model provider，V1 wire API 固定为 Responses；
- provider API key 不进入 App Server environment、命令行或 `--config`。受信任 Mission runner 只在内存中持有解密后的 key，通过 `0700` 临时目录下的 `0600` one-shot Unix socket broker 向 Codex 0.144.4 的 command-backed model-provider `auth` helper 交付一次 token；helper 在首个 provider request 前取用后 broker 关闭，Mission shell、MCP Tool Server、Agent output 与持久 event 均不得获得该 key；
- App Server environment 必须显式清除 provider API key、`QUAZONAI_MASTER_KEY` 与数据库连接 secret，不能依赖普通 shell env filtering 作为 Secret 边界；
- 已保存 provider key 时修改 `codex_base_url`，必须在同一 mutation 中重新输入 key 或显式清除旧 key，禁止把旧 credential 静默重绑定到新 endpoint；
- 未配置 custom provider credential 时，官方 ChatGPT 认证来自 PostgreSQL 中的 `CodexChatgptAuthConfiguration`，并在 Mission child 内以 App Server external auth 的短期内存 token 使用；
- Web/API 只返回 `codex_api_key_configured` 状态，不回读 plaintext/ciphertext/nonce。

### 30.2 ChatGPT Auth ownership

ChatGPT OAuth 是独立于 Runtime Configuration 的认证域。`codex_chatgpt_auth_configurations` 是唯一长期事实源；`access_token` 与 `refresh_token` 均使用既有 `QUAZONAI_MASTER_KEY` 和按认证 UUID/字段/key version 绑定的 AES-GCM AAD 加密保存。OAuth refresh/login/disconnect 不推进 Runtime Configuration revision。

首版只实现 OpenAI Device Code OAuth：Backend 固定官方 issuer、client id、verification URL 和 token endpoints；浏览器只短暂持有 `login_id`、`user_code` 与过期时间，永不收到 access/refresh/id token。Device login attempt 也持久化在 PostgreSQL，服务端限制 poll cadence（收到 `slow_down` 时持久化增加间隔并推进下一次允许轮询时间），并用覆盖一次完整上游 device poll/token exchange 的持久 in-flight lease 串行化同一 attempt 的 exchange；poll 结束时只能清除自己取得且仍匹配的 lease，不能清除超时后由另一 poll 取得的新 lease。start/disconnect 通过 PostgreSQL singleton operation-lock 行序列化，poll/cancel 则通过 login-attempt 行锁、poll lease 和 late-poll 检查保证竞态安全；start 在等待 pending attempt 行锁后必须重新读取 canonical auth，避免成功 poll 已提交后仍创建第二个 login，且 start/disconnect 在没有 pending 行时也必须先争用 singleton lock。`device/start` 是状态变更请求，即使 Operator Authentication 关闭也必须使用服务端要求的非 safelisted `application/json` 请求体，不能被跨站 HTML form 或 `no-cors` 请求触发；重复 start 不得重复写入持久化事件，重复 cancel/disconnect 也不得重复写入状态事件；CONNECTED 事件必须与凭据安装在同一事务内提交。终态 poll 的失败/过期错误码必须保留到 Web UI 的现有错误面板，不能静默关闭。

有限 worker 在 claim queued Mission 之前必须执行一次与 API 相同的 legacy import；因此 custom-provider Mission 也不会绕过该一次性 import/cleanup。Mission Runner 在启动官方 App Server 前仍会再次执行幂等检查（若 `auth.json` 是完整 ChatGPT token shape，则先提交 canonical DB row，再清理文件；多个 initializer 同时清理时，文件已被另一方移除视为成功），然后从 DB 解密或串行刷新 access token，再通过 pinned App Server 的 `account/login/start` `chatgptAuthTokens` 注入内存认证；401 的 `account/chatgptAuthTokens/refresh` server request 只由受信 parent 处理，返回 App Server 要求的 camelCase wire fields，并绑定 Mission 启动时的 canonical auth UUID 与 ChatGPT account，canonical row 被 Disconnect/re-auth 替换后不得把新账号 token 交给旧 Mission。重新认证或发现 CONNECTED row 的 access/refresh 密文损坏时必须创建新的 canonical auth UUID；公开 auth status/readiness 必须报告 reauthentication required，不能把损坏 row 当作仍然 Connected。readiness 在报告 ready 前必须验证 access/refresh 密文可用；custom provider readiness 也必须先解密已保存的 API key，完整、部分或损坏的密文均不得报告 ready；对应的 custom-provider reauth 状态在 `/system/health` 中必须是 degraded。refresh token 永不进入 Mission shell、环境变量、worktree、prompt、事件或 Codex config；App Server external-auth 路径不得创建 `auth.json`。runtime configuration 含 provider API key 时，校验错误响应不得回显请求 input。custom provider/API key 与 ChatGPT route 是互斥的显式路由，禁止隐式 failover。

`CODEX_HOME/auth.json` 只允许作为一次性 legacy import source：没有 canonical DB row 时，仅导入当前 pinned Codex 的完整 ChatGPT auth shape，成功提交 DB 后删除文件；数据库写入或清理失败均 fail closed。已有 DB row 时 DB 胜出并只尝试清理文件，文件不能覆盖 DB。管理员 Disconnect 必须先成功移除 legacy 文件，再提交 canonical DB 凭据删除；文件清理失败时数据库状态保持不变，防止旧凭据在重启时复活。`codex_login_configured`、readiness 和 Mission admission 只读取 DB 状态，不以文件存在作为登录证明。

Runtime Configuration mutation 规则：

- GET 返回当前单调递增 `revision`；尚未创建 singleton 时为 revision `0`；
- PUT 必须携带 `expected_revision`，陈旧保存返回 `RUNTIME_CONFIGURATION_STALE`，首次并发创建的唯一约束竞争也必须被翻译为同一业务冲突而不是数据库 500；
- PUT 支持 `Idempotency-Key`；同一个逻辑请求重试返回原响应，不重复加密 provider key、不重复推进 revision、也不重复写 `RUNTIME_CONFIGURATION_UPDATED` event；
- 新字段支持兼容旧客户端的三态更新：字段省略保持当前值，`codex_reasoning_effort: null` 恢复模型默认，`codex_fast_mode: false` 只有在字段显式出现时才关闭 Fast；第一方 Web 客户端始终显式发送二者；
- `RUNTIME_CONFIGURATION_UPDATED` 只记录非敏感的 requested reasoning/Fast 配置及 action，不记录隐藏 reasoning、token、API key 或其他 Secret；
- Idempotency receipt 不保存 provider key plaintext，也不为了去重额外保存历史 secret 副本。

Worker 规则：

- finite worker 每次领取后续 job 前读取最新 Runtime Configuration；
- plugin validator/bundle child 与 Research Mission child 在启动时冻结当次有效配置；
- `job_poll_seconds` 服务端与数据库下界为 `0.01` 秒，禁止近零 busy loop；
- Administration 保存后不要求重建或重启 Compose stack；
- 已运行 child 的 timeout/model/provider/reasoning/service tier 不被中途改写，修改只影响之后领取/启动的工作；每个新 Mission 冻结启动时的有效配置。

Runtime Configuration 的 API key 使用 `QUAZONAI_MASTER_KEY` 做 AES-256-GCM authenticated encryption。Master key 仍必须外部注入，不能迁入数据库或 Web 配置。

## 31. Workspace Model

每个 Research Program：一个 QZ 管理的 private bare Git repo。

每个 Research Branch：一条持久 Git branch。

每个 Mission：一个临时独占 worktree。

```text
Program bare repo
  ├─ Branch A
  │   ├─ Mission A1 worktree
  │   └─ Mission A2 worktree
  └─ Branch B
      └─ Mission B1 worktree
```

QZ 拥有 branch lease、worktree create/remove、accept changes、commit 和 `workspace_revision_no`。Codex 只写普通文件，不执行 branch/commit/merge/rebase/worktree 管理。

Git history 是开发辅助，不是业务 identity 或 Approval Gate。

## 32. Sandbox 与权限

默认 Mission：

```text
workspace-write
network disabled
approvalPolicy = never
cwd = mission worktree
workspace roots = mission worktree only
```

禁止通过 Codex interactive approval 向用户请求额外 Shell/网络权限。需要数据、实验或受控外部访问时必须调用 QZ Research Tool Server。

Mission 不允许访问：

- QZ source repo；
- Sealed dataset root；
- provider credential store；
- downstream secrets；
-其他 Program worktree；
- Docker socket；
- PostgreSQL credential。

## 33. Agent Profiles

角色是 Mission execution profile，不是独立业务 Agent 身份：

```text
RESEARCH_DIRECTOR
DATA_RESEARCHER
ALPHA_RESEARCHER
VALIDATOR
PORTFOLIO_ARCHITECT
REVIEWER
DEGRADATION_ANALYST
```

`AgentProfileVersion` 固定 model preference、reasoning effort、developer instructions、tool capability set、workspace rules 和 output contract。

RESEARCH_DIRECTOR 可以提出 Mission Graph artifact，但不能直接变更业务状态；REVIEWER 不能 approve Candidate；任何 profile 都不能访问 Sealed raw data 或执行系统。

## 34. Mission-scoped Research Tool Server

V1 使用稳定的 **stdio MCP server**，而不是 experimental dynamicTools 作为核心依赖。

Tool 按 MissionContract 能力过滤，例如：

```text
dataset.list
dataset.describe
dataset.query_sample
experiment.submit
experiment.status
artifact.register
alpha.submit_candidate
calibration.submit_candidate
portfolio.inspect_library
portfolio.submit_candidate
evidence.read_allowed
mission.report_result
```

Tool Server：

- 通过 mission_id 加载不可变 Mission Contract；
- 每次调用重新校验 Mission state、capability、resource scope；
- 不把 DB credential 或 provider secret 暴露给 Codex；
- 大型数据只返回引用、schema、summary 或受限 sample；
- mutation 使用显式 idempotency key 和 expected business revision；
- Agent 不能调用 Approval、Handoff publication、Secret、Plugin activation 或 Admin mutation。

## 35. Codex Event Projection

App Server event 被投影为 `agent_activity_events`：

```text
thread_started
turn_started
turn_completed
item_started
item_completed
command_started/completed
file_change
mcp_tool_call
plan_update
turn_diff
runtime_error
```

保存结构化 metadata、摘要、路径和 exit status。模型 reasoning item 的隐藏内容不保存；UI 只展示允许的可验证活动。

Codex 声称“candidate ready”不能推进业务状态，必须先通过 output schema、artifact validation、Domain Validator 和对应 Gate。

---

# Part IX — Runtime Plugins

## 36. 插件范围

保留“运行时动态插拔”，但插件只允许扩展 QZ 研究/交付能力：

```text
DATA_CONNECTOR
DATA_TRANSFORM_ADAPTER
RESEARCH_ADAPTER          # optional, non-canonical
HANDOFF_CONNECTOR
```

禁止 execution/broker/order adapter capability。

### 36.1 Release model

PRIMARY wheel + optional dependency wheels；唯一 `quazonai.plugins` entry point；只接受 wheel，不接受 sdist/editable/Git URL/远端动态安装。

Release 状态：

```text
RECEIVED → INSTALLING → VALIDATING → STAGED → ACTIVE
                         └→ FAILED
ACTIVE → DRAINING → INACTIVE → REMOVING → REMOVED
```

新版本 side-by-side；已有 Data Source / Downstream Connection 固定 release，不静默漂移。

### 36.2 Process isolation

插件只在 validator 或具体 connector runner child 中 import。API、worker、agent-worker、evaluator 长进程不 import 第三方插件。

动态卸载依赖子进程退出；不使用 `importlib.reload()`、`sys.modules` 修改或原地热补丁。

每个 release 使用隔离 immutable venv/runtime directory；完成后不原地 install/uninstall。版本变化创建新 release/runtime。

---

# Part X — Backend, Database & API

## 37. 运行拓扑

QuaZonai Core production Compose 最小服务：

```text
postgres
migrate
api                 # FastAPI + built React static assets + SSE
finite-worker       # Mission + remote Discovery/Sealed durable jobs
```

外部独立系统（不属于 QZ Compose 或控制面）：

```text
trusted research/sealed evaluator (when configured)
Paper downstream
Live downstream
```

如配置受信 evaluator，QZ 只通过窄的受控接口交付受限输入并接收受控结果。Paper/Live
downstream 的部署、节点生命周期、凭据、网络和恢复均不在 QZ 事实源或 Compose 拓扑中。
仓库中的旧 remote-runtime 部署材料属于迁移删除范围，不能作为 QZ 对下游运行的承诺。

不引入 Redis、Celery、Kafka 或 Kubernetes。使用 PostgreSQL durable jobs + `FOR UPDATE SKIP LOCKED`，事件表 + `LISTEN/NOTIFY` 仅做唤醒。

`QUAZONAI_ENV` 只能为 `development`、`test` 或 `production`（忽略大小写与首尾空白）；未知值在启动时拒绝，不能绕过 production 的安全策略。`api` 默认只发布宿主 `127.0.0.1:8000`。远程访问由操作者自己的受信 TLS/reverse-proxy/tunnel 层处理；V1 不建设多用户业务认证或 RBAC。`QUAZONAI_AUTH_ENABLED=true` 时 Web/operator API 使用单用户 Operator Authentication 边界；为 `false` 时保留 direct access，操作者必须保持 loopback-only 或提供另一个明确可信的访问边界。

### 37.1 Operator Authentication

> 安全语义：TOTP-only 不再是 2FA/MFA，抗在线暴力破解能力弱于密码 + TOTP。公网暴露时必须继续使用 HTTPS、窄化可信代理配置，并优先叠加部署侧网络访问控制。认证因子迁移会升级 Cookie version，因此旧 session/trusted-browser cookie fail closed，升级后需要重新输入一次 TOTP；升级必须先运行 Alembic migration `0010_operator_auth_configuration`，已有 TOTP binding 不变时无需重新绑定验证器。

Operator Authentication 是部署/访问边界，不是新的业务用户、tenant 或 RBAC Domain。V1 只有一个固定 Operator，身份 subject 为 `local-operator`，不得由客户端或环境变量覆盖。浏览器登录是 TOTP-only 单因素认证；Machine API Token 是独立的自动化凭据，不是浏览器登录因子。

旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。Compose 只把旧变量的非空状态转换为内部 names-only presence marker，绝不把旧值注入 API container；API 将 marker 映射回对应旧变量名并执行同一 fail-closed 错误。认证关闭时仍保持 direct access，旧变量与 presence marker 都视为 dormant process environment。

`.env` / process environment 配置：

```text
QUAZONAI_AUTH_ENABLED
QUAZONAI_AUTH_TOTP_SECRET                    # optional legacy binding importer only
QUAZONAI_AUTH_COOKIE_KEY
QUAZONAI_API_TOKEN
QUAZONAI_AUTH_PUBLIC_ORIGIN
QUAZONAI_AUTH_SESSION_TTL_SECONDS          # optional, bounded default
QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS     # optional, bounded default
QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS          # optional direct reverse-proxy CIDRs
```

认证状态的 canonical secret 是 PostgreSQL 单例 `operator_auth_configurations` 中 scope=`SYSTEM` 的 `OperatorAuthConfiguration` binding；独立的 `operator_auth_initializations` SYSTEM marker 记录该安装曾经完成过 binding。该 binding 以 UUID、AES-GCM ciphertext/nonce、key version 和 `bound_at` 保存当前 TOTP secret；plaintext、二维码 URI 与 setup candidate 不进入数据库。没有 binding 且没有 marker 的健康新库是 `SETUP_REQUIRED`，存在且可用是 `BOUND`，marker 存在但 binding 缺失是配置损坏并必须 fail closed，`AUTH_ENABLED=false` 是 `DIRECT_ACCESS`。binding 不会因 cookie/session 到期、重启、cookie key 轮换、错误动态码、设备丢失、master key 错误或数据库故障而回到 setup。

认证启用必须提供 `QUAZONAI_MASTER_KEY`、独立的 `QUAZONAI_AUTH_COOKIE_KEY`、`QUAZONAI_API_TOKEN`、`QUAZONAI_AUTH_PUBLIC_ORIGIN` 与合法 TTL。`QUAZONAI_AUTH_TOTP_SECRET` 仅是兼容旧部署的一次性 legacy importer：数据库无 binding 且无 initialized marker 时，在启动事务内校验、用既有 `crypto.encrypt_bound_secret/decrypt_bound_secret` 加密并原子创建 binding 与 marker；数据库已有 binding 时只用恒定时间比较检查一致性，冲突 fail closed；marker 存在但 binding 缺失时永不重新导入或开放 setup；正常运行永不把环境值当 canonical fallback。启动初始化发生在 session factory 建立后且请求前；数据库不可用、表损坏、ciphertext/AAD/master key 无法解密都不能启动 setup 流程。

首次 binding 使用同源 Web setup：`GET /api/v1/auth/bootstrap` 只返回 `{auth_enabled, setup_required}`；`POST /api/v1/auth/setup/start` 在无 binding 时生成至少 160-bit 的 PyOTP secret，以标准 `otpauth://totp/` URI 返回并把 setup id/secret 放入不超过 10 分钟、`HttpOnly`、`SameSite=Strict`、按 HTTPS 加 `Secure` 的 AEAD setup cookie，不创建 pending DB row；`POST /api/v1/auth/setup/confirm` 只从该 cookie 取 candidate，按现有 limiter/backoff 与 ±1 TOTP window 校验，再以 scope unique constraint 的事务 INSERT 实现 first-claim-wins。成功者复用 browser session/trusted-browser issuance；并发失败者返回 `409 / AUTH_SETUP_ALREADY_COMPLETED`、清除 setup cookie 并重新 bootstrap。setup 响应统一 `Cache-Control: no-store`/`Pragma: no-cache`；secret、二维码 URI 和动态码不进 URL、storage、日志、事件、错误或第三方服务。首次 claim 必须在可信私网/VPN/SSH tunnel/受保护 proxy 后完成，再公开实例。

Setup UI 必须以 `checking | setup | anonymous | authenticated` 明确分流，使用本地 QR renderer、manual key、六位 code、Trust this browser 与过期后重新生成；候选只存在 React 内存，不写 Web Storage。UI 覆盖 English、简体中文、繁體中文、日本語、한국어、Español、العربية，并在 Arabic 使用 RTL。认证入口仍只允许 browser/Machine/Mobile 各自既有 transport；CLI 不执行 setup，不读取 browser TOTP。

规则：

- `QUAZONAI_AUTH_ENABLED=false` 时在所有环境保留 direct Web/operator API access，不显示登录门，其他 auth credential/TTL/proxy identity 配置均视为 dormant 并忽略；该模式只适合 loopback-only 或另有明确可信访问边界的部署；设为 `true` 时 master key、独立 32-byte cookie encryption key、machine API token、public origin 与 bounded TTL 必须全部格式合法，否则启动 fail closed；首次 TOTP binding 由上述 durable setup 流程完成，legacy env 只用于一次性导入；直接注入的 `Settings` 也必须执行同一 TTL 类型/范围验证；启用认证的 production public origin 必须使用 HTTPS；
- 正常浏览器登录只要求 `TOTP`。TOTP 使用 RFC 6238 兼容 Google Authenticator 的标准 30 秒、6 位配置；允许有限 clock-skew window，不自研 OTP/HMAC 协议；
- `POST /api/v1/auth/login` 请求体只允许 `totp_code` 与可选 `trust_browser`（默认 `false`）；`totp_code` 必须最终是恰好 6 个 ASCII 数字，Schema `extra=forbid`，旧 username/password 或任意其他字段不得被静默接受；缺失、格式错误、错误、重放或被限流统一返回 `401 / AUTH_INVALID` 与通用失败文案，不回显请求体或动态码；
- TOTP binding plaintext、setup candidate、二维码 URI 与动态码不进入正常 API response/storage/event/log/URL；cookie key 与 API token 是启动级 secret。仅 setup start 在无 binding 时短暂返回本地 enrollment material，所有 setup response 必须 `no-store`；
- `QUAZONAI_AUTH_COOKIE_KEY` 必须与 `QUAZONAI_MASTER_KEY` 使用不同的随机 32-byte key material；两者解码结果相同即启动失败，不能用用途不同代替密钥分离；
- `QUAZONAI_API_TOKEN` 必须可直接序列化为 RFC 6750 Bearer `b64token`：长度 32–4096，只允许 ASCII 字母、数字、`-._~+/` 与末尾可选 `=`；空白、CR/LF、控制字符、非 ASCII 或其他字符必须在启动时拒绝；
- 成功登录签发短期 browser session cookie。勾选 **Trust this browser** 时另外签发长期 trusted-browser cookie；两者都使用独立 `QUAZONAI_AUTH_COOKIE_KEY` 做 AES-256-GCM authenticated encryption，Cookie 必须 `HttpOnly`、`SameSite=Strict`，启用认证的 production 必须自动标记 `Secure`，不能把 bearer credential 放入 `localStorage`/`sessionStorage`；每个 credential 还必须绑定签发时的进程内 authenticated-logout generation、当前 API runtime 新生成的随机 process issuance epoch 与当前浏览器 profile 的 sealed local epoch，读取时同时核验三者；process epoch 在每次 API runtime 创建时变化，因此 restart 必须 fail closed 并使所有既有 browser credential 失效，不能让复位 generation 重新接受已退出 cookie；读取每一种 credential 时必须扫描同名 Cookie 的全部 raw 值并只认可该 credential kind 的有效 AEAD token，因此 sibling-domain 注入的重复 cookie 不能以顺序遮蔽 host-only credential；
- trusted-browser cookie 是长期设备凭证：有效时可无需再次输入 TOTP 即可为该浏览器恢复登录；它只存在于浏览器 cookie jar，不形成数据库“用户设备”业务模型；
- logout 默认同时删除 session 和 trusted-browser cookie，并在当前 host/browser profile 写入一个 `HttpOnly`、`SameSite=Strict`、host-only、由 `QUAZONAI_AUTH_COOKIE_KEY` AEAD 验证的 browser-local logout barrier 与独立 sealed local epoch，至少持续所有可能 trusted-browser credential 的最长有效期；读取时必须扫描同名 Cookie 的全部值并只认可有效值，因此 sibling-domain 注入的同名值既不能覆盖有效 barrier/epoch，也不能在成功登录清除 host-only barrier 后锁死浏览器。成功 TOTP 登录只清除 barrier、保留 local epoch 并把新 credential 绑定到它；因此 logout response 先到、旧 login/automatic-renewal response 后到时，即使旧 response 清除了新 barrier，其旧 credential 也不能重新验证。已认证 browser logout 还推进进程内 global issuance generation 并停止已打开 stream；credentialless/public logout 只能改变请求者 browser-local barrier/epoch，不能推进该 global generation 或阻塞其他 browser 的登录/续期。所有 browser credential 也必须包含每个 API runtime 唯一的随机 process issuance epoch，API restart 必须创建新 epoch 并使 pre-restart credential 立即不可验证，即使 generation 重置且 cookie key/local epoch 未变。自动续期永不清除 barrier；在途 `/auth/session` trusted-browser probe 若输给 authenticated logout revocation 必须返回 authentication failure，不能报告一个成功 session view；cookie key 轮换必须使全部既有 session/trusted-browser credential 立即不可验证，从而提供全局 revoke；自然到期后也必须重新执行 TOTP；
- `QUAZONAI_AUTH_PUBLIC_ORIGIN` 必须解析并保存为 canonical browser origin：scheme/host 小写，Unicode hostname 使用浏览器兼容 UTS-46/IDNA 规则转为小写 IDNA ASCII，IPv6 使用压缩后的 bracketed literal，HTTP `:80` / HTTPS `:443` 默认端口省略，非默认端口保留；credential、非根 path、params、query、fragment、非法 host 或非法 port 必须拒绝；非 IP hostname 若按 WHATWG 的 ends-in-a-number 规则会进入 IPv4 parser（例如 `example.127` 或 `example.0x`），也必须在启动时拒绝；
- browser cookie 认证的 unsafe request 必须把请求 `Origin` 用同一 canonicalizer 解析后再与配置 origin 做恒定时间精确比较；等价浏览器序列化（例如 `https://EXAMPLE.com:443` 与 `https://example.com`）必须匹配，不同 scheme/host/effective port 必须拒绝；启用认证的 production origin 必须使用 HTTPS；`SameSite=Strict` 不是唯一 CSRF 控制；
- TLS reverse-proxy/tunnel 位于 API 前时，login limiter 只有在 ASGI direct peer 命中可选 `QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS` 中的精确 IP/CIDR 时才读取一个规范化 `X-Forwarded-For`；必须从右向左剥离已信任 proxy hop，再使用最近的 untrusted literal IP。未配置、peer 不匹配、重复/缺失/非法 header 或 header 中只有 trusted hop 时一律回退 direct peer，绝不盲信任 client 提供的 header。proxy 必须 append 自己观测到的 peer（或 overwrite 为经验证的 client IP），不能原样转发入站 header；禁止 `/0` 或宽泛网络。应用负责这项解析，Compose 和手工 Uvicorn 均必须显式使用 `--no-proxy-headers`，并且不能设置 `FORWARDED_ALLOW_IPS` 或传入 `--proxy-headers`，否则它会在应用核验前改写 direct peer；
- browser cookie 认证的受保护 API response 必须禁止 shared-cache storage：默认返回 `Cache-Control: private, no-store` 与 `Vary: Cookie`；已经提供 `no-store` 的 transport-specific response（包括 SSE）保留其原有 cache headers。该 browser-only policy 不改变 public health 或 machine Bearer traffic；
- FastAPI 提供的 Web workbench document 及其 SPA deep-link fallback 必须同时返回 `Content-Security-Policy: frame-ancestors 'none'` 与 `X-Frame-Options: DENY`，拒绝任何 parent frame；此控制独立于 cookie `SameSite`/Origin 检查，防止同站或受损 origin 利用已认证浏览器进行 clickjacking；
- `/api/v1/system/health` 保持 public 供容器/orchestrator healthcheck；`/api/v1/auth/bootstrap`、`/api/v1/auth/setup/start`、`/api/v1/auth/setup/confirm`、`/api/v1/auth/login` 与 session 属于同源认证入口；Operator Authentication 启用时，其余 Operator API 要求有效 browser credential 或 `Authorization: Bearer <QUAZONAI_API_TOKEN>`；关闭时保留 direct access；
- CLI/自动化只使用独立 machine API token，不读取 browser cookie/TOTP；browser login 不把 API token 下发给前端；
- downstream-owned Handoff `claim/accept/reject/package/feedback` 保持现有 per-downstream service credential，只授权对应 Handoff/Feedback，不接受 Operator trusted-browser credential 代替下游身份；
- 认证失败返回统一错误 envelope；缺失、格式错误、错误、重放或被限流的 TOTP 均返回 `401 / AUTH_INVALID`，不回显请求体或动态码；登录验证使用有界、进程内、按观测来源的短退避：credential verification 最少间隔 1 秒，连续失败指数退避但最大 30 秒，成功即清除状态，受限请求仍返回同一通用认证失败，不建立持久账户锁定；
- `/api/v1/events/stream` 不是一次认证后永久有效：每轮 polling 必须按当前 settings 重新验证 session/trusted-browser cookie 或 machine token；session 到期、cookie key/token 轮换立即终止流，成功 logout 推进进程内 stream generation，使已打开的 SSE 在下一轮停止并由浏览器按当前 cookie 状态重新连接；
- Operator TOTP/cookie key/machine token/public origin 不得继承到 Codex App Server 或 Mission-owned child environment；
- 不新增应用级 hash/checksum/fingerprint 身份或完整性 Gate。Cookie 使用标准 authenticated encryption，TOTP 使用标准库实现。

## 38. 技术栈

Backend：

- Python 3.14.x；
- FastAPI / Pydantic v2；
- SQLAlchemy 2 / Alembic；
- psycopg 3；
- PostgreSQL 18；
- PyOTP（RFC 6238 TOTP）；
- Polars；
- PyArrow；
- Optuna；
- CVXPY；
- official MCP Python SDK；
- `uv` 精确锁定。

Frontend：

- React 19；
- TypeScript；
- Vite；
- React Router；
- TanStack Query；
- ECharts；
- CSS variables + lightweight component primitives。

Production 构建后 SPA 静态资产由 FastAPI 提供，减少额外运行服务。

## 39. PostgreSQL schema

### 39.1 通用规则

- PK：UUID；
- 时间：UTC `timestamptz`；
- 金额/价格/权重：`numeric`，不使用 float 保存业务值；
- 状态：`text + CHECK`；
- JSONB 只存开放结构 snapshot，不替代核心关系；
- 不软删正式研究事实；Draft 可物理删除；
- 大型 Arrow/Parquet/wheel/package 存持久卷，DB 存 artifact row；
- 不建立应用级 content hash 字段。

### 39.2 Core orchestration

| 表 | 关键字段 |
|---|---|
| `research_charters` | `id`, `original_idea`, `research_question`, `prediction_horizon`, `scope_json`, `created_at` |
| `research_programs` | `id`, `charter_id`, `state`, `cooling_reason`, `blocked_reason`, `created_at`, `updated_at` |
| `idea_contributions` | `id`, `program_id`, `idea_text`, `relationship`, `created_at` |
| `program_relationships` | `id`, `from_program_id`, `to_program_id`, `type`, `created_at` |
| `research_branches` | `id`, `program_id`, `parent_branch_id`, `hypothesis`, `derivation_type`, `changed_assumptions`, `state`, `revision_no` |
| `research_missions` | `id`, `branch_id`, `type`, `role_profile_version_id`, `state`, `objective`, `contract_json`, `workspace_revision_no`, `attempt`, `started_at`, `finished_at`, `error_code` |
| `mission_dependencies` | `mission_id`, `depends_on_mission_id` |
| `agent_sessions` | `id`, `mission_id`, `codex_thread_id`, `codex_version`, `model`, `started_at`, `last_event_at`, `state` |
| `agent_activity_events` | `id bigint`, `mission_id`, `kind`, `payload`, `created_at` |
| `jobs` | `id`, `kind`, `resource_type`, `resource_id`, `state`, `payload`, `attempt`, `available_at`, `lease_owner`, `lease_expires_at`, `last_error` |
| `events` | `id bigint identity`, `kind`, `aggregate_type`, `aggregate_id`, `payload`, `created_at` |

### 39.3 Data

| 表 | 关键字段 |
|---|---|
| `market_universe_versions` | `id`, `universe_key`, `version_no`, `name`, `spec_json`, `state`, `created_at` |
| `data_sources` | `id`, `plugin_release_id`, `name`, `provider`, `config`, `state` |
| `dataset_revisions` | `id`, `data_source_id`, `universe_version_id`, `revision_no`, `schema_version`, `event_start`, `event_end`, `available_start`, `available_end`, `relative_path`, `row_count`, `quality_state`, `point_in_time_state`, `created_at` |
| `data_quality_results` | `id`, `dataset_revision_id`, `check_kind`, `state`, `summary`, `created_at` |
| `evaluation_dataset_selections` | `id`, `universe_version_id`, `version_no`, `discovery_dataset_revision_id`, `validation_dataset_revision_id`, `sealed_dataset_revision_id`, `state`, `created_at` |

### 39.4 Research assets & evidence

| 表 | 关键字段 |
|---|---|
| `feature_pipeline_versions` | `id`, `program_id`, `branch_id`, `version_no`, `artifact_id`, `contract_json`, `created_at` |
| `alpha_model_versions` | `id`, `program_id`, `branch_id`, `version_no`, `mode`, `artifact_id`, `horizon`, `created_at` |
| `alpha_calibration_versions` | `id`, `alpha_model_version_id`, `version_no`, `method`, `training_dataset_revision_id`, `source_discovery_evaluation_id`, `private_artifact_ref`, `state`, `created_at` |
| `alpha_qualifications` | `id`, `alpha_model_version_id`, `calibration_version_id`, `universe_version_id`, `role`, `state`, `scope_json`, `evaluation_episode_id`, `created_at` |
| `evaluation_design_versions` | `id`, `version_no`, `universe_version_id`, `contract_version`, typed statistical and disclosure fields, `state`, `created_at` |
| `alpha_discovery_evaluations` | `id`, `source_mission_artifact_id`, `alpha_model_version_id`, `program_id`, `cycle_id`, `branch_id`, `mission_id`, `evaluation_dataset_selection_id`, `discovery_dataset_revision_id`, `evaluation_design_version_id`, `cause_event_id`, `evaluator_contract_version`, `state`, `outcome_code`, `created_at`, `completed_at` |
| `alpha_discovery_evaluation_metrics` | `discovery_evaluation_id`, `metric_code`, `value`, `status` |
| `alpha_discovery_evaluation_gates` | `discovery_evaluation_id`, `gate_code`, `status`, `reason_code` |
| `alpha_evaluation_assignments` | `id`, `source_mission_artifact_id`, `discovery_evaluation_id`, `program_id`, `cycle_id`, `branch_id`, `mission_id`, `alpha_model_version_id`, `universe_version_id`, `sealed_dataset_revision_id`, `evaluation_design_version_id`, `promotion_policy_version_id`, `cause_event_id`, `assignment_no`, `state`, `created_at` |
| `alpha_evaluation_assignment_dataset_revisions` | `assignment_id`, `dataset_revision_id`, `phase`, `ordinal` |
| `alpha_evaluation_episodes` | `id`, `assignment_id` (unique), `state`, `result`, `sealed_at`, `evaluated_at`, `disclosed_at`, `consumed_at`, `invalid_reason` |
| `alpha_evaluation_results` | `id`, `episode_id` (unique), `evidence_validity`, `result`, `private_result_ref`, `evaluated_at` |
| `alpha_evaluation_metrics` | `result_id`, `metric_code`, `phase`, `value`, `status` |
| `alpha_evaluation_gates` | `result_id`, `gate_code`, `status`, `reason_code` |
| `alpha_evaluation_forecasts` | `evaluation_result_id`, `alpha_signal_artifact_id`, `instrument_id`, `as_of_time`, `effective_from`, `effective_until`, `expected_return`, `uncertainty`, `confidence`, typed capacity envelope |
| `search_ledger_entries` | `id`, `program_id`, `branch_id`, `mission_id`, `attempt_type`, `family_key`, `params_json`, `outcome_class`, `created_at` |
| `evidence_exposures` | `id`, `episode_id`, `subject_type`, `subject_id`, `level`, `created_at` |
| `disclosures` | `id`, `episode_id`, `audience`, `level`, `classification_json`, `created_at` |

`alpha_discovery_evaluations` is unique by `(source_mission_artifact_id, cause_event_id)` and
only its `FROZEN | QUEUED | RUNNING | VALID | INCONCLUSIVE | INVALID` processing state may
advance. `alpha_evaluation_assignments` is unique by `(source_mission_artifact_id, cause_event_id)` and
by `(alpha_model_version_id, cycle_id, assignment_no)`. Its Dataset relation is unique by
`(assignment_id, phase, ordinal)`. `AlphaEvaluationEpisode.assignment_id` and
`AlphaEvaluationResult.episode_id` are unique. These explicit keys, not a digest, make
validator/worker retries converge.

### 39.5 Portfolio

| 表 | 关键字段 |
|---|---|
| `portfolio_mandates` | `id`, `key`, `name`, `enabled`, `created_at` |
| `portfolio_mandate_versions` | `id`, `mandate_id`, `version_no`, `base_currency`, `objective`, `eligible_alpha_roles`, `universe_version_id`, typed V1 policy/constraint fields, `state`, `created_at` |
| `capital_context_versions` | `id`, `configuration_contract_version`, `source_type`, `source_downstream_system_id`, `base_currency`, `deployable_capital`, `observed_at`, `valid_until`, `created_at` |
| `portfolio_programs` | `id`, `mandate_version_id` (unique in V1), `state`, `current_candidate_id`, `created_at`, `updated_at` |
| `portfolio_input_evaluation_assignments` | `id`, `portfolio_program_id`, `mandate_version_id`, `capital_context_version_id`, `evaluation_dataset_selection_id`, `sealed_dataset_revision_id`, `promotion_policy_version_id`, `cause_event_id`, `previous_candidate_id` nullable, `as_of_time`, `evaluator_contract_version`, `state`, `private_result_ref`, `evaluated_at`, `outcome_code`, `created_at`, `completed_at` |
| `portfolio_input_evaluation_assignment_members` | `assignment_id`, `axis_index`, `alpha_qualification_id`, `alpha_evaluation_result_id`, `alpha_signal_artifact_id`, `instrument_id` (composite-ref to its forecast) |
| `portfolio_assembly_inputs` | `id`, `portfolio_input_evaluation_assignment_id` (unique), `portfolio_program_id`, `mandate_version_id`, `capital_context_version_id`, `universe_version_id`, `promotion_policy_version_id`, `cause_event_id`, `snapshot_no`, `input_contract_version`, `as_of_time`, `effective_from`, `effective_until`, `previous_candidate_id`, covariance method/observations/decay/shrinkage, typed V1 constraint/risk/cost/aversion fields, `state`, `outcome_code`, `created_at`, `completed_at` |
| `portfolio_assembly_input_members` | `input_id`, `axis_index`, `alpha_qualification_id`, `alpha_signal_artifact_id`, `instrument_id`, `expected_return`, `uncertainty`, `confidence`, `previous_weight`, `max_trade_notional`, `max_position_notional`, `max_participation_rate`, `days_to_liquidate`, `stressed_capacity` |
| `portfolio_assembly_input_covariances` | `input_id`, `left_axis_index`, `right_axis_index`, `covariance` (upper triangle) |
| `portfolio_search_ledger_entries` | `id`, `portfolio_program_id`, `cause_event_id`, `portfolio_assembly_input_id` nullable, `attempt_type`, `outcome_class`, `reason_code`, `created_at` |
| `portfolio_candidate_families` | `id`, `portfolio_program_id` (unique), `mandate_version_id`, `created_at` |
| `portfolio_candidates` | `id`, `assembly_input_id` (unique), `candidate_family_id` (non-null), `portfolio_program_id`, `mandate_version_id`, `capital_context_version_id`, `universe_version_id`, `state`, `created_at` |
| `portfolio_candidate_members` | `candidate_id`, `alpha_qualification_id`, `role`, `target_weight` |
| `portfolio_evaluation_assignments` | `id`, `candidate_id`, `candidate_family_id`, `previous_candidate_id` nullable, `assembly_input_id`, `evaluation_dataset_selection_id`, `sealed_dataset_revision_id`, `policy_version_id`, `cause_event_id`, `evaluator_contract_version`, `private_result_ref`, `state`, `evaluated_at`, `completed_at`, `created_at` |
| `portfolio_evaluation_episodes` | `id`, `assignment_id` (unique), `state`, `result`, `evaluated_at`, `disclosed_at`, `created_at` |
| `portfolio_evaluation_metrics` | `episode_id`, `metric_code`, `value`, `status` |
| `portfolio_evaluation_gates` | `episode_id`, `gate_code`, `status`, `reason_code` |
| `portfolio_evaluation_disclosures` | `episode_id` (unique), `candidate_id`, `classification_code`, `reason_code`, `created_at` |

`portfolio_input_evaluation_assignments` is unique by `(portfolio_program_id, cause_event_id)`;
its member identity is unique by `(assignment_id, axis_index)`,
`(assignment_id, alpha_qualification_id)` and `(assignment_id, instrument_id)`. It is the only
durable evaluator descriptor for Portfolio covariance. `portfolio_assembly_inputs` replaces the unimplemented
`portfolio_eligibility_snapshots`; there is no parallel optimization-run or content-addressed
input table. Its parent identity is unique by `(portfolio_program_id, snapshot_no)` and by
`(portfolio_program_id, cause_event_id)`; only one `PENDING` Input may exist per Program.
Member identity is unique by `(input_id, axis_index)`, `(input_id, alpha_qualification_id)` and
`(input_id, instrument_id)`; covariance identity is `(input_id, left_axis_index,
right_axis_index)` with `left_axis_index <= right_axis_index`. `portfolio_candidates` has one
`assembly_input_id`, and Candidate Packages are unique by `(candidate_id, revision)`.

V1 stores the listed risk/cost/capacity/constraint facts as typed scalar columns and relational
rows. JSON may hold non-gating display diagnostics only; it cannot encode a core relationship,
an Alpha set, a covariance matrix, a constraint or a model input. Reusable independent model
version tables are deferred until an actual second consumer exists; their absence must not be
papered over with free JSON or string labels.

`portfolio_evaluation_assignments` and `portfolio_evaluation_episodes` are one-to-one by
Assignment ID. `PORTFOLIO_EVALUATION` is their only result writer and its Job only references the
frozen Assignment resource with payload `{}`. Its evaluator descriptor includes Family and the
nullable frozen predecessor; `CANDIDATE_CURRENT` is not a valid Portfolio metric/gate or descriptor
field. Candidate-bound evidence, disclosures and exposures retain the Candidate UUID lineage; a
Package conformance check is not Portfolio Evaluation evidence.

Portfolio metric rows use only `AVAILABLE` finite values or `NOT_AVAILABLE` with no value. Portfolio
gate rows use only `PASS | FAIL | INCONCLUSIVE | INVALID`; a passing row has no reason and every
non-passing row has a nonempty reason. Each Portfolio Level-1 disclosure is one-to-one with its
Episode and explicitly carries that Episode's Candidate ID; `QUALIFIED` has no reason and every
other classification has one. No JSON field can substitute for any of these gate or disclosure facts.

### 39.6 Approval / Handoff / Feedback

| 表 | 关键字段 |
|---|---|
| `candidate_packages` | `id`, `candidate_id`, `package_revision`, `contract_version`, `state`, `manifest_json`, `relative_path`, `created_at` |
| `downstream_systems` | `id`, `name`, `environment_type`, `enabled`, `package_contract_version`, `feedback_contract_version`, `compatibility`, `preflight_state`, `revision`, `created_at` |
| `preflight_receipts` | `id`, `resource_type`, `resource_id`, `resource_revision`, `revision`, `status`, `reason_codes`, `capabilities`, `contract_version`, `checked_at`, `valid_until`, `checker_version` |
| `downstream_connection_versions` | `id`, `downstream_system_id`, `version_no`, `plugin_release_id` nullable, `credential_set_id` nullable, `package_contract_version`, `feedback_contract_version_id`, `public_config`, `state`, `created_at` |
| `feedback_contract_versions` | `id`, `downstream_system_id`, `version_no`, `purpose`, typed duration/sample/package/Arrow/disclosure fields, `spec_json` display-only, `created_at` |
| `feedback_contract_metric_requirements` | `feedback_contract_version_id`, `metric_code`, `ordinal` |
| `feedback_contract_accepted_package_contracts` | `feedback_contract_version_id`, `contract_version`, `ordinal` |
| `feedback_contract_accepted_arrow_contracts` | `feedback_contract_version_id`, `contract_version`, `ordinal` |
| `promotion_policy_versions` | `id`, `version_no`, `purpose`, `mode`, `policy_contract_version` nullable (`PROMOTION_POLICY_V1` for new facts), `paper_downstream_system_id`, `paper_connection_version_id`, `paper_feedback_contract_version_id`, `paper_preflight_receipt_id`, `live_downstream_system_id`, `live_connection_version_id`, `live_feedback_contract_version_id`, `live_preflight_receipt_id`, `paper_to_live_policy_version_id` nullable self-FK, `state`, `created_at` |
| `promotion_policy_gates` | `policy_version_id`, `metric_code`, `comparator`, `threshold`, `ordinal` |
| `promotion_evaluations` | `id`, `purpose`, `portfolio_evaluation_episode_id` nullable, `forward_evidence_episode_id` nullable, `candidate_id`, `candidate_package_id`, `package_revision`, `policy_version_id`, `paper_to_live_policy_version_id` nullable (P2P required), `downstream_system_id`, `downstream_connection_version_id`, `feedback_contract_version_id`, `preflight_receipt_id`, `outcome`, `action`, `created_at` |
| `promotion_gate_results` | `evaluation_id`, `gate_code`, `status`, `actual`, `expected`, `reason_code` |
| `approval_snapshots` | `id`, `type`, `promotion_evaluation_id`, `promotion_purpose` nullable, `candidate_id`, `candidate_package_id`, `package_revision`, `downstream_system_id`, `downstream_connection_version_id`, `feedback_contract_version_id`, `preflight_receipt_id`, `paper_to_live_policy_version_id` nullable, `state`, `evidence_snapshot_json`, `dependency_snapshot_json`, `valid_until`, `reason_code`, `note`, `created_at`, `decided_at` |
| `handoff_offers` | `id`, `approval_id`, `promotion_purpose` nullable, `paper_to_live_policy_version_id` nullable, `state`, `claim_deadline`, `claimed_at`, `accepted_at`, `revoked_at`, `expired_at`, `created_at` |
| `feedback_packages` | `id`, `handoff_offer_id`, `state`, `observation_start`, `observation_end`, `sample_size`, `summary_json`, `relative_path`, `received_at` |
| `forward_evidence_episodes` | `id`, `feedback_package_id`, `state`, `evaluation_summary`, `created_at` |
| `forward_evidence_metrics` | `episode_id`, `metric_code`, `value`, `status` |
| `degradation_observations` | `id`, `program_id`, `subject_type`, `subject_id`, `forward_evidence_episode_id`, `metric_name`, `severity`, `confidence`, `policy_revision`, `policy_snapshot`, `reason_code`, `state`, `consecutive_breaches`, `evaluated`, `created_at` |
| `research_wake_events` | `id`, `program_id`, `degradation_observation_id`, `forward_evidence_episode_id`, `subject_type`, `subject_id`, `policy_revision`, `reason_code`, `state`, `cycle_id`, `consumed_at`, `created_at` |

`feedback_contract_metric_requirements` 是 complete Feedback 的唯一 metric schema；其 ordinal
必须连续，完整 Feedback 的 typed rows 必须与之精确一致。`spec_json`、Feedback summary 与
optional artifact path 只能承载非 gating 展示资料。

`promotion_policy_versions` 的 `PROMOTION_POLICY_V1` tuple 以 purpose 严格 XOR：Alpha purpose 的 Paper/Live tuple
均为 NULL；`PORTFOLIO_TO_PAPER` 的 Paper tuple 全部非 NULL、Live tuple 全部为 NULL，且 mode
只能是 `MANUAL_APPROVAL`，并有非 NULL `paper_to_live_policy_version_id` 指向 P2L；
`PAPER_TO_LIVE` 的两个 tuple 均全部非 NULL，且 self-FK 为 NULL。目标 P2L 与来源 P2P 的
Paper tuple 必须完全相同。每个 tuple 的 logical Downstream、Connection、Feedback Contract 与
Receipt 必须精确相互对应，Receipt 不得由 writer 按 current/latest 重选。
`policy_contract_version IS NULL` 仅表示迁移前 legacy policy，所有生产 writer/API 结构性
拒绝它；新写入绝不能留 NULL。

`promotion_evaluations` 使用明确 `purpose`：`PORTFOLIO_TO_PAPER` 必有且只有一个
`portfolio_evaluation_episode_id`，不带 Forward Evidence；`PAPER_TO_LIVE` 必有且只有一个
`forward_evidence_episode_id`，不带 Portfolio Evaluation。两个方向均冻结具体 connection、
feedback contract 与仍有效的 `preflight_receipt_id`；不从 logical Downstream 的 mutable 现状
重选。partial unique 分别保证同一通过的 Portfolio Evaluation 只生成一次 Paper 决策、同一
完整 Forward Evidence 只生成一次 Live 决策。`approval_snapshots.promotion_evaluation_id` 与
`handoff_offers.approval_id` 都是一对一；`forward_evidence_metrics` 只保存 contract 已验证的
有限数值/`NOT_AVAILABLE` 状态，JSON summary 不能作为 promotion gate 输入。
P2P 的 `promotion_evaluations.paper_to_live_policy_version_id` 必须等于其 P2P policy 的
已验证 self-FK，P2L 为 NULL。P2P 写入的 Approval/Handoff 必须复制同一 ID，且
Promotion Evaluation→Approval→Handoff 的复合 lineage 也必须精确包含相同
`promotion_purpose`；Approval/Handoff 保留的 downstream-facing `purpose` 仅映射为
P2P→Paper、P2L→Live，不能代替该精确绑定；P2L
必须验证该 lineage 与 P2P policy 一致后才可使用它，不能以 mutable configuration 补全或替换。

### 39.7 Plugins / credentials / runtime configuration / artifacts

| 表 | 关键字段 |
|---|---|
| `plugin_releases` | `id`, `plugin_id`, `version`, `api_version`, `capabilities`, `state`, `descriptor_snapshot`, `created_at`, `activated_at`, `removed_at` |
| `plugin_artifacts` | `id`, `plugin_release_id`, `role`, `filename`, `relative_path`, `package_name`, `package_version`, `size_bytes` |
| `plugin_runtimes` | `id`, `plugin_release_id`, `state`, `python_version`, `environment_path`, `created_at`, `ready_at` |
| `archive_manifests` | `id`, `manifest_uri`, `data_source_id`, `universe_version_id`, `provider`, `source_license`, `source_spec`, `coverage_start`, `coverage_end`, `scanned_until`, `shard_count`, `total_bytes`, `missing_shard_count`, `probe_error_count`, `state`, `point_in_time_result`, `created_at`, `updated_at` |
| `archive_manifest_shards` | `id`, `manifest_id`, `shard_key`, `source_url`, `coverage_start`, `coverage_end`, `size_bytes`, `state`, `observed_at` |
| `credential_sets` | `id`, `purpose`, `owner_resource_type`, `owner_resource_id`, `public_config`, `created_at`, `updated_at` |
| `credential_secrets` | `credential_set_id`, `field_name`, `ciphertext`, `nonce`, `key_version` |
| `runtime_configurations` | `id`, `scope`, `revision`, `codex_model`, `codex_reasoning_effort`, `codex_fast_mode`, `codex_use_default_model_settings`, `codex_base_url`, `codex_api_key_ciphertext`, `codex_api_key_nonce`, `codex_api_key_key_version`, `max_plugin_wheel_bytes`, `plugin_validation_timeout_seconds`, `bundle_build_timeout_seconds`, `plugin_job_timeout_seconds`, `mission_job_timeout_seconds`, `job_poll_seconds`, `job_lease_seconds`, `created_at`, `updated_at` |
| `artifacts` | `id`, `kind`, `owner_type`, `owner_id`, `relative_path`, `media_type`, `size_bytes`, `created_at` |

## 40. API

Wire contract 由 FastAPI + Pydantic 定义。主要资源：

```text
POST   /api/v1/auth/login
GET    /api/v1/auth/session
POST   /api/v1/auth/logout

POST   /api/v1/ideas/preview
POST   /api/v1/research-programs
GET    /api/v1/research-programs
GET    /api/v1/research-programs/{id}
POST   /api/v1/research-programs/{id}/pause
POST   /api/v1/research-programs/{id}/resume
POST   /api/v1/research-programs/{id}/archive
POST   /api/v1/research-programs/{id}/restore
GET    /api/v1/research-programs/{id}/activity
GET    /api/v1/research-programs/{id}/missions

GET    /api/v1/alpha-library
GET    /api/v1/alpha-library/{qualification_id}

GET    /api/v1/portfolio-mandates
POST   /api/v1/portfolio-mandates/{id}/enable
POST   /api/v1/portfolio-mandates/{id}/disable
GET    /api/v1/portfolio-programs
GET    /api/v1/portfolio-candidates/{id}

GET    /api/v1/approvals
GET    /api/v1/approvals/{id}
POST   /api/v1/approvals/{id}/approve              # expected_state only; bound target cannot change
POST   /api/v1/approvals/{id}/reject

GET    /api/v1/handoffs
POST   /api/v1/handoffs/{id}/revoke
POST   /api/v1/handoffs/{id}/claim              # downstream service auth
POST   /api/v1/handoffs/{id}/accept             # downstream
POST   /api/v1/handoffs/{id}/reject             # downstream
GET    /api/v1/handoffs/{id}/package            # downstream
POST   /api/v1/handoffs/{id}/feedback           # downstream
POST   /api/v1/handoffs/{id}/degradation-observations  # Operator; no downstream control

# Fresh-install resource configuration: one canonical root contract.
# Collection reads always return {"items": [...], "next_cursor": ...}.
GET/POST /api/v1/universes
POST     /api/v1/universes/{universe_id}/versions
GET/POST /api/v1/data-sources
POST     /api/v1/data-sources/{data_source_id}/preflight
GET      /api/v1/datasets
POST     /api/v1/datasets/materializations
GET      /api/v1/datasets/{dataset_id}
GET      /api/v1/datasets/{dataset_id}/quality
GET      /api/v1/datasets/{dataset_id}/profile
GET/POST /api/v1/evaluation-dataset-selections
GET/POST /api/v1/evaluation-design-versions
GET/POST /api/v1/promotion-policy-versions
GET      /api/v1/operations/{operation_id}
GET/POST /api/v1/portfolio-mandates
POST     /api/v1/portfolio-mandates/{mandate_id}/versions
GET/POST /api/v1/capital-contexts
GET/POST /api/v1/downstream-systems
GET/POST /api/v1/downstream-connection-versions
GET/POST /api/v1/feedback-contract-versions
POST     /api/v1/downstream-connection-versions/{connection_version_id}/preflight  # downstream service auth; writes one frozen connection receipt
POST     /api/v1/downstream-systems/{downstream_id}/preflight  # downstream service auth
POST     /api/v1/downstream-systems/{downstream_id}/rotate-service-token
# /api/v1/configuration/* is not an API alias.
GET      /api/v1/portfolio-candidates/{candidate_id}/promotion-readiness
GET      /api/v1/readiness
GET      /api/v1/events/stream
GET      /api/v1/system/health                   # public healthcheck
GET/PUT  /api/v1/system/runtime-configuration
```

没有 `POST /alpha-evaluation-assignments`、手工 assemble、手工 portfolio evaluate、
手工 promote 或 Auto-Live endpoint。它们都是上述 Core validator / durable worker 的
内部写入；HTTP 只暴露 Administration 配置、只读状态和人工 Approval/Reject。Handoff
Feedback HTTP payload 只接受 contract 的 typed metric rows 与 observation header，不能携带
任意 evidence JSON 作为 Promotion 输入。

Operator Authentication 启用时，Operator API 要求 authenticated browser session/trusted-browser credential 或 machine API token；认证入口和 healthcheck 是明确例外。认证关闭时保留 direct access。下游 service credential 只授权其自身 Handoff/Feedback/preflight 资源，不形成业务用户/RBAC 域，也不被 Operator auth 替代。

统一错误 envelope：

```json
{
  "error": {
    "code": "APPROVAL_STALE",
    "message": "The approval snapshot is no longer current.",
    "details": {}
  }
}
```

## 41. Durable jobs / events / idempotency

`jobs` 使用 PostgreSQL lease；每个 mutation 资源自己拥有业务状态，job 只执行操作，不是事实源。

生产链 jobs 均只引用一个已冻结 resource，payload 必为 `{}`：
`PORTFOLIO_INPUT_EVALUATION(PortfolioInputEvaluationAssignment)`、
`PORTFOLIO_ASSEMBLY(PortfolioAssemblyInput)`、
`CANDIDATE_PACKAGE_BUILD(PortfolioCandidate)`、
`PORTFOLIO_EVALUATION(PortfolioEvaluationAssignment)`、
`PORTFOLIO_TO_PAPER_PROMOTION(PortfolioEvaluationEpisode)` 与
`PAPER_TO_LIVE_PROMOTION(ForwardEvidenceEpisode)`。worker 使用 lease-fenced transaction；
重复投递只能收敛到同一 frozen Assignment/Input/Candidate/Package/evaluation/snapshot/offer，
不能据 Job payload 选择事实或创造额外 identity。

`events` 与状态变更同事务写入。SSE 的 `id` 等于 `events.id`，前端使用 Last-Event-ID 恢复。

所有公开 mutation 支持 `Idempotency-Key`；同 key + 同 normalized request 返回原结果，同 key + 不同 request 返回冲突。业务 optimistic concurrency 使用 `expected_revision` / `expected_state` / `expected_version`，不使用内容 fingerprint。

---

# Part XI — Frontend

## 42. 页面设计

### Operator Login（仅在 `QUAZONAI_AUTH_ENABLED=true` 时）

- 未认证浏览器只显示登录门，不加载研究工作台数据；
- 只输入 Google Authenticator-compatible 6 位 TOTP 动态码，不展示、缓存或提交 username/password；
- 提供 `Trust this browser` 选项，并明确其会在当前浏览器保存长期 HttpOnly device credential；
- trusted-browser credential 有效时自动恢复会话，不要求再次输入 TOTP；
- 登录失败使用统一错误，不暴露哪一项凭证错误；
- logout 默认忘记当前 trusted browser。

### Home

- Action Center；
- Research Pulse；
- readiness badge；
- 最近 material events；
- `Propose Idea` / `Review Approvals`。

### Idea Composer

- natural-language editor；
- overlap recommendation；
- clarification card；
- generated Charter preview；
- Start Research。

### Research Observatory

Program list/detail：

- Charter；
- state / cooling/wake reason；
- Branch graph；
- Mission timeline；
- Alpha progression；
- Search Ledger summary；
- Evidence Exposure；
- Level 3 Codex activity/diff/test evidence。

### Alpha Library

- role/status/universe/horizon filters；
- Qualification lineage；
- standalone evidence；
- portfolio contribution；
- degradation state；
- Shadow Alpha view。

### Portfolio Lab

- Mandate cards；
- Portfolio Program lifecycle；
- Eligibility Snapshot；
- Alpha role map；
- redundancy graph；
- candidate comparison；
- risk/cost/capacity / multi-universe exposure。

普通用户不能手工拖 Alpha、改权重或修改 Candidate。

### Approval Inbox

每卡一个唯一推荐 Candidate：

- Paper/Live purpose；
- Mandate + Capital Context；
- recommendation rationale；
- Level 2 evidence；
- risk / drawdown / cost / capacity；
- 已冻结的 downstream / connection / contract / preflight receipt（只读，无 selector）；
- valid_until / freshness；
- Approve / Reject。

### Handoff & Feedback

- offer state；
- claim deadline；
- package contract；
- feedback milestones；
- stale/incomplete reason；
- unclaimed offer revoke；
- claimed 后只显示 advisory，不显示 stop runtime。

### Administration

- readiness；
- Codex login/status；
- Runtime Configuration：Codex model / reasoning effort / Fast service tier / custom Base URL / write-only API key；
- Runtime Configuration：Worker limits；
- Data Source Registry；
- Universe；
- Mandate templates；
- Capital Context；
- downstream systems；
- plugin releases；
- storage/worker/evaluator health。

### 42.1 国际化与本地化

Web 工作台的语言是**纯展示层用户偏好**。它不进入 Domain、API、Approval、Handoff 或任何不可变业务事实，也不改写后端规范枚举、reason code、对象 ID 或 wire payload。

V1 随前端静态包提供以下 UI locale：

```text
English
简体中文
繁體中文
日本語
한국어
Español
العربية
```

规则：

- 首次访问按“浏览器已保存偏好 → `navigator.languages` → English”解析 locale；显式切换后仅持久化到浏览器本地存储，不创建后端用户配置或新的业务状态；
- message catalog 必须有稳定 key、English source/fallback，并支持变量插值；数量语义使用 `Intl.PluralRules`，日期、数字、百分比使用当前 locale 的 `Intl` formatter；
- 切换 locale 必须同步 `html.lang`；RTL locale 必须同步 `html.dir=rtl`，并保证 App Shell、导航、表格、审批卡、时间线、表单和可访问性文本的阅读方向正确；
- UI chrome、固定说明、已知状态枚举和拒绝原因可以本地化，但 API 返回的自由文本研究内容、审计事实、用户输入、schema 字段、ID 和 canonical reason/status value 不做隐式机器翻译；
- 展示层可以把 canonical value 映射为本地化 label，但 mutation 和业务判断始终使用原始 canonical value；
- 新增用户可见固定文案应进入统一 catalog 或共享翻译 primitive，不能在各页面复制平行 locale 机制；
- 核心测试至少覆盖 locale negotiation、English fallback、catalog key、状态映射、持久化以及 RTL `lang/dir` 行为。

实现使用 React Context 与浏览器标准 `Intl` / `navigator.languages` / local storage 能力；i18n 不成为新的 server state，也不要求新增运行服务。

### 42.2 Responsive Web Workbench 与可安装 PWA

Web、移动浏览器和已安装 PWA 必须是同一个 React/Vite 客户端、同一套路由、API、Domain mutation、校验、分页、排序、搜索、图表和权限边界；不得维护 mobile fork、删减字段或把 PC-only mutation 隐藏成另一套业务状态。小屏 App Shell 使用 Home、Research、Approvals、Portfolio 四个主入口，Idea、Alpha、Handoff、Administration、语言、主题、安装/更新和退出从 More 进入；所有工作台能力仍可达。

响应式实现必须在 320/375/390/430 CSS px 下无水平溢出。DataTable 保持单一 TanStack table state；手机使用包含全部可见列、状态、摘要和行操作的 card projection，并保留搜索、排序、字段显示、结果数和 20/50/100 page-size 控件。Dialog、表单、图表和 React Flow 图必须适配触控、键盘、焦点、Reduced Motion 和 RTL；图表提供紧凑视图，图谱提供全屏和可读列表回退。

PWA 只缓存静态 application shell、版本化构建资源和图标。`/api/**` 使用 `NetworkOnly`，不做 API/auth/data cache、不保存 token/cookie、不使用 Background Sync；所有 mutation 仍在线执行。离线时只展示已缓存壳和明确的“需要连接 QuaZonai server”提示，不伪造或回退旧领域数据。Service Worker 使用 `registerType: prompt`：前台可见时每 15 分钟主动检查，后台恢复前台或联网恢复时以 60 秒最短间隔补查；发现 waiting worker 后自动打开确认 Dialog，但只在操作者选择立即更新后调用 `updateServiceWorker(true)` 激活并重载。选择稍后只关闭 Dialog，More → Update 仍保留；后台检查失败静默并由下一触发点重试。manifest 必须声明 standalone、`orientation:any`、192/512/maskable 图标和 Apple touch icon。

FastAPI 提供的 `index.html`、`sw.js`、`manifest.webmanifest` 使用 no-cache，带构建版本的静态 assets 使用 immutable；Web/API 分界不可被 SPA fallback 绕过，`/api` 不得返回 HTML，路径遍历必须拒绝。静态 Web 响应继续保留 `frame-ancestors 'none'` 与 `X-Frame-Options: DENY`。CI 必须对同一 smoke/parity flows 运行 desktop Chromium、mobile Chromium、mobile WebKit，并验证 manifest、Service Worker API policy、离线壳和静态安全响应。

---

# Part XII — Security & Isolation

## 43. Secrets

QZ 只管理研究数据源、Codex provider、Operator access 和下游 Handoff service credentials，不保存 broker/exchange trading credential。

Provider/Data/Handoff/ChatGPT Auth secret 使用既有 AES-256-GCM + externally injected master key 边界；Operator TOTP binding 使用同一 master key、binding UUID/AAD 与独立 `OperatorAuthConfiguration` ciphertext/nonce。Operator browser cookie 使用 independently generated、externally injected `QUAZONAI_AUTH_COOKIE_KEY`。该 key 解码后不得与 `QUAZONAI_MASTER_KEY` 相同，不能复用 browser credential 作为业务 secret。API 永不回读 persisted plaintext/ciphertext/nonce。Operator setup candidate/URI/code/API token/cookie key 不得进入 Codex Mission shell、事件、日志或第三方服务；ChatGPT access/refresh/id token、device auth id、authorization code、code verifier 和 user code 也不得进入这些边界；唯一例外是同源 setup/device start 的短暂浏览器响应与内存 UI 展示。

Provider/Data/Handoff/ChatGPT Auth secret 不得进入 Codex Mission shell、Research Tool Server 或持久事件；Codex provider credential 只通过受信任 runner 的 one-shot broker 进入 Codex command-backed provider authentication，ChatGPT access token 只通过 external auth 内存接口注入，不能进入 App Server environment 或命令行。

## 44. Sealed Evaluator isolation

`evaluator`：

- 不挂载 Codex workspace；
- 不挂载 CODEX_HOME；
- 不允许 Agent Tool 调用；
- 只访问 assigned sealed Dataset Revision 和 Candidate artifact；
- 只向 Core 写完整私有 result + policy-derived disclosure；
- Codex 只读 Level 1 disclosure。

Evaluator child 由固定绝对路径启动，不经过 shell；它只收到 assignment-scoped descriptor
路径和最小环境。descriptor 只包含受限 ID/revision，不能含 Dataset URI、raw frame/return、
secret、QZ database URL 或任意 command。Core 校验 stdout 的 typed result 后才写数据库；
任何子进程输出、超时或 schema 错误都只形成受限 failure code，不能把原始内容写进 Job
error 或 Event。

## 45. Threat / failure boundary

V1 假定合作的单机操作者；插件和 Mission code 不是恶意代码安全沙箱。暴露 Web/API 时，若启用 QuaZonai Operator Authentication 就必须验证 Operator 身份；若关闭认证，则必须由 loopback-only 或另一个明确可信的访问边界承担保护，不能无意中把服务公开为匿名工作台。trusted-browser cookie 等价于长期设备凭证，应只授予操作者控制的浏览器 profile；设备丢失/浏览器 profile 泄漏时通过轮换 `QUAZONAI_AUTH_COOKIE_KEY` 全局撤销。

不得因为“不是恶意沙箱”而放弃：

- Operator authentication / CSRF origin validation；
- Sealed 数据隔离；
- Secret 隔离；
- Mission workspace 限制；
- Tool capability server-side validation；
- immutable business snapshots；
- idempotency/concurrency；
- downstream ownership boundary。

---

# Part XIII — CLI & External Surface Summary

## 46. CLI 定位

`quazonai` 是 Web 之外的本地薄客户端，用于自动化、Admin、debug 和明确的人类操作。它只调用 Core API，不直接访问 DB/文件系统/Codex internals。启用 Operator Authentication 时，CLI 从环境读取 `QUAZONAI_API_TOKEN` 并以 Bearer machine credential 调用 Operator API；CLI 不模拟 browser session，也不读取 TOTP secret。

`QUAZONAI_API_TOKEN` 必须符合可直接写入 HTTP Authorization header 的 RFC 6750 `b64token` 语法；CLI 不对非法 token 做转义或降级。

Built-in Codex **不通过 CLI 作为 RPC**，而通过 mission-scoped stdio MCP Tool Server。

命令族详见 `CLI.md`。

---

# Part XIV — Implementation Plan

## 47. 代码树目标

```text
QuaZonai/
├─ README.md
├─ DESIGN.md
├─ OPERATIONS.md
├─ CLI.md
├─ AGENTS.md
├─ compose.yml
├─ deploy/
├─ frontend/
│  ├─ package.json
│  └─ src/
├─ backend/
│  ├─ pyproject.toml
│  ├─ uv.lock
│  ├─ alembic/
│  ├─ src/
│  │  ├─ api/
│  │  ├─ domain/
│  │  ├─ db/
│  │  ├─ orchestration/
│  │  ├─ research_engine/
│  │  ├─ evaluation/
│  │  ├─ agent_runtime/
│  │  ├─ plugins/
│  │  ├─ packaging/
│  │  └─ workers/
│  └─ tests/
└─ skills/
   └─ quazonai/
      └─ SKILL.md
```

旧 execution-specific code（Nautilus risk decorator、TradingNode/live supervisor、execution connections、deployment/recovery/order risk）必须删除而不是保留兼容 wrapper。

## 48. 实施阶段

### P0 — Governance & cleanup

- 本文、AGENTS、OPERATIONS、CLI、README、Skill 同步；
- 删除临时 Grill-Me 决策文件；
- inventory 旧 code path；
- 删除 Nautilus execution/control-plane 依赖和 native risk crate；
- 建立新 Alembic baseline 策略。

### P1 — Core domain + Web shell

- PostgreSQL schema；
- FastAPI resources/errors/events/jobs；
- React shell/Home/Idea/Observatory/Admin；
- Operator TOTP login / trusted-browser gate；
- readiness；
- Program lifecycle。

### P2 — Codex Harness

- agent-worker；
- App Server stdio client；
- per-Mission child / thread resume；
- Mission workspace Git manager；
- mission-scoped stdio MCP；
- event projection；
- role profiles；
- crash/interruption recovery。

### P3 — Data + Research Engine

- plugin runtime for Data Connector；
- Data Source Registry / Dataset Revision；
- Arrow/Parquet / Polars；
- Canonical Evaluator；
- Optuna；
- Search Ledger。

### P4 — Evaluation + Alpha Library

- Discovery/Sealed split；
- Evaluation Episode；
- Exposure Graph；
- deterministic disclosure；
- Alpha/Calibration/Qualification/Shadow lifecycle。

### P5 — Portfolio

- Mandate templates；
- Capital Context；
- portfolio auto-trigger；
- staged assembly；
- CVXPY policies；
- multi-universe normalization/risk/cost/capacity；
- Material Improvement Gate。

### P6 — Approval + Package + Handoff

- Approval Snapshot staleness/expiry；
- Candidate Package builder / Reference Runtime；
- downstream registry/service auth；
- claim/revoke；
- Feedback Contract；
- Forward Evidence；
- Degradation Monitoring。

### P7 — Full product acceptance

- browser E2E；
- Codex Mission E2E；
- Sealed non-leakage；
- data point-in-time tests；
- deterministic evaluation；
- package fixture conformance；
- downstream fake consumer E2E；
- crash/restart/replay；
- independent architecture review。

## 49. Required Acceptance Tests

### Product

- [ ] `QUAZONAI_AUTH_ENABLED=true` 时未认证浏览器不能读取或修改 Operator API；为 `false` 时保留 direct access；
- [ ] Google Authenticator-compatible TOTP 可建立 browser session；
- [ ] 勾选 Trust this browser 后，同一浏览器在 session 过期后可用 trusted credential 免 TOTP 恢复；
- [ ] logout/forget browser、trusted credential expiry 和 cookie-key rotation 会阻止后续免密恢复；
- [ ] Operator Authentication 启用后，配置缺失/非法时 API fail closed；启用认证的 production 要求 HTTPS/Secure cookie；healthcheck 仍保持 public；
- [ ] machine API token 可供 CLI/automation 使用且不会被下发到浏览器；
- [ ] downstream service token 仍只授权对应 Handoff/Feedback，Operator auth 不破坏 downstream contract；
- [ ] configured/browser Origin 经同一 canonical browser-origin 规则归一化，默认端口、大小写、IDNA 与 IPv6 等价表示可以匹配，不同 effective origin 被拒绝；
- [ ] `QUAZONAI_AUTH_COOKIE_KEY` 与有效 `QUAZONAI_MASTER_KEY` 解码值相同时启动失败；
- [ ] `QUAZONAI_API_TOKEN` 只接受 32–4096 字符 RFC 6750 `b64token`，空白、控制字符、非 ASCII 与非法字符在启动时被拒绝；
- [ ] cookie-authenticated unsafe request 的 Origin 不匹配时被拒绝；
- [ ] 第一次达到 RESEARCH_READY 后可从 Web 提交 Idea；
- [ ] Charter 澄清最多一轮并冻结；
- [ ] 正常 Program 无需人工操作即可持续到 Candidate；
- [ ] 同 Program 同时最多一个可操作 Approval；
- [ ] Approval stale/expired 后不能批准；
- [ ] Paper approval 不产生 Live authorization；
- [ ] claimed Handoff 后 QZ UI 不提供 stop runtime。

### Codex

- [ ] App Server 仅使用固定受支持版本和 stdio；
- [ ] 每 Mission 独立 process/worktree；
- [ ] worker crash 后可 resume Thread 或安全重建 Mission attempt；
- [ ] Codex 无 Sealed root / Secret / downstream credential；
- [ ] network-disabled Mission 仍可通过 MCP 使用批准数据能力；
- [ ] Codex 输出不能绕过 Domain Validator 推进状态；
- [ ] reasoning 内容不持久化为产品事实；
- [ ] Administration 配置的 custom Base URL/model/API key 可在不依赖 `.env` 的情况下应用于新 Mission；
- [ ] Administration 配置的 reasoning effort/Fast 只影响后续新 Mission，`null`/Standard 保持 Codex 默认行为；
- [ ] reasoning effort 只接受 `minimal|low|medium|high|xhigh`，Fast 不静默降级，且二者不记录隐藏 reasoning；
- [ ] Codex provider API key 不回读、不写事件、不进入 App Server env/命令行，也不会进入 Mission shell；
- [ ] Runtime Configuration stale revision 与并发首次创建返回业务冲突，幂等重试不重复写入；
- [ ] Worker limits 修改无需重启，并只影响之后领取/启动的工作。
- [ ] finite-worker preflight 可实际创建 Codex workspace sandbox namespace；默认 seccomp 不可用时必须 fail closed，不能把 Mission 降级为无沙箱执行。

### Research / Evidence

- [ ] point-in-time available_at 防 look-ahead；
- [ ] Search Ledger 保存失败与被淘汰尝试；
- [ ] Exposure 跨 Branch/Program relationship 继承；
- [ ] Sealed disclosure 后 Episode 被消费；
- [ ] Level 1 不泄漏日期/instrument/精确指标；
- [ ] 新 Qualification 不恢复旧 quarantined 版本。

### Portfolio

- [ ] relative score 未校准时不能进入 required-return policy；
- [ ] Shadow Alpha 不能直接 Handoff；
- [ ] multi-universe cost/risk 不混用；
- [ ] Candidate 任一依赖改变会创建新 Candidate；
- [ ] Material Improvement 不足不会生成 Approval；
- [ ] Capital Context 超出 capacity envelope 会阻断 Promotion。

### Handoff

- [ ] approval 绑定明确 downstream/contract/version；
- [ ] AVAILABLE claim 与 revoke 原子竞争；
- [ ] CLAIMED 后不能 revoke runtime；
- [ ] stale/partial feedback 不被计为 Candidate failure；
- [ ] 只有 complete valid Paper feedback 可进入 Live Promotion；
- [ ] degradation 只触发 Research wake/advisory，不控制下游。

### No custom hash gates

- [ ] DB schema 无应用级 hash/checksum/digest/fingerprint 业务字段；
- [ ] Package、plugin、workspace、approval、idempotency 不以内容 hash 做身份或 Gate；
- [ ] Operator auth 不引入自定义 password/session/TOTP hash gate；cookie 使用标准 authenticated encryption；
- [ ] 测试不引入自定义完整性 hash 流程。

## 50. 当前实现状态与迁移原则

截至本基线，已验证的实现切片是：Draft/冻结 Charter/固定 DAG 与 durable Mission
facts；PIT/Alpha signal/诚实评估的纯合同和 Alpha 版本化持久化基础；至少两个 Alpha
才可求解的 target-weight-only Portfolio engine；target-only Package archive/禁止字段
验证；Promotion 的纯 fail-closed policy；以及 completed Forward Evidence 到 immutable
Observation、deduplicated Wake、固定 Diagnostic/Replan Cycle 的受限持久化闭环。它们不能
合并表述成一个已验证的 Research→Paper→Live 全链路。

尤其不得声称已完成：从空库的生产配置、真实多 Agent Alpha 研究、Package-before-
Approval、独立 simulation gate、自动 Live Handoff，或完整 Research→Paper→Live
全链路。每一项仍需要真实对象、契约测试、集成测试和独立复核；seed、mock 或旧生成式
执行 runtime 不能替代。

迁移原则：

- 新架构没有旧运行状态兼容义务；
- 优先删除旧生成式执行、execution-control、动态第三方 plugin 和旧 runtime
  兼容路径，再扩展新领域；
- 现有 DB 不做复杂业务迁移；开发阶段采用新的干净 baseline；
- 临时 Grill-Me 文件在本文成为事实源后删除；
- 任何实现偏离本文必须先更新本文并说明原因。

---

本文描述 QuaZonai 的最终 V1 产品与技术目标：**持续自治研究 + Alpha Library + Portfolio Construction + Human Approval + Downstream-neutral Handoff**。除非代码、测试和独立复核证明相应验收项通过，不得把目标能力描述为已交付或 production-ready。
