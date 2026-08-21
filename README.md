# QuantFoundry

QuantFoundry 是一个 API-only、单机、单操作员的量化研究与实盘工作台。NautilusTrader 是唯一交易内核；QuantFoundry 负责研究、策略、运行时插件、优化、审批、部署、恢复编排和中央风险最小投影。

> **当前状态：P0/P1 Core Foundation 已建立，尚未 `conforming`、`release-ready` 或 `live-ready`。**
>
> 当前提交完成了旧系统清场、新包结构、Fresh Schema、Core API、插件生命周期基础、持久 Job/Event 原语、本地 CLI、Worker/Supervisor 骨架和 Native Risk 算术。运行时插件安装、Nautilus factory、研究回测、Polymarket 实盘、完整 MCP Edge 和真实资金验收仍未完成。

## 当前已实现

```text
Core API
  ├── /api/v1/system/health
  ├── /api/v1/openapi.json
  ├── Plugin catalog / release read API
  └── Plugin activate / deactivate transaction

PostgreSQL
  ├── 单一 0001_initial Fresh Schema
  ├── Plugin / Control Plane / Job / Event / Risk tables
  └── Idempotency / Agent Artifact / MCP task projection tables

Runtime
  ├── finite-worker 基础 durable job claim
  ├── live-supervisor observation-only 骨架
  ├── qf 本地 CLI：status 与 plugin read/lifecycle
  └── qf_nautilus_risk：整数 micro-pUSD gross reservation 算术
```

代码和配置不使用 SHA-256、内容哈希、checksum、digest 或 fingerprint 作为校验、身份、状态判断或发布门槛。

## 目标工作流

```text
Runtime Plugin Install / Activate
  → Credential / Data Source / Execution Connection
  → Dataset
  → Research + Strategy Version
  → Nautilus BacktestNode / Optuna NSGA-II
  → Deterministic Pareto Candidate
  → Single Holdout
  → Human Approval
  → Recovery
  → Armed
  → Polymarket Trading
```

## Core 运行拓扑

```text
Local operator
  → qf CLI
  → QF API 127.0.0.1:8000

QF API
  ├── PostgreSQL
  ├── finite-worker
  └── live-supervisor

Optional remote AI edge（尚未实现）
  → HTTPS MCP
  → qf-mcp-gateway
  → internal QF API
```

Core Compose 只包含：

```text
postgres
migrate
api
finite-worker
live-supervisor
```

## 本地启动

生成本地 Master Key：

```bash
python -c 'import base64,secrets; print(base64.b64encode(secrets.token_bytes(32)).decode())'
```

复制环境文件并设置 `QF_MASTER_KEY`：

```bash
cp .env.example .env
$EDITOR .env
```

启动：

```bash
docker compose --env-file .env up --build
qf status
```

API 只发布到：

```text
http://127.0.0.1:8000
```

## 开发检查

```bash
make install-dev
make compile
make lint
make typecheck
make test
make verify-compose
```

Rust 工具链可用时：

```bash
make rust-format
make rust-lint
make rust-test
```

## 文档

- [统一产品与技术架构设计](DESIGN.md)：唯一完整事实源。
- [用户运行操作模型](OPERATIONS.md)：运行角色、人工节点和自动化边界。
- [CLI、MCP Gateway 与远程 Agent Skill](CLI.md)：本地 CLI 与可选远程 Agent Edge。
- [QuantFoundry Skill](skills/quantfoundry/SKILL.md)：外部 Agent 运行工作流。
- [Agent 开发治理](AGENTS.md)：项目围栏、变更顺序和验收要求。

## 许可证

QuantFoundry 使用 [AGPL-3.0-only](LICENSE)。第三方组件遵循各自许可证，见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)。
