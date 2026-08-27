from __future__ import annotations

import re
from pathlib import Path


def replace_section(text: str, start: str, end: str, replacement: str) -> str:
    pattern = re.compile(re.escape(start) + r".*?(?=" + re.escape(end) + r")", re.S)
    updated, count = pattern.subn(replacement.rstrip() + "\n\n", text, count=1)
    if count != 1:
        raise RuntimeError(f"section not found: {start!r} -> {end!r}")
    return updated


def update_design() -> None:
    path = Path("DESIGN.md")
    text = path.read_text(encoding="utf-8")
    text = re.sub(
        r"> 架构基线：[^\n]+",
        "> 架构基线：2026-08-27（Issue #22 Nautilus-first）  ",
        text,
        count=1,
    )
    text = re.sub(
        r"> 当前状态：[^\n]+",
        "> 当前状态：**Nautilus-first 远程量化运行架构为唯一目标边界；实现与验收以本文为准**",
        text,
        count=1,
    )
    old_summary = (
        "QuaZonai（QZ）只拥有两个核心领域：\n\n"
        "1. **Research Intelligence**：从自然语言 Idea 到可验证 Alpha；\n"
        "2. **Portfolio Construction**：把已验证 Alpha 映射到明确 Portfolio Mandate，形成可交付 Portfolio Candidate。\n\n"
        "QuaZonai **不拥有交易执行**。NautilusTrader、LEAN 或任何自定义执行系统均是独立下游 Consumer。QZ 不启动、停止、监控或恢复交易节点，不保存 broker credential，不提交订单，不维护订单、成交、仓位、账户或 NAV，不提供中央执行风险，不把下游状态伪装为自己的 Deployment 状态。"
    )
    new_summary = (
        "QuaZonai（QZ）是 **AI 自治量化研究与治理 Control Plane**，拥有 Research Intelligence、跨 run Evaluation Governance、Alpha/Portfolio 决策、Approval、Handoff 与 Forward Evidence。\n\n"
        "NautilusTrader 是 **Canonical Quant Runtime**。凡 NautilusTrader 已可靠提供的 Instrument/Market Data/Catalog、Strategy/Actor/Indicator、BacktestNode、event clock、matching、order lifecycle、Fee/Fill/Latency/Funding、Account/Position/PnL、交易级 Risk、Paper/Live runtime，QZ 默认直接复用而不重写。\n\n"
        "用户的 NautilusTrader 作为远程独立实例运行。QZ 通过类型化 HTTP experiment contract 调用 Remote Research Runtime 和独立 Sealed Runtime；Core 不 import NautilusTrader、不共享其文件系统、不启动本地 LiveNode，也不持有 broker credential、真实订单、成交、仓位、账户或 NAV。独立 Paper/Live runtime 仍由下游持有 secrets 并负责 stop/cancel/flatten/recovery。"
    )
    if old_summary not in text:
        raise RuntimeError("DESIGN executive summary baseline not found")
    text = text.replace(old_summary, new_summary, 1)

    section11 = """## 11. Canonical Quant Runtime：Remote NautilusTrader

QZ 不再自建平行的向量化交易模拟器。NautilusTrader `1.231.0` 是首个、默认且正式支持的 Canonical Quant Runtime。

### 11.1 Thin Quant Runtime Adapter

Core 只依赖薄合同：

```python
class QuantRuntime:
    def ingest(...): ...
    def validate_catalog(...): ...
    def run_backtest(...): ...
    def run_sealed_backtest(...): ...
    def build_candidate(...): ...
    def verify_candidate(...): ...
```

正式实现为 `RemoteNautilusQuantRuntime`。它只使用绝对 HTTP(S) endpoint、service Bearer credential、严格 Pydantic contract、timeout、idempotency key 和结构化错误；URL 禁止内嵌 username/password/query token/fragment。QZ API/Codex child 不获得 runtime credential。

### 11.2 远程独立实例

```text
QZ Control Plane
  API / PostgreSQL / Agent Worker / Durable Jobs
          |
          | typed HTTP experiment contract
          v
Remote Nautilus Research Runtime
  ParquetDataCatalog / Strategy / BacktestNode
  Simulated Venue / Matching / Fee / Fill / Latency
  Cache / Portfolio / Positions / PnL / Reports
          |
          | structured run evidence
          v
QZ Evaluation Governance
          |
          | sealed contract, separate endpoint/token/catalog
          v
Remote Sealed Nautilus Runtime
          |
          | deterministic controlled disclosure only
          v
Alpha / Portfolio / Approval / Candidate Bundle
```

Core production image不得安装 `nautilus_trader`。该依赖只存在于独立 remote-runtime image 与真实 integration tests。Research 和 Sealed runtime 可运行在另一台主机/集群；不得依赖共享本地路径、Docker socket、QZ 子进程或固定 `localhost`。

### 11.3 Nautilus-first data

Remote runtime 使用 Nautilus Instrument model、Quote/Trade/Bar/OrderBook/CustomData、loader/wrangler/adapter 与 `ParquetDataCatalog`。QZ PostgreSQL 只保存治理事实：

```text
dataset_revision_id
provider
source_license
catalog_uri
nautilus_data_type
instrument_scope
event_time_range
available_time_range
schema_revision
quality_result
point_in_time_result
runtime_name / runtime_version
ingested_at
```

Agent 只能引用已治理 Dataset Revision。受信 Mission runner 强制把 Agent contract 绑定到 immutable Catalog binding，并校验 instrument scope、quality 与 point-in-time 状态。

### 11.4 Research Mission 与真实实验

Codex 在隔离 worktree 中生成研究代码和 `experiment-contract.json`，但不获得 DB、runtime endpoint/token、Sealed data 或 broker credential。受信 runner 校验 Mission capability 和 contract 后创建 `QuantExperiment` 与 durable job。

Discovery run 返回 orders/fills/positions/account/PnL/statistics 结构化 evidence；成功和失败均追加 `SearchLedgerEntry`。Discovery 成功后创建新的 Sealed contract/run ID，并通过独立 endpoint/token/catalog 执行相同 pinned runtime 与 Strategy artifact。

### 11.5 Sealed disclosure

Sealed runtime 不向 Codex 返回 raw orders、逐期收益、日期、instrument failure 或阈值差距。它只返回 deterministic classification/disclosure。QZ 根据该 disclosure 创建不可变 `SealedEvaluation`；只有 PASS 才能产生 Alpha Qualification、Portfolio Candidate 和待人工 Paper Approval。

### 11.6 Portfolio 与 Risk

```text
Alpha selection / Mandate / target-weight optimization -> QuaZonai
Target weights -> rebalance / orders                   -> Nautilus Strategy
Positions / PnL / margin / execution risk              -> NautilusTrader
Research / promotion / overfitting risk                 -> QuaZonai
```

QZ 可用 CVXPY 等成熟 optimizer 补足 NautilusTrader 没有的组合优化能力，但最终 Candidate 必须重新通过 Nautilus transaction-level simulation。
"""
    text = replace_section(text, "## 11. Canonical Research Engine", "## 12. Alpha Contract", section11)

    section24 = """## 24. Nautilus-native Candidate Bundle

旧 Feature/Alpha/Calibration/Portfolio Policy 微型 wheel runtime 不再是主要交付协议。批准后冻结能够在 Nautilus Backtest/Paper/Live 之间复用的 Strategy artifact、config、runtime pin、data requirements、验证 reports、evidence 与 lineage：

```text
candidate-bundle/
  manifest.json
  requirements.lock
  strategy/
    strategy.whl
    strategy-config.json
    actor-config.json
  data/
    requirements.json
    instrument-scope.json
    custom-data-schemas/
  runtime/
    nautilus-version.json
    backtest-run-config.json
    venue-config.json
    risk-config.json
    live-node-template.json
  validation/
    fixture-catalog/
    expected-orders.json
    expected-positions.json
    expected-statistics.json
  evidence/
    discovery-summary.json
    sealed-summary.json
    robustness-summary.json
  lineage.json
```

`requirements.lock` 精确固定 `nautilus_trader==1.231.0`。Bundle 不包含真实 broker/provider/runtime credential、private key、account secret 或 execution-control endpoint。`live-node-template.json` 只是 downstream-owned 配置模板，QZ 不启动或控制节点。

Bundle conformance 依赖显式 artifact/version、wheel metadata、schema、required files、fixture/report/statistics 与 remote `verify_candidate`；不新增应用级 hash/checksum/fingerprint gate。
"""
    text = replace_section(text, "## 24. Candidate Package", "## 25. Handoff Registry", section24)

    section37 = """## 37. 运行拓扑

QuaZonai Core production Compose：

```text
postgres
migrate
api
finite-worker        # Mission + remote Discovery/Sealed durable jobs
```

外部独立部署：

```text
remote-nautilus-research-runtime
remote-nautilus-sealed-runtime
nautilus-paper-node
nautilus-live-node
```

`deploy/Dockerfile.nautilus-runtime` 是 reference remote runtime image；`deploy/nautilus-runtime.compose.example.yml` 仅用于在另一主机部署示例，不加入 Core Compose。Research 与 Sealed 使用不同 endpoint/token/catalog。Core API image 必须证明未安装 NautilusTrader。

QZ 仍不引入 Redis/Celery/Kafka/Kubernetes，使用 PostgreSQL durable jobs。Remote runtime 负责单次 quant run 内部 MessageBus/Cache；QZ PostgreSQL 负责业务事实和跨进程恢复。
"""
    text = replace_section(text, "## 37. 运行拓扑", "### 37.1 Operator Authentication", section37 + "\n### 37.1 Operator Authentication")
    text = text.replace("### 37.1 Operator Authentication\n\n### 37.1 Operator Authentication", "### 37.1 Operator Authentication")
    path.write_text(text, encoding="utf-8")


def update_readme() -> None:
    path = Path("README.md")
    text = path.read_text(encoding="utf-8")
    old = (
        "QuaZonai is a single-user, self-hosted autonomous quantitative **research and portfolio construction** workbench. It uses the official OpenAI Codex App Server SDK for finite research Missions, an independent sealed evaluator for promotion evidence, and downstream-neutral Candidate Packages for Paper/Live handoff.\n\n"
        "QuaZonai does **not** own broker credentials, orders, fills, positions, accounts, NAV, TradingNode, live execution, execution risk, heartbeat, recovery, or downstream stop/undeploy."
    )
    new = (
        "QuaZonai is a single-user, self-hosted AI quantitative research and governance **Control Plane**. It uses the official OpenAI Codex App Server SDK for finite Missions and a pinned remote NautilusTrader `1.231.0` runtime for canonical market-data catalogs, strategy execution, backtesting, matching, order/fill/position/PnL evidence and sealed evaluation.\n\n"
        "The NautilusTrader instances run independently, typically on another host. QuaZonai calls them through a typed HTTP contract and does not import NautilusTrader in the Core API/worker image. QuaZonai still does **not** own broker credentials, real orders, fills, positions, accounts, NAV, TradingNode/LiveNode, execution risk, heartbeat, recovery, or downstream stop/undeploy."
    )
    if old not in text:
        raise RuntimeError("README introduction baseline not found")
    text = text.replace(old, new, 1)
    marker = "## Remote Nautilus runtime"
    if marker not in text:
        text += """

## Remote Nautilus runtime

Configure the finite worker with independent Discovery and Sealed service endpoints:

```dotenv
QUAZONAI_NAUTILUS_RESEARCH_URL=https://research-runtime.example
QUAZONAI_NAUTILUS_RESEARCH_TOKEN=...
QUAZONAI_NAUTILUS_SEALED_URL=https://sealed-runtime.example
QUAZONAI_NAUTILUS_SEALED_TOKEN=...
QUAZONAI_NAUTILUS_TIMEOUT_SECONDS=120
```

Deploy the pinned reference service from `deploy/Dockerfile.nautilus-runtime`; `deploy/nautilus-runtime.compose.example.yml` demonstrates separate Research and Sealed instances. These service credentials are not broker credentials and are injected only into the trusted finite worker. Codex Mission children and the Web/API never receive them.

A governed Dataset Revision must have an immutable Nautilus Catalog binding before a Mission experiment can be queued. Discovery evidence is persisted with the Search Ledger; Sealed evaluation uses a separate endpoint/token/catalog and returns controlled disclosure only. Approved output is a Nautilus-native Candidate Bundle.
"""
    path.write_text(text, encoding="utf-8")


def append_once(path_name: str, marker: str, body: str) -> None:
    path = Path(path_name)
    text = path.read_text(encoding="utf-8")
    if marker not in text:
        text += "\n\n" + body.strip() + "\n"
        path.write_text(text, encoding="utf-8")


def update_secondary_docs() -> None:
    append_once(
        "OPERATIONS.md",
        "## Remote Nautilus Research Runtime",
        """
## Remote Nautilus Research Runtime

QuaZonai Core 与 NautilusTrader 分开部署。管理员在 worker 部署边界配置 Discovery/Sealed endpoint 与 service token；Web Administration 不回读 token，Codex child 永远看不到它们。

运行顺序：

```text
注册/摄取数据到远程 Nautilus Catalog
→ 在 QZ 创建 immutable Dataset Revision + Catalog binding
→ Idea / Mission 生成 experiment contract
→ finite worker 调用 Remote Research Runtime
→ orders/fills/positions/PnL/statistics 回流 Search Ledger
→ 独立 Sealed Runtime 返回 controlled disclosure
→ PASS 后产生 Alpha / Candidate / Paper Approval
→ 批准后生成 Nautilus-native Candidate Bundle
→ 独立 Paper runtime claim 并回传 Forward Evidence
```

远程运行时不可用时，Experiment 保留 FAILED evidence 并可按 durable job policy 重试；不得退回 QZ 自研模拟器。Sealed endpoint/token/catalog 必须独立，Sealed raw report 不展示给 Agent。
""",
    )
    append_once(
        "CLI.md",
        "## Remote Nautilus Quant Runtime Contract",
        """
## Remote Nautilus Quant Runtime Contract

Core API 提供治理与观察接口：

```text
POST /api/v1/nautilus-catalog-bindings
GET  /api/v1/nautilus-catalog-bindings
POST /api/v1/research-missions/{mission_id}/quant-experiments
GET  /api/v1/research-programs/{program_id}/quant-experiments
GET  /api/v1/research-programs/{program_id}/search-ledger
```

Remote runtime contract：

```text
GET  /v1/health
POST /v1/catalogs/ingest
POST /v1/catalogs/validate
POST /v1/backtests
POST /v1/sealed-backtests
POST /v1/candidates/build
POST /v1/candidates/verify
```

所有 remote mutation 使用 `Idempotency-Key`；认证使用独立 service Bearer token。Agent 不能直接调用这些 endpoint，只能写 `experiment-contract.json`，由受信 Mission runner 校验并排队。
""",
    )


def update_tests_and_helpers() -> None:
    path = Path("backend/tests/integration/test_domain_api.py")
    text = path.read_text(encoding="utf-8")
    old_required = '''        required = {
            "manifest.json",
            "schemas/canonical-input.schema.json",
            "schemas/raw-alpha.schema.json",
            "schemas/target-portfolio-frame.schema.json",
            "fixtures/input.arrow",
            "fixtures/expected_alpha.arrow",
            "fixtures/expected_portfolio.arrow",
            "evidence/approval-summary.json",
            "lineage.json",
        }
        assert required <= names
        manifest = json.loads(archive.read("manifest.json"))
        for runtime_path in manifest["runtime"].values():
            if isinstance(runtime_path, str):
                assert runtime_path in names
'''
    new_required = '''        required = {
            "manifest.json",
            "requirements.lock",
            "strategy/strategy.whl",
            "strategy/strategy-config.json",
            "data/requirements.json",
            "runtime/nautilus-version.json",
            "runtime/live-node-template.json",
            "validation/expected-orders.json",
            "validation/expected-positions.json",
            "validation/expected-statistics.json",
            "evidence/discovery-summary.json",
            "evidence/sealed-summary.json",
            "lineage.json",
        }
        assert required <= names
        manifest = json.loads(archive.read("manifest.json"))
        assert manifest["runtime"] == {"name": "NAUTILUS_TRADER", "version": "1.231.0"}
        assert manifest["contains_broker_credentials"] is False
'''
    if old_required not in text:
        raise RuntimeError("domain Candidate Package assertion baseline not found")
    path.write_text(text.replace(old_required, new_required, 1), encoding="utf-8")

    candidate_path = Path("backend/src/candidate_packages.py")
    candidate = candidate_path.read_text(encoding="utf-8")
    marker = "def _write_descriptor_wheel(path: Path, strategy: dict[str, Any]) -> None:\n"
    if "def _write_wheel(" not in candidate:
        helper = '''def _write_wheel(
    path: Path,
    *,
    distribution: str,
    module: str,
    source: str,
    version: str = "1.0.0",
) -> None:
    """Create a standards-conforming pure-Python wheel for frozen artifacts."""
    dist_info = f"{distribution.replace('-', '_')}-{version}.dist-info"
    module_path = f"{module}/__init__.py"
    metadata_path = f"{dist_info}/METADATA"
    wheel_path = f"{dist_info}/WHEEL"
    record_path = f"{dist_info}/RECORD"
    metadata = (
        "Metadata-Version: 2.4\\n"
        f"Name: {distribution}\\n"
        f"Version: {version}\\n"
        "Summary: Frozen QuaZonai artifact\\n"
    )
    wheel = "Wheel-Version: 1.0\\nGenerator: QuaZonai\\nRoot-Is-Purelib: true\\nTag: py3-none-any\\n"
    record = "\\n".join(
        [f"{module_path},,", f"{metadata_path},,", f"{wheel_path},,", f"{record_path},,", ""]
    )
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(module_path, source)
        archive.writestr(metadata_path, metadata)
        archive.writestr(wheel_path, wheel)
        archive.writestr(record_path, record)


'''
        if marker not in candidate:
            raise RuntimeError("candidate wheel insertion marker not found")
        candidate_path.write_text(candidate.replace(marker, helper + marker, 1), encoding="utf-8")


def main() -> None:
    update_design()
    update_readme()
    update_secondary_docs()
    update_tests_and_helpers()


if __name__ == "__main__":
    main()
