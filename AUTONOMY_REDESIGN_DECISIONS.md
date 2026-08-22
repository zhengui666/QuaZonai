# QuaZonai Codex 自治研究工作台：Grill-Me 决策记录

> 状态：**工作草案，不是当前 `DESIGN.md` 的竞争事实源**。  
> 分支：`design/codex-autonomy-prd`。  
> 用途：记录本轮 Grill-Me 已确认的产品与关键技术决策；讨论结束后据此重写正式 PRD、`DESIGN.md`、`OPERATIONS.md`、CLI/Skill 方案，并删除本文件或将其归档为历史决策记录。

当前仓库设计仍以 QuaZonai 控制 NautilusTrader 运行、执行、Recovery 和风险为前提。本轮重构已经明确推翻该前提：**QuaZonai 只负责持续自治研究、Alpha 校准、策略组合、候选审批与下游交付；NautilusTrader 或其他系统完全独立地负责模拟盘、实盘和交易执行。**

---

## 1. 产品定位

QuaZonai V2 是一个：

> **由 Codex Harness 驱动、单用户自托管、持续运行的自治量化研究与策略组合工作台。**

产品只承担：

- 自然语言 Idea 理解与 Research Charter；
- 持续自治研究池；
- Feature、Alpha、Calibration、Portfolio Policy 的生成与评估；
- 可消耗、分层保护的独立证据；
- 全局 Alpha Library；
- 角色约束的策略组合研究；
- Paper / Live 候选审批；
- 下游无关 Candidate Package；
- Handoff Registry 与 Forward Feedback 导入。

产品不承担：

- 行情接入、交易账户、Credential、订单、成交、仓位和 NAV；
- 模拟盘或实盘运行进程；
- NautilusTrader 节点管理；
- TradingNode、Recovery、Heartbeat、Execution Risk；
- Venue adapter、钱包和资金操作；
- 对下游系统的启动、停止或远程控制。

---

## 2. 系统 Ownership

### 2.1 QuaZonai 是业务工作流 Owner

已选：**QuaZonai 拥有 Research、Mission、Evidence、Candidate、Approval 和 Handoff 状态；Codex App Server 是内置自治研究运行时。**

```text
用户
  → QuaZonai Web
  → QuaZonai Research / Portfolio Orchestrator
  → Codex App Server
  → QuaZonai Research Engine
  → Candidate Package / Handoff Registry
  → 独立下游系统
```

- PostgreSQL 是业务事实源；
- Codex Thread 只保存 Mission 上下文和执行活动；
- Codex Thread、Turn、Item 不得成为 Research 或 Approval 的事实源；
- Codex 输出必须经 QuaZonai 领域校验后才能推进正式状态。

### 2.2 Codex Harness 接入方式

- 采用 `codex app-server`，不采用 `codex exec` 作为产品主接口；
- App Server 负责 Thread、Turn、Item、流式事件和工具执行；
- 前端不直接呈现为通用 Codex 聊天应用；
- Codex Mission 的代码工作区与 QuaZonai Core 仓库隔离；
- 业务 Tool 使用结构化合同，不以 Shell 拼接 `quazonai` CLI 作为 Agent RPC；
- CLI 保留给本地人工运维和应急操作。

---

## 3. 人类操作承诺

正常 Research Program 生命周期只有两类人工操作：

1. **提出 Research Idea**；
2. **审批系统推荐的 Paper / Live 候选**。

以下不计入普通 Research Program 的常规旅程：

- 首次部署与系统初始化；
- 数据、Codex 认证和下游系统配置；
- Credential 更新；
- 故障、紧急暂停和归档；
- Admin 运维。

### 3.1 Idea Composer

已选：**有限、条件触发的澄清**。

```text
自然语言 Idea
→ 识别重大歧义
→ 必要时一次性提出 1–3 个问题
→ 生成 Charter 摘要
→ 用户点击“开始研究”
→ Charter 冻结
```

只追问会改变产品边界的问题：

- 市场或资产域；
- 核心可检验命题；
- Horizon；
- 允许的数据域；
- 明确禁止范围。

不把模型、特征、验证、优化器、参数和算力配置交给用户。

### 3.2 Research Charter

用户 Idea 是不可变研究章程。Codex 可以在章程内派生有 lineage 的子假设，但不能：

- 改写原始 Idea；
- 跨市场、数据域或风险域漂移；
- 删除不利证据；
- 把失败 Branch 改名为新 Idea；
- 绕过独立评估。

---

## 4. 持续自治研究池

已选：**Research Pool 长期存在，Program 可无限期研究；Mission 必须有限。**

```text
Research Pool
  └── Research Program
        ├── Research Branch
        │     └── Research Mission
        │           └── Codex Thread
        ├── Evaluation Episode
        ├── Alpha Candidate
        └── Portfolio Candidate
```

### 4.1 不设置累计预算上限

不设置：

- Token 总额；
- CPU-hours 总额；
- Program 实验总数；
- Mission 总数；
- Branch 总数；
- 因成本触发的 `EXHAUSTED`。

仍必须保留：

- 物理容量与并发控制；
- 有限 Mission；
- 公平调度；
- 重复研究拒绝；
- Search Ledger；
- Evidence Exposure；
- Sealed Episode 约束。

资源消耗持续记录用于观察、诊断和容量规划，但不作为停止条件或研究质量证据。

### 4.2 Cooling

已选：**没有新信息时自动 `COOLING`，由事件自动唤醒。**

进入 Cooling 的典型原因：

- 无信息增量；
- Discovery 空间饱和；
- 无新颖假设；
- 校准或组合贡献进入平台期；
- 等待新数据、Sealed Episode 或下游反馈。

唤醒事件：

- 新市场数据；
- 新 Forward Feedback；
- 新 Sealed Episode；
- 新数据或研究能力；
- Regime 变化；
- 已有 Alpha 退化；
- Portfolio Gap；
- 新假设通过 Novelty Gate。

Cooling 不重置 Search Ledger 或 Evidence Exposure。

---

## 5. Codex 研究工作区

已选技术基线：**每 Program 一个 QuaZonai 管理的私有 Bare Git Repository；每 Research Branch 一条持久分支；每 Mission 一个临时独占 Worktree。**

```text
Program bare repo
  ├── Branch A
  │     ├── Mission A1 temporary worktree
  │     └── Mission A2 temporary worktree
  └── Branch B
        └── Mission B1 temporary worktree
```

- QuaZonai 拥有 Branch Lease、Worktree 创建/删除、接受修改、Commit、Revision 和 Strategy/Alpha 发布；
- Codex 只写 Mission Worktree 内普通文件；
- Codex 不创建 Branch、Commit、Merge、Rebase 或修改 Git 元数据；
- Git 只提供开发辅助历史，不作为 Candidate、Approval、幂等、完整性或发布 Gate；
- 禁止应用级 SHA、checksum、digest、fingerprint 和内容寻址身份。

Mission 成功后：验证 → 接受修改 → QuaZonai Commit → 业务 Revision 增加 → 删除 Worktree。Mission 失败或崩溃时保留正式业务证据，但临时 Worktree 可重建或清理。

---

## 6. 独立 Research Engine

已选：**QuaZonai 拥有与 Nautilus 完全无关的 Canonical Research Engine。**

技术基线默认采用推荐方案：

- Canonical 数据：Arrow / Parquet；
- 特征计算：Polars Lazy；
- 确定性研究仿真：QuaZonai 自有最小向量化 Evaluator；
- 搜索与多目标优化：Optuna；
- Portfolio 优化：CVXPY 为底层，显式 Policy Family；
- 第三方研究库只能作为可选实验适配器，不成为业务事实源。

Research Engine 只模拟：

- 信号、目标暴露、目标权重；
- 再平衡；
- 成本、滑点和容量假设；
- 收益、风险和组合行为。

不模拟：

- Broker Session；
- Venue Order Lifecycle；
- Partial Fill；
- Credential；
- 交易恢复和真实账户状态。

---

## 7. Alpha 与 Portfolio 两阶段合同

已选：**Alpha Model + Portfolio Policy**。

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

### 7.1 Alpha 双模式与校准层

已选：**双模式 Alpha + 显式 Calibration**。

- `RELATIVE_SCORE`：只承诺相对排序，禁止直接冒充预期收益；
- `CALIBRATED_RETURN`：只有经独立校准检验后才能输出经济量纲的 expected return 和 uncertainty；
- `AlphaCalibrationVersion` 固定训练 Episode、方法、参数、Horizon、Universe 和适用 Regime；
- Portfolio Policy 必须声明接受相对信号还是校准收益。

### 7.2 全局 Alpha Library

已选：**合格 Alpha 进入全局、版本化 Alpha Library，并由独立 Portfolio Program 跨 Research Program 组合。**

Library 条目不可原地修改。新 Alpha 版本不会静默替换已有 Portfolio Candidate 的版本引用。

### 7.3 双通道准入

已选：

1. `PRIMARY_ALPHA`：通过 Standalone Quality Gate；
2. `DIVERSIFIER_ALPHA` / `HEDGE_ALPHA` / `REGIME_SIGNAL` / `RISK_MODULATOR`：通过 Portfolio Contribution Gate。

未证明的实验 Alpha 只留在原始 Research Program 内，不进入全局组合搜索空间。

---

## 8. Portfolio Assembly

已选：**分阶段、角色约束、可审计的 Portfolio Assembly Pipeline。**

```text
Alpha Library
→ Eligibility Snapshot
→ Role Pooling
→ Redundancy / Common-source Clustering
→ Portfolio Skeleton
→ 有限 Policy Family
→ Discovery Evaluation
→ Robustness / Marginal Contribution
→ Portfolio Candidate Family
→ Portfolio-level Sealed Evaluation
```

- Eligibility Snapshot 冻结 Alpha 与 Calibration 版本；
- 不允许 Library 新版本静默进入当前 Program；
- 冗余判断综合输出、收益、回撤、Lineage、数据和 Feature 依赖；
- 不使用源码 Hash 或 Fingerprint 做重复判断；
- 第一版 Policy Family 为明确版本化集合，如 Equal Weight、Volatility Scaling、Risk Parity、HRP、Constrained Mean-Variance、Mean-CVaR；
- 每次 Alpha 子集、角色、Policy、约束和再平衡尝试均进入 Portfolio Search Ledger；
- 单 Alpha 通过不代表组合自动通过，Portfolio Candidate 必须有新的组合级独立评估。

---

## 9. 独立评估与证据保护

已选：**分层、可消耗、滚动 Evaluation Episode。**

### 9.1 三层数据

1. `Discovery Zone`：Codex 可见，用于特征、模型、校准和组合研究；
2. `Sealed Promotion Zone`：Codex 不可访问原始数据，由独立 Evaluator 执行；
3. `Forward Evidence`：策略形成后到达的新市场数据和下游反馈。

Evaluation Episode：

```text
PLANNED
→ SEALED
→ ASSIGNED
→ EVALUATING
→ EVALUATED
→ DISCLOSED
→ CONSUMED
```

披露后永久 `CONSUMED`，不能作为同一研究家族后代候选的独立证据。

### 9.2 Evidence Exposure Graph

Exposure 沿 Program → Branch → Mission → Candidate → Portfolio Candidate lineage 继承。不能通过改名、新 Branch、新 Thread 或复制 Program 重置已见证据。

### 9.3 Search Ledger

必须保存所有 Feature、Alpha、Calibration、参数、Portfolio Policy、组合和 Promotion 尝试，不只保存赢家。Search-adjusted Evidence 同时考虑 Alpha 与 Portfolio 两层搜索。

### 9.4 分级最小披露

已选：

- Level 0：Evaluator 私有完整结果；
- Level 1：Codex 只收到固定类别和低分辨率裁决；
- Level 2：Human Approval 聚合报告；
- Level 3：Program 关闭或 Episode 永久退出后的人类复盘。

Level 1 禁止返回具体失败日期、Instrument、精确阈值差距、逐期收益或参数修改方向。Disclosure Policy 必须是确定性映射，不允许 LLM 自由总结 Sealed 明细。

---

## 10. Candidate 选择与审批

### 10.1 唯一推荐候选

已选：**每张 Approval 只对应一个系统推荐的不可变候选。**

用户可以查看只读对比和淘汰原因，但不能在审批页改选第二名、改权重或修改约束。拒绝后不能自动递补第二名。

### 10.2 Paper 与 Live 分开审批

已选：

```text
Portfolio Candidate
→ PAPER_HANDOFF_APPROVAL
→ 下游 Paper Feedback
→ Fresh Re-evaluation
→ LIVE_HANDOFF_APPROVAL
```

Paper 批准不预授权未来 Live。两者分别冻结 Candidate Package、证据、下游系统和审批报告。

### 10.3 结构化拒绝

已选：**固定拒绝原因 + 可选说明**。

拒绝反馈可以触发新 Mission 或等待新证据，但不能：

- 改写 Research Charter；
- 原地修改 Candidate；
- 绕过重新评估；
- 泄漏 Level 2 Sealed 明细给 Codex。

### 10.4 审批节流与实质改善门槛

已选：**全局 Approval Throttling + Material Improvement Gate。**

只有候选同时满足以下条件时才进入 Approval Inbox：

- 全部 Promotion Gate 通过；
- 是 Candidate Family 的唯一推荐候选；
- 相比现有最佳候选或已批准组合具有实质改善；
- 与其他待审批候选不重复；
- 同一 Program 没有未处理审批；
- 证据成熟度达标；
- 目标下游兼容性预检通过。

固定规则：

- 同一 Program 同时最多一个待审批；
- 同一 Candidate Family 未处理前不提交替代版本；
- 没有实质改善的版本不打扰用户；
- Paper Forward Evidence 不足时不生成 Live Approval；
- 用户拒绝后必须有新证据或实质改变，不能立即提交第二名。

Research Pool 可以无限地产生内部 Candidate，但 Approval Inbox 只接收具有实际决策价值的唯一推荐候选。

---

## 11. Candidate Package 与下游分离

已选：**标准化、可执行、下游无关的 Candidate Package；V1 提供 Python Reference Runtime。**

### 11.1 合同

```text
Market / Feature Data
→ Feature Pipeline
→ Alpha Model
→ Calibration
→ Portfolio Policy
→ TargetPortfolioFrame
```

`TargetPortfolioFrame` 表达目标权重、有效时间和置信度，不表达订单、Venue、Credential、Order Type 或执行重试。

### 11.2 物理格式

已选技术基线：

```text
Contract Bundle
+ fixed Python Wheels
+ Arrow Reference Fixtures
```

Package 包含 manifest、schemas、feature/alpha/calibration/portfolio wheels、state schema、reference fixtures、evidence summary 和 lineage。禁止把 Package 变成 Nautilus Strategy 或执行系统。

### 11.3 Handoff Registry

已选：**QuaZonai 发布不可变 Package，下游 Consumer 主动领取并回传 Forward Feedback。**

```text
APPROVED
→ PUBLISHING
→ AVAILABLE
→ CLAIMED
→ DOWNSTREAM_ACCEPTED / DOWNSTREAM_REJECTED
→ FEEDBACK_PENDING
→ FEEDBACK_RECEIVED
```

QuaZonai 不拥有下游 Deployment 状态，也不远程控制下游启停。

### 11.4 下游目标选择

已选：**用户在审批页选择逻辑下游系统**，例如 `Nautilus Paper Lab`、`Nautilus Live Primary` 或其他 Consumer；不暴露机器、IP、容器或节点实例。

- 一个 Approval Snapshot 固定一个下游系统和用途；
- 下游拒绝后不自动切换第二个系统；
- 同一 Candidate 交付不同系统需要分别审批；
- Paper Approval 不能自动改投 Live 系统。

---

## 12. Web 产品

已选：**单用户、自托管、私有工作台。**

不建设 tenant、organization、workspace、RBAC、团队共享、多人会签或 SaaS 计费。

### 12.1 Dashboard-first

主导航：

```text
Idea Composer
Research Observatory
Alpha Library
Portfolio Lab
Approval Inbox
Administration
```

- Idea Composer 是唯一正常的对话式输入；
- Charter 提交后不允许通过聊天随意干预研究；
- Research Observatory 只读展示 Codex 与研究活动；
- Alpha Library 和 Portfolio Lab 不允许普通用户手工选择 Alpha 或调权；
- Approval Inbox 只提供批准、拒绝、结构化原因和下游系统选择；
- Admin 保存低频初始化和故障操作。

### 12.2 分层透明度

已选：

- Level 1：默认研究摘要；
- Level 2：Research Charter、Branch lineage、Mission、Evaluation、Search Ledger 和 Evidence；
- Level 3：代码 Diff、测试、Tool 调用、命令退出状态和 Codex Item 时间线。

不展示或保存为产品事实：模型隐藏思维链、Secret、Token、Private Key 和未经验证的内部推测。

---

## 13. 已确认的技术决策默认规则

用户已授权：后续纯技术问题默认采用经过调研后推荐的方案，不再逐项询问；只有会显著改变产品能力、成本、安全边界或产生不可逆技术锁定时才单独提出。

继续共同讨论的是：

- 用户体验；
- 产品承诺；
- 业务状态；
- 人工介入边界；
- 审批与披露语义；
- 自动化行为和产品范围。

---

## 14. 待继续 Grill-Me 的产品问题

本文件记录到：**全局审批节流 + Material Improvement Gate 已确认。**

后续需继续确定的产品问题包括但不限于：

- 新 Idea 与现有 Program 高度重叠时如何处理；
- 用户是否可以暂停、恢复或归档 Program；
- 下游反馈不足或长期不返回时如何呈现；
- Paper / Live Approval 的有效期与过期语义；
- 用户是否允许撤回尚未被领取的 Handoff；
- Research Program 与 Portfolio Program 的首页优先级；
- 首次安装向导的产品范围。
