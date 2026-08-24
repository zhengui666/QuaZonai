# QuaZonai Agent 治理

本文件是开发 Agent 的最小治理入口。它定义事实源、不可突破边界、工作顺序和完成标准；产品与架构事实只在 `DESIGN.md` 定义。

## 1. 事实源与读取顺序

1. `DESIGN.md`：QuaZonai 唯一完整的产品、领域与技术架构事实源；
2. `OPERATIONS.md`：单用户运行视图；不得创造新的产品事实；
3. `CLI.md`：CLI、Codex App Server、Mission Tool 和外部 Skill 的实现展开；
4. `skills/quazonai/SKILL.md`：外部/人工 Codex 使用的薄工作流，不是业务状态机；
5. `README.md`：入口、当前实现状态、Quick Start 和文档链接；
6. 代码、配置、测试、运行结果：实现证据，不能静默改写设计。

任何跨层产品、领域、API、Agent Runtime、数据、Evaluation、Alpha、Portfolio、Approval、Package、Handoff 或插件变更前，先读取 `DESIGN.md` 对应章节。若设计没有明确行为，先更新 `DESIGN.md`。

## 2. 不可突破的产品边界

### 2.1 QuaZonai ownership

QuaZonai 只拥有：

- Idea / Research Charter；
- Research Program / Branch / Mission；
- Data Source Registry / Dataset Revision；
- Search Ledger / Evidence Exposure；
- Feature / Alpha / Calibration / Alpha Library；
- Portfolio Mandate / Portfolio Program / Portfolio Candidate；
- Independent Evaluation；
- Approval Snapshot；
- Candidate Package；
- Handoff Registry / Feedback / Forward Evidence；
- Degradation Monitoring；
- Codex Harness 研究运行时与 Web 工作台。

QuaZonai **不拥有**：

- broker/exchange credential；
- order、fill、position、account、NAV；
- TradingNode、Paper/Live runtime；
- execution risk、heartbeat、recovery、venue reconciliation；
- 下游启动/停止/撤单/平仓/强制下线。

NautilusTrader、LEAN 或自定义交易系统都是独立 downstream consumer。任何实现把 QZ 重新做成 Execution Control Plane 都是架构违规。

### 2.2 人类操作承诺

正常 Research Program 生命周期只允许两类常规人工动作：

1. 提出 Idea；
2. 审批系统推荐的 Paper/Live Handoff Candidate。

Pause/Resume/Archive/Restore、数据授权、Codex 登录、Mandate/Universe/Downstream/Plugin 配置和故障处理属于低频 Administration，不得被开发成每个 Program 的必经人工步骤。

### 2.3 不可变事实

以下正式对象不原地改写语义：

- Research Charter；
- Dataset Revision；
- Feature/Alpha/Calibration Version；
- Alpha Qualification Version；
- Evaluation Episode disclosure/exposure；
- Portfolio Mandate Version；
- Capital Context Version；
- Portfolio Candidate；
- Approval Snapshot；
- Candidate Package；
- Handoff Offer 的历史终态。

改变依赖就创建新 Version/Candidate/Snapshot，而不是 patch 旧事实。

### 2.4 No custom hash gates

不得新增应用级 SHA、hash、checksum、digest、fingerprint、内容寻址身份或以其为 Gate 的完整性流程。

允许 Git、wheel、数据库等底层工具自身使用内部 hash，但 QZ 业务身份、幂等、审批、发布、插件、Package、Workspace 或验证不得依赖这些值。

## 3. Codex Harness 边界

### 3.1 App Server

- 内置 Agent Runtime 使用官方 `codex app-server`；
- V1 稳定生产传输使用 stdio；
- experimental WebSocket、dynamicTools、project/environments 等不能成为 V1 必需能力；
- Codex Thread/Turn/Item 是执行上下文，不是业务事实源；
- 一个 Mission 对应一个 durable Thread；Program 不使用无限长 Thread。

### 3.2 Mission isolation

默认 Mission：

- 独立 Codex App Server child；
- 临时独占 Git worktree；
- `workspace-write`；
- network disabled；
- `approvalPolicy=never`；
- 只允许 Mission worktree root；
- 不访问 QZ source repo、其他 Program、Sealed data、Secrets、Docker socket 或数据库凭据。

需要数据、实验或受控外部能力时，通过 mission-scoped stdio MCP Tool Server。

### 3.3 Agent 不能做什么

任何 Codex profile 都不能：

- approve/reject Candidate；
- publish Handoff；
- 修改 Charter/Mandate/Sealed result；
- 访问 provider/downstream secret；
- 写 PostgreSQL；
- 读取 Sealed raw data；
- 控制 downstream runtime；
- 用 Git branch/commit/merge/rebase/worktree 管理绕过 QZ Workspace Manager。

Agent 输出必须通过 schema、artifact validation 和 Domain Validator 才能推进状态。

### 3.4 隐藏推理

不得把模型隐藏 chain-of-thought 当作产品事实、审计证据或 UI 内容。只保存可验证活动：Tool 调用、文件 diff、测试、命令结果、结构化结论、Domain Event。

## 4. Research / Evidence 边界

- Discovery、Sealed Promotion、Forward Evidence 三层必须分离；
- Sealed raw data 不进入 Codex workspace/MCP；
- Level 1 disclosure 由 deterministic policy 生成；
- Search Ledger 保存失败和被淘汰尝试；
- Evidence Exposure 沿 lineage 继承；
- Episode 一旦披露后不能重新作为该 lineage 的独立证据；
- 数据必须保留 point-in-time `available_at` 语义；
- Data Quality failure 不得偷换成 Alpha failure。

## 5. Alpha / Portfolio 边界

- Alpha 不发订单，只发 score/expected return + uncertainty；
- relative score 未校准时不得冒充 expected return；
- Alpha Qualification 绑定 Universe + Horizon；
- Shadow Alpha 只能参加受限 Portfolio Contribution research，不能直接 Handoff；
- Portfolio Program 必须绑定 Mandate Version；
- Candidate 任一关键依赖变化就创建新 Candidate；
- Multi-Universe Portfolio 必须使用 universe-specific cost/capacity 与 cross-universe risk；
- Material Improvement Gate 控制 Approval 噪音；
- 不允许在 Approval 页面手工改 Alpha、权重或 Mandate。

## 6. Handoff / Downstream 边界

- Candidate Package 只输出 TargetPortfolioFrame，不输出订单；
- Approval 绑定一个逻辑 downstream system；
- Paper 与 Live 分开审批；
- 未领取 Offer 可 revoke；`CLAIMED` 后 QZ 无 stop/revoke runtime 权限；
- 缺失/迟到/部分 feedback 不等于 Candidate failure；
- 只有 complete valid Paper feedback 才能进入 Live Promotion；
- Degradation 只能产生 Research wake/advisory，不自动换仓或停止交易。

## 7. Runtime Plugin 边界

插件只允许：

```text
DATA_CONNECTOR
DATA_TRANSFORM_ADAPTER
RESEARCH_ADAPTER
HANDOFF_CONNECTOR
```

禁止 broker/execution/order capability。

- 只接受 wheel；禁止 sdist/editable/Git URL/运行时源码编译；
- 每个 release side-by-side；已有资源固定具体 release；
- 第三方插件只在 validator/connector runner child 中 import；
- 长进程不得 import plugin；
- 动态卸载依赖进程退出，不使用 `reload()` 或 `sys.modules` 热替换；
- Secret 不暴露给 Codex。

## 8. 文档优先工作流

顺序固定：

```text
确认事实源
→ 画 ownership / data flow
→ 更新 DESIGN.md
→ 同步 OPERATIONS.md / CLI.md / README / Skill
→ 实现最小正确改动
→ 运行最窄有效验证
→ 跨边界验证
→ 独立复核
→ 汇总已验证/未验证项
```

不得让 README、OPERATIONS、CLI、Skill、代码注释或聊天记录成为竞争事实源。

## 9. 实现纪律

使用 Ponytail 原则：

1. 没有真实需求就删除；
2. 先复用平台和标准，不自建平行机制；
3. 删除优先于兼容 wrapper；
4. 状态、接口和抽象只为真实边界存在；
5. CLI 是薄客户端；
6. Agent Tool Server 只做 capability-enforced domain bridge，不复制业务状态机；
7. 能由 PostgreSQL transaction、Arrow、Polars、Optuna、CVXPY、MCP SDK、Codex App Server 可靠承担的，不重复造轮子；
8. 旧 Nautilus execution-control code 没有兼容义务，应删除而非迁移到新抽象里。

## 10. Ownership

- Frontend：展示和用户输入，不决定领域状态；
- API：wire validation、operator mutation、SSE、统一错误；
- Domain：全部状态机和业务 Gate；
- Orchestrator：Mission/Program scheduling、Cooling/Wake、Promotion；
- Agent Worker：Codex child/process/thread/worktree lifecycle；
- Mission Tool Server：按 MissionContract 暴露受限 MCP tools；
- Research Engine：Arrow/Polars/evaluator/Optuna/CVXPY；
- Sealed Evaluator：独立 Promotion Evaluation；
- Plugin Manager：data/handoff plugin release/runtime；
- Worker：data/plugin/package/handoff/degradation jobs；
- PostgreSQL：业务事实、jobs、events、Search Ledger、Exposure；
- Persistent volumes：datasets、artifacts、packages、plugin runtimes、Program repos；
- Downstream：运行、订单、仓位、账户和执行安全。

## 11. 验证纪律

每个检查前明确：

1. 要发现什么具体失败；
2. 失败会如何改变下一步。

最少层次：

1. 文档/术语/路径一致性；
2. 受影响模块 unit；
3. PostgreSQL transaction/integration；
4. process isolation；
5. Codex App Server / MCP contract；
6. Sealed non-leakage；
7. browser + fake downstream E2E；
8. 只有真实边界扩大时才跑更宽检查。

不能只用 mock 证明：

- Codex stdio protocol lifecycle；
- thread resume；
- Mission worktree isolation；
- MCP capability hard deny；
- Sealed raw data 不可达；
- PostgreSQL concurrency/idempotency；
- plugin wheel install/entry point/process isolation；
- Candidate Package Reference Fixture conformance；
- Handoff claim vs revoke 原子竞争；
- event replay / SSE reconnect。

## 12. 文档任务完成标准

- `DESIGN.md` 仍是唯一完整事实源；
- `OPERATIONS.md` 只写用户运行视图；
- `CLI.md` 只展开接口/Agent Runtime；
- `README.md` 不承诺未实现能力；
- Skill 不复制完整领域模型；
- 不再出现 QZ 管理 Nautilus execution/deployment/recovery/risk 的当前目标描述；
- Codex built-in runtime 与 optional external automation 区分清楚；
- 无应用级 hash gate；
- 术语、状态、路径、API、服务名一致；
- 独立 Documentation/Architecture review 无阻断意见。

## 13. 代码任务完成标准

交付前确认：

- 目标行为已先进入 `DESIGN.md`；
- 变更位于正确 ownership；
- 无 broker/order/position/deployment control 回流 QZ；
- Codex 无 Secret/Sealed/DB 越权；
- immutable versions、idempotency 和 expected revision 有测试；
- Search Ledger/Exposure 不因复制对象被重置；
- Candidate/Approval/Package 不被原地修改；
- downstream claim 后 QZ 不提供 runtime stop；
- plugin 不在长进程热加载/热卸载；
- 无新增应用级 hash/checksum/digest/fingerprint 业务逻辑；
- 实现报告与独立复核报告均已提交。

未满足任一项，只能标记为部分完成，不得宣称 conforming/release-ready。