# QuaZonai 用户运行操作模型

> 本文件是 [`DESIGN.md`](DESIGN.md) 的用户运行视图，不定义新的产品状态或技术事实；冲突时以 `DESIGN.md` 为准。

## 1. 用户真正需要做什么

QuaZonai V1 是单用户、自托管私有工作台。正常 Research Program 生命周期只有两类常规人工操作：

1. **提出 Research Idea**；
2. **审批系统推荐的 Paper / Live Candidate Handoff**。

其余动作都属于低频 Administration 或故障处置，不应成为每个 Program 的必经步骤。

完整用户视图：

```text
提出 Idea
→ 系统自治研究
→ 系统构建 Alpha / Portfolio
→ 有实质价值时通知审批
→ 用户批准或拒绝
→ 系统发布 Handoff Package
→ 独立下游运行
→ QZ 接收 Forward Feedback
→ 系统自动继续研究或进入 Cooling
```

## 2. 运行责任帽子

系统只有一个人类操作者，但不同场景承担不同责任帽子。

### 2.1 Research Operator

常规操作：

- 提出 Idea；
- 必要时回答一次澄清；
- 查看 Research Pulse / Observatory；
- 审批唯一推荐 Candidate。

不需要：

- 手工选模型；
- 手工选 Alpha；
- 手工调权；
- 手工运行回测；
- 手工选择第二名候选；
- 手工管理 Codex Mission。

### 2.2 Administrator

低频操作：

- 完成首次 `RESEARCH_READY`；
- Codex 登录/认证；
- 在 Runtime Configuration 配置 Codex provider/model 与 Worker limits；
- 配置 Data Source / Universe / Mandate / Capital Context；
- 配置 Paper/Live downstream；
- 安装/激活/停用 research/data/handoff plugin；
- 处理 storage、worker、evaluator、connector 故障；
- Pause/Resume/Archive/Restore Program。

### 2.3 QuaZonai Automation

自动负责：

- Charter 解析与 overlap detection；
- Mission Graph / scheduling；
- Codex Mission execution；
- Data acquisition 与 quality validation；
- Feature/Alpha/Calibration research；
- Search Ledger / Evidence Exposure；
- Sealed Promotion Evaluation；
- Alpha Library qualification；
- Portfolio Assembly；
- Material Improvement Gate；
- Approval freshness；
- Candidate Package；
- Handoff state；
- Feedback validation；
- Degradation Monitoring / Research Wake-up。

### 2.4 Independent Downstream

独立下游负责：

- Paper/Live runtime；
- market/broker connectivity；
- orders/fills/positions/accounts/NAV；
- execution risk；
- runtime stop/recovery；
- 把约定的 Feedback Package 返回 QZ。

QZ 不拥有这些状态。

## 3. 首页

Home = `Action Center + Research Pulse`。

### 3.1 Action Center

仅在需要人时出现：

- Paper / Live Approval；
- Approval 即将过期；
- Idea 澄清未完成；
- 必须人工处理的关键 Admin 事件。

优先级：

```text
资金交付审批
→ Idea 澄清
→ 证据/系统完整性 Admin 事件
→ 其他信息
```

### 3.2 Research Pulse

无人工任务时首页展示：

- ACTIVE / COOLING / PAUSED / BLOCKED Programs；
- 最近晋级 Alpha；
- Portfolio readiness；
- 等待 downstream feedback 的候选；
- 最近 material evidence changes。

Token、命令数、trial 数不展示为研究成果。

## 4. Idea Composer

### N0：提交 Idea

用户输入自然语言 Idea。

系统自动解析：

- Research Question；
- Market / Universe；
- Horizon；
- Data Domain；
- Explicit Exclusions；
- 与已有 Program 重叠程度。

若存在重大歧义，最多一次提出 1–3 个问题。

### N1：重叠处理

系统可能建议：

- 复用已有 Program；
- 在原 Program 创建 Branch；
- 创建关联的新 Program。

用户可以在 Idea 提交阶段接受或坚持独立创建；这不增加第三类常规操作。

### N2：冻结 Charter

点击 `Start Research` 后 Charter 永久冻结。之后普通用户不能用聊天不断改变研究方向。

## 5. Autonomous Research

### N3：Program ACTIVE

系统自动创建有限 Mission：

```text
Planning
Data Requirement
Data Quality
Hypothesis
Feature Research
Alpha Discovery
Calibration
Robustness
Promotion Review
```

Codex 运行状态只在 Observatory 展示，不需要用户确认 shell/网络权限。

### N4：COOLING

当没有信息增量时自动进入 `COOLING`，等待：

- 新市场数据；
- 新 Dataset Revision；
- 新 Forward Feedback；
- 新 Sealed Episode；
- Alpha/Portfolio degradation；
- 新 data capability；
- 新颖 hypothesis。

Cooling 是正常状态，不是失败。

### N5：BLOCKED

典型：

- 缺少必要 Data Capability；
- Codex 不可用；
- Sealed Evaluator 不可用；
- 关键 storage 不可用。

若是需要新增 license/credential 的数据源，只在 Administration 创建任务，不反复中断普通 Research workflow。

## 6. Alpha Library

用户主要是查看，不需要手工管理研究资产。

可见状态：

```text
SHADOW
ACTIVE
WATCH
QUARANTINED
RETIRED
```

旧 Qualification 不会“恢复”。若失效 Alpha 后来重新成立，系统创建新的 Qualification Version。

Shadow Alpha 可以参与受限组合贡献研究，不能直接用于 Handoff。

## 7. Portfolio Lab

系统基于启用的 Portfolio Mandate 和 Alpha Library 自动决定何时创建 Portfolio Program。

普通用户不：

- 手工拖入 Alpha；
- 改 Candidate 权重；
- 改优化器；
- 在 Approval 时改变 Mandate。

Portfolio Lab 用来解释：

- 哪个 Mandate；
- 哪些 Alpha roles；
- redundancy / marginal contribution；
- multi-universe exposures；
- risk/cost/capacity；
- 为什么当前 Candidate 是推荐版本。

## 8. Candidate Approval

### N6：Approval Inbox

只有通过全部 Promotion Gate + Material Improvement Gate 的唯一推荐 Candidate 才进入。

每张卡显示：

```text
Paper / Live purpose
Mandate Version
Capital Context
Candidate Version
Evidence Summary
Risk / Drawdown / Turnover / Cost / Capacity
Selected logical downstream
valid_until
freshness state
```

系统只展示兼容的 downstream。

### N7：Approve

Paper：允许把当前 Package 交给指定 Paper consumer。

Live：必须基于完整 Paper Forward Evidence 和新的 Promotion Evaluation 单独生成；Paper Approval 不预授权 Live。

Approve 后系统创建 Handoff Offer。

### N8：Reject

选择结构化 reason code，可附 note。

Reject 后：

- 不自动递补第二候选；
- 不改写 Charter；
- 必须有新证据/实质改进才可能再次出现 Approval。

## 9. Approval Freshness

`PENDING` Approval 可能变为：

- `STALE`：已知关键依赖变化；
- `EXPIRED`：超过最长有效期。

二者都只读，不能继续批准，也不能简单延长；系统需要基于当前事实重新生成新 Snapshot。

## 10. Handoff

### N9：AVAILABLE

下游尚未领取时，用户可以 `Withdraw offer`：

```text
PUBLISHING → REVOKED
AVAILABLE  → REVOKED
```

撤回不判 Candidate 失败，也不删除 Approval。

### N10：CLAIMED

一旦下游领取：

- QZ 不再提供 revoke runtime；
- QZ 不提供 stop/undeploy/close position；
- 用户必须在独立 downstream 处理运行与资金操作。

QZ 可以发布 `WithdrawalAdvisory` / `DegradationAdvisory`，但不声称下游已停止。

## 11. Feedback

正常：

```text
FEEDBACK_PENDING
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
```

这些异常不自动等于 Candidate 失败。

只有 complete + contract-valid 的 Paper Feedback 才能进入 Live Promotion。

## 12. Degradation

QZ 监控研究有效性，不监控订单执行。

```text
HEALTHY → WATCH → DEGRADING → INVALIDATED
```

达到 Degradation Policy 门槛时自动 wake Research Program，创建诊断 Mission，产生新 Qualification/Candidate。

不会自动：

- 停止下游；
- 平仓；
- 调仓；
- 替换已批准 Package。

## 13. Pause / Resume / Archive

### Pause

- 停止创建新 Mission；
- 不响应自动 wake；
- 保留所有研究事实；
- 不影响 downstream runtime。

### Resume

系统重新判断：

```text
ACTIVE / COOLING / BLOCKED
```

不重置 Search Ledger、Exposure 或 consumed Episodes。

### Archive

退出 active research pool，不再自动研究或生成 Approval；历史、Alpha、Package、Handoff、Feedback 全保留。

### Restore

恢复后继续继承全部历史研究负担，绝不当作 fresh Program。

## 14. Administration

### 14.1 Readiness

分别显示：

```text
System
Research
Paper handoff
Live handoff
```

首次安装只要求达到 `RESEARCH_READY`。

### 14.2 Codex / Runtime Configuration

Administration 是 Codex runtime 配置的事实入口，显示并允许修改：

- Codex executable/version 与 login 状态；
- 可选 `model`；
- 可选自定义 OpenAI-compatible `Base URL`；
- 可选 Codex API key；API key 只写、永不回显；
- App Server preflight；
- Agent worker health。

自定义 Base URL 必须是绝对 HTTP(S) URL，不能把 username/password、query token 或 fragment 嵌入 URL。配置了 Base URL/API key 时，Mission 使用独立 Codex model provider；未配置时继续使用持久 `CODEX_HOME` 中的标准 Codex 登录。

Codex API key 由 `QUAZONAI_MASTER_KEY` 使用 AES-256-GCM 加密后保存到 PostgreSQL。Secret/token 不在 Web 展示，也不写入事件 payload。

`.env` 只负责启动级基础设施：运行环境、PostgreSQL、master key、存储根目录和 HTTP port。Codex model/API key/Base URL 不再由 `.env` 配置。

### 14.3 Worker limits

以下运行参数在 Runtime Configuration 中由管理员维护，而不是 `.env`：

- plugin wheel 最大字节数；
- plugin validation timeout；
- runtime bundle build timeout；
- plugin job timeout；
- research Mission job timeout；
- job poll interval；
- job lease duration。

Worker 在领取后续 job 时读取最新配置；修改这些值不要求重建 Compose stack，也不改变已经运行中的 child process 的既定 deadline。

### 14.4 Data

管理员配置 approved Data Sources/Connectors。Codex 只调用批准的数据能力。

### 14.5 Universe / Mandate

Universe 和 Mandate 是长期配置。首次启用默认 Mandate；其他模板按需启用。

### 14.6 Capital Context

可以由 Administration 或 downstream feedback 提供现实资金规模快照；QZ 不读取 broker account/positions。

### 14.7 Downstream

配置逻辑系统及连接，例如：

```text
Nautilus Paper Lab
Nautilus Live Primary
External Validator
```

QZ 只验证 Handoff/Feedback contract，不检查其交易节点内部状态。

### 14.8 Plugins

只允许 Data/Research/Handoff plugins。运行时 side-by-side install/activate/drain/remove，不允许 execution broker plugins。

## 15. 故障呈现原则

产品必须区分：

- Research failure；
- Data quality failure；
- Codex/runtime failure；
- Sealed evaluator failure；
- downstream operational failure；
- negative market evidence。

不能用一个 `FAILED` 混合所有问题。

首页只提升需要人工处置的关键问题；低级 retry/command failure 留在 Level 2/3 Observatory。

## 16. 用户不应看到的内部实现

普通工作流不暴露：

- PostgreSQL row / job lease；
- Codex Thread ID；
- worktree path；
- MCP transport；
- plugin venv path；
- Sealed raw metrics；
- secrets；
- model hidden reasoning。

这些只在必要的 Level 3/Admin diagnostics 中以安全形式出现。

---

QuaZonai 的产品体验应始终保持：**用户提出投资研究问题，系统自治完成研究与组合，只有真正需要资本决策时再把一个可解释、不可变、经过独立验证的 Candidate 交给用户审批。**
