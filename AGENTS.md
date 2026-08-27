# QuaZonai Agent 治理

本文件是开发 Agent 的最小治理入口。`DESIGN.md` 是产品、领域和技术架构的唯一完整事实源；`OPERATIONS.md`、`CLI.md`、`README.md` 和 Skill 只能展开对应视图，不能创造竞争事实。

## 1. 长期产品边界

一句话定义：

> **QuaZonai 决定研究什么、如何搜索、何时相信、是否批准；NautilusTrader 负责市场数据如何进入、策略如何运行、交易如何仿真、订单/仓位/PnL 如何形成，以及独立 Paper/Live runtime 如何执行。**

新增量化基础设施前，必须先判断 NautilusTrader 是否已经可靠提供；答案为“是”时默认复用，不在 QZ 重写平行机制。

### 1.1 QuaZonai ownership

QuaZonai 是 AI 自治量化研究与治理 Control Plane，只拥有：

- Idea、Research Charter、Program、Branch、Mission DAG；
- Codex Mission 编排、Search Ledger、Evidence Exposure；
- 数据来源、许可、版本、point-in-time 与 Catalog binding 治理元数据；
- Discovery / Sealed / Forward Evidence 隔离与披露策略；
- 多重检验、过拟合控制、Alpha Qualification / Alpha Library；
- Portfolio Mandate、Alpha selection、target-weight optimization；
- Portfolio Candidate、Material Improvement、Paper/Live 人工 Approval；
- Nautilus-native Candidate Bundle、Handoff Registry、Forward Evidence；
- Degradation Monitoring、Research Wake-up；
- Web/API/PostgreSQL/durable jobs/audit events。

### 1.2 NautilusTrader ownership

NautilusTrader 是 Canonical Quant Runtime，优先拥有：

- Instrument、Venue 与 Quote/Trade/Bar/OrderBook/CustomData 模型；
- loader、wrangler、adapter、ParquetDataCatalog；
- Actor、Strategy、Indicator；
- BacktestNode / BacktestEngine、事件时钟和事件排序；
- simulated venue、matching、order lifecycle；
- Fee / Fill / Latency / Funding / Margin / Account 模型；
- Cache、runtime Portfolio、positions、balances、realized/unrealized PnL；
- Execution RiskEngine；
- 单次 backtest 的 reports/statistics；
- 独立 Paper/Live 的 DataEngine、RiskEngine、ExecutionEngine、LiveNode；
- broker/exchange adapters、reconciliation、recovery。

## 2. 远程运行拓扑

用户运行的 NautilusTrader 是**远程独立实例**。QuaZonai Core 不通过 Python import、共享文件系统、Docker socket、子进程或 `localhost` 假设嵌入它。

正式边界：

```text
QuaZonai API / Agent Worker / PostgreSQL
        │ typed HTTP experiment contract
        ▼
Remote Nautilus Research Runtime
        │ structured raw run evidence
        ▼
QuaZonai Evaluation Governance
        │ sealed contract
        ▼
Remote Sealed Nautilus Runtime
        │ deterministic controlled disclosure
        ▼
Alpha / Portfolio / Approval / Candidate Bundle
        │ handoff
        ▼
Independent Nautilus Paper / Live Runtime
```

要求：

- Core 代码只依赖 `quant_runtime.RemoteNautilusQuantRuntime`；
- `nautilus_trader` 只允许在独立 remote-runtime entry point 和真实 integration tests 中 import；
- pinned version 为 `1.231.0`，升级必须经过完整兼容测试；
- Research 与 Sealed endpoint/token/catalog 必须可独立配置；
- runtime service credential 只进入受信 worker，不进入 Web/API、Codex child、事件、日志或 Bundle；
- Sealed endpoint 只返回 deterministic disclosure，不向 Codex 返回 raw sealed reports；
- runtime URL 不允许内嵌 username/password/query token/fragment。

## 3. 必须保留的执行安全边界

采用 Nautilus-first 不等于把实盘执行塞进 QZ Core：

- QZ API/Web/DB 不持有 broker/exchange credential；
- QZ 不提交、修改或撤销真实订单；
- QZ 不维护实时订单、成交、仓位、账户或 NAV 账本；
- QZ 不成为 OMS/EMS，不提供实时交易控制 UI；
- QZ 不启动、停止、flatten、recover 或 reconcile 下游 Paper/Live node；
- Paper/Live runtime 持有自己的 secrets，并独立承担 stop/cancel/flatten/recovery；
- QZ 只能批准、交付、接收反馈、发出 degradation/withdrawal advisory。

任何 `TradingNode`/`LiveNode`、broker adapter、真实订单控制或账户状态回流 QZ Core，均为阻断级架构违规。

## 4. Data / Cache / DB / MessageBus 边界

- Nautilus Catalog：canonical market data 与 Nautilus data types；
- QZ PostgreSQL：Dataset Revision 的 provider/license/catalog URI/type/instrument scope/time range/schema/quality/PIT 等治理元数据；
- Nautilus Cache/MessageBus：单个 quant runtime 内部市场、订单、仓位、账户和事件；
- QZ durable jobs/events：跨进程 Mission、Experiment、Approval、Handoff 和恢复；
- 不自建与 Nautilus 平行的行情文件格式、事件类型、撮合、PnL 或 order-risk ledger。

## 5. Research Mission 与受控实验

Codex 默认仍使用独立 app-server child、独占 worktree、`workspace-write`、network disabled、`approvalPolicy=never`。

Agent 可以写研究代码与 `experiment-contract.json`，但不能：

- 获得 runtime URL/token、数据库凭据、provider secret 或 broker credential；
- 直接调用远程 Nautilus 实例；
- 写 PostgreSQL；
- 读取 Sealed raw data/results；
- approve Candidate 或控制 Paper/Live runtime。

受信 Mission runner 负责：

1. 校验 Mission state/type/capability；
2. 校验 Dataset Revision 与 immutable Catalog binding；
3. 将 Agent contract 强制绑定 governed URI/type/instrument scope；
4. 持久化 `QuantExperiment`；
5. 写入成功和失败 Search Ledger；
6. 排入 remote Discovery Backtest；
7. Discovery 成功后创建独立 Sealed contract/job。

Agent 输出不能直接推进正式状态；必须经过 schema、catalog scope、runtime version、evidence 和 deterministic promotion gates。

## 6. Evidence / Alpha / Portfolio

- Discovery、Sealed、Forward Evidence 三层必须分离；
- 同一 pinned Nautilus version 和同一 Strategy artifact 可复用于 Backtest/Paper/Live；
- Independent 的核心是数据、token、进程和 disclosure 隔离，不是第二套模拟器；
- Sealed raw reports 不进入 Codex workspace；
- Alpha Qualification 必须引用真实 `QuantExperiment` / `SealedEvaluation`；
- 测试 seed、人工 DB 写入或文本 `RESULT.md` 不能冒充正式运行证据；
- QZ 负责 Alpha selection、Mandate、optimizer 与 target weights；
- target weights → rebalance/orders、positions/PnL/margin/order risk 由 Nautilus Strategy/runtime 承担；
- Portfolio Candidate 必须重新经过 Nautilus transaction-level simulation。

## 7. Nautilus-native Candidate Bundle

主要交付协议必须冻结真实 Nautilus runtime contract，而非 QZ 自建 Feature/Alpha/Calibration/Portfolio Policy 微型运行时。

最低布局：

```text
candidate/
  manifest.json
  requirements.lock
  strategy/strategy.whl
  strategy/strategy-config.json
  data/requirements.json
  data/instrument-scope.json
  runtime/nautilus-version.json
  runtime/backtest-run-config.json
  runtime/venue-config.json
  runtime/risk-config.json
  runtime/live-node-template.json
  validation/expected-orders.json
  validation/expected-positions.json
  validation/expected-statistics.json
  evidence/discovery-summary.json
  evidence/sealed-summary.json
  evidence/robustness-summary.json
  lineage.json
```

Bundle 禁止真实 broker/provider/runtime credential、private key、account secret、order-control endpoint。验证依赖 schema、显式版本、wheel metadata、fixture/report conformance，不新增应用级 hash/checksum/fingerprint gate。

## 8. Codex / Secret / Sealed hard deny

任何 Agent profile 都不能：

- approve/reject Candidate；
- publish Handoff；
- 修改 Charter/Mandate/Sealed result；
- 访问 provider/downstream/runtime secret；
- 访问 QZ DB、Docker socket、其他 Program 或 Sealed catalog；
- 管理 Git branch/commit/merge/rebase/worktree 绕过 Workspace Manager；
- 展示或持久化隐藏 chain-of-thought。

只保存可验证活动：Tool 调用、文件 diff、测试、命令结果、结构化结论、runtime evidence、Domain Event。

## 9. 文档优先顺序

```text
确认 DESIGN.md 事实
→ 画 QZ / Remote Nautilus / Downstream ownership 与 data flow
→ 更新 DESIGN.md
→ 同步 AGENTS / OPERATIONS / CLI / README / Skill
→ 实现最小正确改动
→ unit / DB / remote-contract / real Nautilus integration
→ Sealed non-leakage / Candidate conformance / vertical E2E
→ 独立 review
```

旧代码、旧 schema、旧 Candidate Package、旧测试 seed 和与新边界冲突的 compatibility wrapper 均无兼容义务；删除优先于双写或 wrapper。

## 10. 完成标准

代码任务只有满足以下全部条件才可声明完成：

- `DESIGN.md` 已先定义最终行为，其他文档无冲突；
- Core 不 import/install NautilusTrader，remote runtime 精确 pin `1.231.0`；
- 至少一个真实 Catalog ingestion + BacktestNode integration test；
- orders/fills/positions/PnL/statistics 形成结构化 evidence；
- Discovery 与 Sealed 使用独立 remote contract，Sealed 无 raw result 泄漏；
- Search Ledger 同时记录成功和失败；
- Alpha/Candidate/Approval 来源于真实 evidence；
- Candidate Bundle 为 Nautilus-native 且无 secrets；
- Paper/Live 仍是独立下游，QZ 无 runtime control；
- compile、lint、typecheck、PostgreSQL migration、unit/integration/E2E、Compose、两个镜像构建全部通过；
- GitHub CI 在最终 head 全绿；
- GitHub `@codex review` 对同一最终 head 明确无问题；
- 所有 review thread 已解决后才合并 main。
