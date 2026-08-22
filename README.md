# QuaZonai

QuaZonai 是一个 **由 Codex Harness 驱动、单用户、自托管的持续自治量化研究与策略组合工作台**。

它负责：

```text
Idea
→ Research Charter
→ Autonomous Research
→ Alpha Library
→ Portfolio Construction
→ Candidate Evaluation
→ Human Approval
→ Downstream-neutral Handoff
→ Forward Evidence
→ Degradation-driven Research Wake-up
```

它**不负责交易执行**：不管理 NautilusTrader/LEAN/其他交易节点，不保存 broker credential，不提交订单，不维护成交、仓位、账户或 NAV，也不控制下游 Paper/Live runtime。

> **当前实现状态：Architecture target 已完成重写，但现有代码仍主要来自旧的 Nautilus execution-control 架构，因此尚未 conforming / release-ready。**
>
> `design/codex-autonomy-prd` 当前工作的目的，是先把 PRD、领域模型、Codex Harness、Web、Research Engine、Evaluation、Portfolio、Approval 与 Handoff 方案收敛为唯一事实源，再按 `DESIGN.md` 的 P0–P7 实施路线替换旧代码。

## 产品承诺

正常 Research Program 生命周期中，人类只有两类常规操作：

1. 提出 Research Idea；
2. 审批系统推荐的 Paper / Live Candidate。

数据授权、Codex 登录、Universe/Mandate/Downstream/Plugin 配置、Pause/Archive 和故障处理属于低频 Administration。

## 目标架构

```text
Human
  ↓
QuaZonai Web
  ↓
FastAPI Domain/API
  ├─ PostgreSQL
  ├─ Worker
  ├─ Agent Worker
  │    └─ per-Mission codex app-server (stdio)
  │          ↕ mission-scoped stdio MCP
  └─ Sealed Evaluator

Research Engine
  ├─ Arrow / Parquet
  ├─ Polars
  ├─ Optuna
  └─ CVXPY

Portfolio Candidate
  ↓
Candidate Package
  ↓
Handoff Registry
  ↓
Independent Downstream Consumer
```

## Codex Harness

QuaZonai 把官方 `codex app-server` 作为内置自治研究运行时：

- 每个有限 Research Mission 一个独立 App Server child；
- 一个 Mission 对应一个 durable Codex Thread；
- 稳定主传输使用 stdio；
- Mission 默认只写自己的临时 Git worktree；
- 默认禁用任意网络访问；
- 数据、实验和研究资源通过 Mission-scoped stdio MCP Tool Server 提供；
- Codex Thread/Turn/Item 不是 QuaZonai 业务事实源；
- Agent 不能审批 Candidate、访问 Sealed raw data/Secret 或控制 downstream runtime。

## Research 与 Portfolio

Canonical Research Engine 与 Nautilus 完全独立：

```text
Feature Pipeline
→ Alpha Model
→ Calibration
→ Risk / Cost / Capacity
→ Portfolio Policy
→ TargetPortfolioFrame
```

技术基线：Arrow/Parquet、Polars Lazy、Optuna、CVXPY 与 QZ 自有确定性 target-weight evaluator。

Alpha Library 支持：

```text
PRIMARY_ALPHA
DIVERSIFIER_ALPHA
HEDGE_ALPHA
REGIME_SIGNAL
RISK_MODULATOR
SHADOW_ALPHA
```

Portfolio Program 绑定明确的 versioned Portfolio Mandate，支持 Multi-Universe Portfolio，并通过独立 Sealed Promotion Evaluation 与 Material Improvement Gate 决定是否生成审批。

## Candidate 与 Handoff

Portfolio Candidate 永久不可变。任何 Alpha、Mandate、Capital Context、权重规则、Risk/Cost/Capacity Model 或 Contract 的实质变化都创建新的 Candidate。

Candidate Package 是下游无关的 Python Reference Runtime + contract bundle + Arrow fixtures，只输出 `TargetPortfolioFrame`，不输出订单。

Handoff Offer 未被领取前可撤回；一旦下游 `CLAIMED`，QuaZonai 不再拥有停止、撤销或下线该独立 runtime 的权限。

## No custom hash gates

QuaZonai 不使用应用级 SHA、checksum、digest、fingerprint 或内容寻址身份作为：

- 业务 ID；
- Approval Gate；
- Package 发布 Gate；
- Plugin identity；
- Workspace revision；
- Idempotency；
- 完整性状态。

业务事实使用 UUID、version/revision、显式关系、schema、metadata、file size 与 Reference Fixture conformance。

## 目标运行拓扑

```text
postgres
migrate
api
worker
agent-worker
evaluator
```

Web SPA 在 production build 后由 FastAPI 提供静态文件，不增加独立 frontend runtime service。

## 文档

- [`DESIGN.md`](DESIGN.md) — **唯一完整 PRD + 领域 + 技术架构事实源**；
- [`OPERATIONS.md`](OPERATIONS.md) — 用户实际运行视图；
- [`CLI.md`](CLI.md) — CLI、Codex App Server 与 Mission MCP 详细合同；
- [`skills/quazonai/SKILL.md`](skills/quazonai/SKILL.md) — 可选外部 Codex/Agent 薄工作流；
- [`AGENTS.md`](AGENTS.md) — 开发治理和不可突破边界。

## 实施顺序

见 `DESIGN.md` P0–P7：

```text
P0 Governance / 删除旧 execution-control code
P1 Core Domain + Web Shell
P2 Codex Harness Runtime
P3 Data + Research Engine
P4 Independent Evaluation + Alpha Library
P5 Portfolio Construction
P6 Approval + Package + Handoff + Feedback
P7 Full Product Acceptance
```

在对应代码、集成测试和独立复核完成之前，不得把目标能力描述为已实现。

## 许可证

QuaZonai 使用 [AGPL-3.0-only](LICENSE)。第三方组件遵循各自许可证，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。