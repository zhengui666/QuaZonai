# QuaZonai 产品需求与技术架构设计

> 架构基线：2026-08-24  
> 文档地位：**QuaZonai 唯一完整的产品与架构事实源**  
> 目标：Codex Harness 驱动的单用户、自托管、持续自治量化研究与策略组合工作台  
> 当前状态：**目标方案已锁定；现有代码仍包含旧 Nautilus 执行控制路径，尚未 conforming / release-ready**

`OPERATIONS.md` 只展开用户运行视图；`CLI.md` 只展开 CLI、Codex Runtime 和 Agent Tool 合同；`README.md` 只做入口与当前状态摘要；代码、测试、聊天记录和临时决策文件不得静默改写本文。

---

## 0. 执行摘要

QuaZonai（QZ）只拥有两个核心领域：

1. **Research Intelligence**：从自然语言 Idea 到可验证 Alpha；
2. **Portfolio Construction**：把已验证 Alpha 映射到明确 Portfolio Mandate，形成可交付 Portfolio Candidate。

QuaZonai **不拥有交易执行**。NautilusTrader、LEAN 或任何自定义执行系统均是独立下游 Consumer。QZ 不启动、停止、监控或恢复交易节点，不保存 broker credential，不提交订单，不维护订单、成交、仓位、账户或 NAV，不提供中央执行风险，不把下游状态伪装为自己的 Deployment 状态。

正常 Research Program 生命周期中，人类只有两类常规操作：

1. **提出 Research Idea**；
2. **审批系统推荐的 Paper / Live Candidate Handoff**。

首次安装、数据授权、Codex 登录与 Runtime Configuration、Mandate/Universe/下游配置、插件管理和故障处理属于低频 Administration，不计入正常研究旅程。

完整闭环：

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

### 2.1 Idea Composer

```text
自然语言 Idea
→ 解析 Market / Hypothesis / Horizon / Data Domain / Explicit Exclusions
→ 重大歧义时一次性提出 1–3 个问题
→ 生成 Research Charter 摘要
→ 用户点击 Start Research
→ Charter 冻结
```

只追问会改变研究边界的问题；模型、特征、CV、优化器、参数、算力和技术实现由系统决定。

### 2.2 重叠 Idea

系统在 Idea Composer 内检测与已有 Program 的语义和领域重叠：

- 实质重复：默认记录 `IdeaContribution` 并唤醒已有 Program；
- 原 Charter 内新角度：推荐在已有 Program 新建 Research Branch；
- 超出原 Charter：创建关联的新 Program；
- 用户可以坚持创建独立 Program，但适用的 Search Ledger 与 Evidence Exposure 必须继承，不能制造“新独立证据”。

这个选择仍属于“提出 Idea”操作。

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
- Restore。

正式开始的 Program 不提供业务层物理删除。只有未提交 Idea Draft 可以删除。

Pause/Archive 只影响 QZ 研究，不停止任何已领取 Package 的外部 Paper/Live 系统。

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
- 至少一个 Discovery Dataset 或批准的数据获取能力可用；
- Canonical Research Engine 最小 preflight 通过；
- Sealed Evaluator 与 Codex 的访问隔离成立；
- Scheduler 有可用 Mission slot。

达到后即可提出 Idea，不要求先配置 Paper/Live 下游。

### 4.2 PAPER_HANDOFF_READY / LIVE_HANDOFF_READY

Paper readiness 要求至少一个可用 Paper downstream、Package/Feedback Contract 兼容、claim/feedback preflight 成功。

Live readiness 除独立 Live downstream preflight 外，还要求完整、有效、已重新评估的 Paper Forward Evidence。

缺少下游 readiness 时，研究继续。候选进入 `PAPER_CONFIGURATION_REQUIRED` 或 `LIVE_CONFIGURATION_REQUIRED`，不创建无法行动的 Approval。

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
                    ├→ FAILED
                    ├→ INTERRUPTED
                    └→ CANCELLED
```

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

Codex Thread 是 Mission 执行上下文，不是业务状态。

## 7. Mission Graph 与自治调度

Research Program 使用持久化 Mission DAG，不使用“让 Agent 一直继续研究”的无限聊天。

典型节点：

```text
PLAN_RESEARCH
DATA_REQUIREMENT
DATA_QUALITY
HYPOTHESIS
FEATURE_RESEARCH
ALPHA_DISCOVERY
CALIBRATION
ROBUSTNESS
PROMOTION_REVIEW
PORTFOLIO_ASSEMBLY
DEGRADATION_DIAGNOSIS
```

每次 Mission 结束后由 deterministic Orchestrator 依据 Domain Event 和 Policy 决定：

- 解锁依赖节点；
- 创建 replan Mission；
- 进入 Promotion；
- 进入 COOLING；
- 等待 Data Capability / Forward Feedback；
- 标记 BLOCKED。

Codex 可以提交“建议的 Mission Graph / replan artifact”，但只有 QZ Domain Validator 可以持久化正式图。

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

## 11. Canonical Research Engine

QZ Research Engine 与 NautilusTrader 完全无关。

技术基线：

- Canonical columnar format：Apache Arrow / Parquet；
- Feature computation：Polars Lazy；
- 参数与多目标搜索：Optuna；
- Portfolio optimization：CVXPY；
- Canonical evaluator：QZ 自有最小确定性、向量化 target-weight evaluator；
- 可选第三方 research adapters 不成为业务事实源。

Research Engine 模拟：

```text
signals
→ target exposure / weight
→ rebalance
→ cost/slippage/capacity assumptions
→ returns/risk/turnover
```

不模拟 broker session、order lifecycle、partial fill、venue protocol、credentials、recovery 或真实账户状态。

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

## 18. Portfolio Program 自动创建

满足以下条件时自动创建 Portfolio Program：

```text
Enabled Mandate
+ Qualified Alpha Pool
+ 可证明的组合机会
+ 不存在等价活跃 Portfolio Program
```

没有合格 Alpha 时是 `WAITING_FOR_ALPHA`，不是失败。

Portfolio Program 永久绑定一个 Mandate Version，并在 Promotion 时冻结 Capital Context Version。

## 19. Staged Portfolio Assembly

```text
Alpha Library
→ Eligibility Snapshot
→ Role Pools
→ Redundancy / Common-source Clustering
→ Portfolio Skeletons
→ Approved Policy Families
→ Discovery Evaluation
→ Robustness / Marginal Contribution
→ Candidate Family
→ Portfolio-level Sealed Evaluation
```

第一版 approved policy families：

- Equal Weight；
- Volatility Scaling；
- Risk Parity；
- Hierarchical Risk Parity；
- Constrained Mean-Variance；
- Mean-CVaR。

`PortfolioSearchLedger` 保存 Alpha subset、role、policy、constraint、rebalance 和结果。

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
evaluation_episode_id
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

`MaterialImprovementPolicyVersion` 综合：

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

Snapshot 冻结：Candidate Package、Evidence Set、Alpha/Calibration、Mandate、Capital Context、Portfolio Policy、Risk/Cost/Capacity/Constraint、目标 downstream、downstream connection version、Package/Feedback Contract、compatibility preflight、validity policy。

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

## 24. Candidate Package

V1 标准格式：

```text
candidate-package/
  manifest.json
  schemas/
  runtime/
    feature_pipeline.whl
    alpha_model.whl
    calibration.whl
    portfolio_policy.whl
  fixtures/
    input.arrow
    expected_alpha.arrow
    expected_portfolio.arrow
  evidence/
    approval-summary.json
  lineage.json
```

Python Reference Runtime 是正式参考实现：

```text
canonical input
→ Feature Pipeline
→ Alpha
→ Calibration
→ Portfolio Policy
→ TargetPortfolioFrame
```

它不连接行情源、broker 或 wallet，不提交订单。

Package 禁止包含：broker URL、API key、private key、account ID、order type、TIF、order id、recovery、heartbeat 或 execution retry。

QZ 不为 Package 创建应用级 hash/checksum/fingerprint。完整性与兼容性依赖：显式 artifact ID/version、文件名/长度、wheel/package metadata、schema validation、Reference Fixture 执行结果与 contract version。

## 25. Handoff Registry

用户在 Approval 页选择**逻辑下游系统**，例如 `Nautilus Paper Lab`、`Nautilus Live Primary`、`External Validator`，不选择机器、容器或节点。

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
required_fields
first_status_deadline
complete_feedback_deadline
grace_period
accepted_package_contracts
accepted_arrow_contracts
disclosure_policy
```

缺失、迟到、部分或 invalid feedback 是运营/证据质量问题，不等于 Candidate 失败。

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
HEALTHY → WATCH → DEGRADING → INVALIDATED
```

`DegradationPolicyVersion` 判断持续时间、严重程度、统计置信度、多 Episode 一致性、Mandate 影响和是否存在可研究的新信息。

只有达到门槛才自动 wake Program 并创建 Diagnostic Mission。不会自动停止下游、自动换仓或替换 Live Candidate。

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
- 初始化连接后使用 `thread/start` / `thread/resume`、`turn/start`、`turn/interrupt` 和 item/turn notifications；
- `runtimeWorkspaceRoots`、project API、environments、dynamicTools 等 experimental 字段不作为 V1 必需依赖；需要时必须先在设计中升级为批准能力。

## 30. Codex Process Model

每个 RUNNING Mission 使用独立 Codex App Server child process：

- 共享只读/受控 `CODEX_HOME` 认证与 thread persistence volume；
- 一个 Mission 对应一个 durable Codex Thread；
- Mission crash 后可由新 child `thread/resume`；
- child 退出即释放 shell、MCP 和文件句柄；
- 不在一个无限长 Thread 中承载整个 Research Program。

这提供清晰的权限、workspace、tool 和失败边界。

### 30.1 Runtime Configuration ownership

Codex provider 配置与 Worker limits 属于**运行时管理配置**，由本地 Administrator 在 Web Administration 中维护，并持久化到 PostgreSQL 单例 `runtime_configurations`。它们不是 Compose/bootstrap 环境变量。

`.env` / process environment 只负责启动级基础设施：

```text
QUAZONAI_ENV
PostgreSQL database/user/password + DATABASE_URL/ALEMBIC_URL
QUAZONAI_MASTER_KEY
plugin/package/mission storage roots
HTTP port
fixed CODEX_HOME / frontend dist deployment paths
```

Runtime Configuration 至少包含：

```text
revision
codex_model nullable
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
- `codex_base_url` 支持自定义 OpenAI-compatible API root，必须是绝对 HTTP(S) URL；
- Base URL 不允许内嵌 username/password、query token 或 fragment；
- 配置 custom Base URL 或 API key 时，App Server 使用显式 model provider，V1 wire API 固定为 Responses；
- provider API key 不进入 App Server environment、命令行或 `--config`。受信任 Mission runner 只在内存中持有解密后的 key，通过 `0700` 临时目录下的 `0600` one-shot Unix socket broker 向 Codex 0.144.4 的 command-backed model-provider `auth` helper 交付一次 token；helper 在首个 provider request 前取用后 broker 关闭，Mission shell、MCP Tool Server、Agent output 与持久 event 均不得获得该 key；
- App Server environment 必须显式清除 provider API key、`QUAZONAI_MASTER_KEY` 与数据库连接 secret，不能依赖普通 shell env filtering 作为 Secret 边界；
- 已保存 provider key 时修改 `codex_base_url`，必须在同一 mutation 中重新输入 key 或显式清除旧 key，禁止把旧 credential 静默重绑定到新 endpoint；
- 未配置 custom provider credential 时，可继续使用持久 `CODEX_HOME` 中的官方 Codex/ChatGPT 登录；
- Web/API 只返回 `codex_api_key_configured` 状态，不回读 plaintext/ciphertext/nonce。

Runtime Configuration mutation 规则：

- GET 返回当前单调递增 `revision`；尚未创建 singleton 时为 revision `0`；
- PUT 必须携带 `expected_revision`，陈旧保存返回 `RUNTIME_CONFIGURATION_STALE`，首次并发创建的唯一约束竞争也必须被翻译为同一业务冲突而不是数据库 500；
- PUT 支持 `Idempotency-Key`；同一个逻辑请求重试返回原响应，不重复加密 provider key、不重复推进 revision、也不重复写 `RUNTIME_CONFIGURATION_UPDATED` event；
- Idempotency receipt 不保存 provider key plaintext，也不为了去重额外保存历史 secret 副本。

Worker 规则：

- finite worker 每次领取后续 job 前读取最新 Runtime Configuration；
- plugin validator/bundle child 与 Research Mission child 在启动时冻结当次有效配置；
- `job_poll_seconds` 服务端与数据库下界为 `0.01` 秒，禁止近零 busy loop；
- Administration 保存后不要求重建或重启 Compose stack；
- 已运行 child 的 timeout/model/provider 不被中途改写，修改只影响之后领取/启动的工作。

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

Production Compose 最小服务：

```text
postgres
migrate
api                 # FastAPI + built React static assets + SSE
worker              # jobs: data/plugin/package/handoff/degradation
agent-worker        # Codex App Server child + Mission lifecycle
evaluator           # Sealed Promotion evaluator, no Codex workspace access
```

不引入 Redis、Celery、Kafka 或 Kubernetes。使用 PostgreSQL durable jobs + `FOR UPDATE SKIP LOCKED`，事件表 + `LISTEN/NOTIFY` 仅做唤醒。

`api` 默认只发布宿主 `127.0.0.1:8000`。远程访问由操作者自己的受信网络/TLS 层处理；V1 不建设多用户 auth。

## 38. 技术栈

Backend：

- Python 3.14.x；
- FastAPI / Pydantic v2；
- SQLAlchemy 2 / Alembic；
- psycopg 3；
- PostgreSQL 18；
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

### 39.4 Research assets & evidence

| 表 | 关键字段 |
|---|---|
| `feature_pipeline_versions` | `id`, `program_id`, `branch_id`, `version_no`, `artifact_id`, `contract_json`, `created_at` |
| `alpha_model_versions` | `id`, `program_id`, `branch_id`, `version_no`, `mode`, `artifact_id`, `horizon`, `created_at` |
| `alpha_calibration_versions` | `id`, `alpha_model_version_id`, `version_no`, `method`, `training_evidence_json`, `artifact_id`, `created_at` |
| `alpha_qualifications` | `id`, `alpha_model_version_id`, `calibration_version_id`, `universe_version_id`, `role`, `state`, `scope_json`, `evaluation_episode_id`, `created_at` |
| `search_ledger_entries` | `id`, `program_id`, `branch_id`, `mission_id`, `attempt_type`, `family_key`, `params_json`, `outcome_class`, `created_at` |
| `evaluation_episodes` | `id`, `kind`, `state`, `universe_version_id`, `dataset_revision_id`, `policy_version`, `sealed_at`, `assigned_candidate_type`, `assigned_candidate_id`, `evaluated_at`, `disclosed_at`, `consumed_at`, `invalid_reason` |
| `evidence_exposures` | `id`, `episode_id`, `subject_type`, `subject_id`, `level`, `created_at` |
| `disclosures` | `id`, `episode_id`, `audience`, `level`, `classification_json`, `created_at` |

### 39.5 Portfolio

| 表 | 关键字段 |
|---|---|
| `portfolio_mandates` | `id`, `key`, `name`, `enabled`, `created_at` |
| `portfolio_mandate_versions` | `id`, `mandate_id`, `version_no`, `spec_json`, `state`, `created_at` |
| `capital_context_versions` | `id`, `source_type`, `source_downstream_system_id`, `base_currency`, `deployable_capital`, `observed_at`, `valid_until`, `created_at` |
| `portfolio_programs` | `id`, `mandate_version_id`, `state`, `created_at`, `updated_at` |
| `portfolio_eligibility_snapshots` | `id`, `portfolio_program_id`, `snapshot_no`, `alpha_set_json`, `created_at` |
| `portfolio_search_ledger_entries` | `id`, `portfolio_program_id`, `eligibility_snapshot_id`, `attempt_json`, `outcome_class`, `created_at` |
| `portfolio_candidates` | `id`, `candidate_family_id`, `portfolio_program_id`, `mandate_version_id`, `capital_context_version_id`, `universe_set_json`, `policy_version`, `risk_model_version`, `cost_model_version`, `capacity_model_version`, `constraint_set_version`, `evaluation_episode_id`, `state`, `created_at` |
| `portfolio_candidate_members` | `candidate_id`, `alpha_qualification_id`, `role`, `target_contribution` |

### 39.6 Approval / Handoff / Feedback

| 表 | 关键字段 |
|---|---|
| `candidate_packages` | `id`, `candidate_id`, `contract_version`, `state`, `manifest_json`, `relative_path`, `created_at` |
| `downstream_systems` | `id`, `name`, `environment_type`, `enabled`, `created_at` |
| `downstream_connection_versions` | `id`, `downstream_system_id`, `version_no`, `plugin_release_id`, `public_config`, `credential_set_id`, `state`, `created_at` |
| `feedback_contract_versions` | `id`, `version_no`, `purpose`, `spec_json`, `created_at` |
| `approvals` | `id`, `type`, `candidate_id`, `candidate_package_id`, `downstream_system_id`, `downstream_connection_version_id`, `feedback_contract_version_id`, `state`, `evidence_snapshot_json`, `dependency_snapshot_json`, `valid_until`, `reason_code`, `note`, `created_at`, `decided_at` |
| `handoff_offers` | `id`, `approval_id`, `state`, `claim_deadline`, `claimed_at`, `accepted_at`, `revoked_at`, `expired_at`, `created_at` |
| `feedback_packages` | `id`, `handoff_offer_id`, `state`, `observation_start`, `observation_end`, `sample_size`, `summary_json`, `relative_path`, `received_at` |
| `forward_evidence_episodes` | `id`, `feedback_package_id`, `state`, `evaluation_summary`, `created_at` |
| `degradation_observations` | `id`, `subject_type`, `subject_id`, `forward_evidence_episode_id`, `state`, `severity`, `classification`, `created_at` |

### 39.7 Plugins / credentials / runtime configuration / artifacts

| 表 | 关键字段 |
|---|---|
| `plugin_releases` | `id`, `plugin_id`, `version`, `api_version`, `capabilities`, `state`, `descriptor_snapshot`, `created_at`, `activated_at`, `removed_at` |
| `plugin_artifacts` | `id`, `plugin_release_id`, `role`, `filename`, `relative_path`, `package_name`, `package_version`, `size_bytes` |
| `plugin_runtimes` | `id`, `plugin_release_id`, `state`, `python_version`, `environment_path`, `created_at`, `ready_at` |
| `credential_sets` | `id`, `purpose`, `owner_resource_type`, `owner_resource_id`, `public_config`, `created_at`, `updated_at` |
| `credential_secrets` | `credential_set_id`, `field_name`, `ciphertext`, `nonce`, `key_version` |
| `runtime_configurations` | `id`, `scope`, `revision`, `codex_model`, `codex_base_url`, `codex_api_key_ciphertext`, `codex_api_key_nonce`, `codex_api_key_key_version`, `max_plugin_wheel_bytes`, `plugin_validation_timeout_seconds`, `bundle_build_timeout_seconds`, `plugin_job_timeout_seconds`, `mission_job_timeout_seconds`, `job_poll_seconds`, `job_lease_seconds`, `created_at`, `updated_at` |
| `artifacts` | `id`, `kind`, `owner_type`, `owner_id`, `relative_path`, `media_type`, `size_bytes`, `created_at` |

## 40. API

Wire contract 由 FastAPI + Pydantic 定义。主要资源：

```text
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
POST   /api/v1/approvals/{id}/approve
POST   /api/v1/approvals/{id}/reject

GET    /api/v1/handoffs
POST   /api/v1/handoffs/{id}/revoke
POST   /api/v1/handoffs/{id}/claim              # downstream service auth
POST   /api/v1/handoffs/{id}/accept             # downstream
POST   /api/v1/handoffs/{id}/reject             # downstream
GET    /api/v1/handoffs/{id}/package            # downstream
POST   /api/v1/handoffs/{id}/feedback           # downstream

GET/POST /api/v1/data-sources
GET      /api/v1/datasets
GET      /api/v1/universes
GET/POST /api/v1/downstream-systems
GET      /api/v1/readiness
GET      /api/v1/events/stream
GET      /api/v1/system/health
GET/PUT  /api/v1/system/runtime-configuration
```

下游 service credential 只授权其自身 Handoff/Feedback 资源，不形成业务用户/RBAC 域。

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

`events` 与状态变更同事务写入。SSE 的 `id` 等于 `events.id`，前端使用 Last-Event-ID 恢复。

所有公开 mutation 支持 `Idempotency-Key`；同 key + 同 normalized request 返回原结果，同 key + 不同 request 返回冲突。业务 optimistic concurrency 使用 `expected_revision` / `expected_state` / `expected_version`，不使用内容 fingerprint。

---

# Part XI — Frontend

## 42. 页面设计

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
- downstream selector（只显示兼容目标）；
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
- Runtime Configuration：Codex model / custom Base URL / write-only API key；
- Runtime Configuration：Worker limits；
- Data Source Registry；
- Universe；
- Mandate templates；
- Capital Context；
- downstream systems；
- plugin releases；
- storage/worker/evaluator health。

---

# Part XII — Security & Isolation

## 43. Secrets

QZ 只管理研究数据源、Codex provider 和下游 Handoff service credentials，不保存 broker/exchange trading credential。

使用 AES-256-GCM，master key 外部注入。API 永不回读 plaintext/ciphertext/nonce。Provider/Data/Handoff secret 不得进入 Codex Mission shell、Research Tool Server 或持久事件；Codex provider credential 只通过受信任 runner 的 one-shot broker 进入 Codex command-backed provider authentication，不能进入 App Server environment 或命令行。

## 44. Sealed Evaluator isolation

`evaluator`：

- 不挂载 Codex workspace；
- 不挂载 CODEX_HOME；
- 不允许 Agent Tool 调用；
- 只访问 assigned sealed Dataset Revision 和 Candidate artifact；
- 只向 Core 写完整私有 result + policy-derived disclosure；
- Codex 只读 Level 1 disclosure。

## 45. Threat / failure boundary

V1 假定合作的单机操作者；插件和 Mission code 不是恶意代码安全沙箱。但必须通过进程、文件根、DB credential 和 capability scope 防止意外越界。

不得因为“不是恶意沙箱”而放弃：

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

`quazonai` 是 Web 之外的本地薄客户端，用于自动化、Admin、debug 和明确的人类操作。它只调用 Core API，不直接访问 DB/文件系统/Codex internals。

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
│  │  ├─ mcp_gateway/
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
- [ ] Codex provider API key 不回读、不写事件、不进入 App Server env/命令行，也不会进入 Mission shell；
- [ ] Runtime Configuration stale revision 与并发首次创建返回业务冲突，幂等重试不重复写入；
- [ ] Worker limits 修改无需重启，并只影响之后领取/启动的工作。

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
- [ ] 测试不引入自定义完整性 hash 流程。

## 50. 当前实现状态与迁移原则

截至本基线，仓库代码仍主要实现旧的 QZ+Nautilus execution control-plane。它与本文冲突，不能称为 conforming。

迁移原则：

- 新架构没有旧运行状态兼容义务；
- 优先删除 execution-specific code，再实现新领域；
- 现有 DB 不做复杂业务迁移；开发阶段采用新的干净 baseline；
- 临时 Grill-Me 文件在本文成为事实源后删除；
- 任何实现偏离本文必须先更新本文并说明原因。

---

本文描述 QuaZonai 的最终 V1 产品与技术目标：**持续自治研究 + Alpha Library + Portfolio Construction + Human Approval + Downstream-neutral Handoff**。除非代码、测试和独立复核证明相应验收项通过，不得把目标能力描述为已交付或 production-ready。