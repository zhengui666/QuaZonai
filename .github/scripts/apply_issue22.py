from __future__ import annotations

import ast
from pathlib import Path
import re
import textwrap

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    destination = ROOT / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(content, encoding="utf-8")


def append_once(path: str, marker: str, section: str) -> None:
    text = read(path)
    if marker not in text:
        write(path, text.rstrip() + "\n\n" + section.strip() + "\n")


def add_pyproject_entries() -> None:
    path = "backend/pyproject.toml"
    text = read(path)
    if '"candidate_bundles"' not in text:
        text, count = re.subn(
            r"(py-modules\s*=\s*\[)",
            r'\1\n  "candidate_bundles",',
            text,
            count=1,
        )
        if count != 1:
            raise RuntimeError("backend py-modules list not found")
    if '"quant_runtime*"' not in text:
        match = re.search(r"include\s*=\s*\[(.*?)\]", text, flags=re.S)
        if not match:
            raise RuntimeError("backend package include list not found")
        body = match.group(1).rstrip()
        replacement = body + ('\n' if body else '') + '  "quant_runtime*",\n'
        text = text[: match.start(1)] + replacement + text[match.end(1) :]
    write(path, text)


def replace_candidate_packages() -> None:
    write(
        "backend/src/candidate_packages.py",
        '''"""Candidate Bundle v2 public entry points.

The historical module name is retained only because API rows are still called
CandidatePackage.  The generated artifact is exclusively the Nautilus-native
Candidate Bundle contract.
"""

from candidate_bundles import (
    BUNDLE_CONTRACT_VERSION,
    BuiltCandidateBundle,
    BuiltCandidatePackage,
    build_candidate_bundle,
    build_candidate_package,
    validate_candidate_bundle,
    validate_candidate_package,
)

CandidatePackageBuild = BuiltCandidateBundle
CandidatePackageBuildResult = BuiltCandidateBundle

__all__ = [
    "BUNDLE_CONTRACT_VERSION",
    "BuiltCandidateBundle",
    "BuiltCandidatePackage",
    "CandidatePackageBuild",
    "CandidatePackageBuildResult",
    "build_candidate_bundle",
    "build_candidate_package",
    "validate_candidate_bundle",
    "validate_candidate_package",
]
''',
    )
    for path in (ROOT / "backend/tests").rglob("test_candidate_package*.py"):
        path.unlink()


def find_domain_models() -> Path:
    for path in (ROOT / "backend/src/db").glob("*.py"):
        text = path.read_text(encoding="utf-8")
        if "class DatasetRevision(" in text and "class PortfolioCandidate(" in text:
            return path
    raise RuntimeError("domain model module not found")


def class_node(text: str, name: str) -> ast.ClassDef:
    tree = ast.parse(text)
    for node in tree.body:
        if isinstance(node, ast.ClassDef) and node.name == name:
            return node
    raise RuntimeError(f"class {name} not found")


def insert_class_fields(text: str, name: str, sentinel: str, block: str) -> str:
    if sentinel in text:
        return text
    node = class_node(text, name)
    lines = text.splitlines(keepends=True)
    insertion = textwrap.indent(textwrap.dedent(block).strip("\n"), "    ") + "\n"
    lines.insert(node.end_lineno, insertion)
    return "".join(lines)


def uuid_db_type(text: str) -> str:
    patterns = [
        r"id:\s*Mapped\[UUID\]\s*=\s*mapped_column\((UUID\(as_uuid=True\)|Uuid)",
        r"id:\s*Mapped\[uuid\.UUID\]\s*=\s*mapped_column\((UUID\(as_uuid=True\)|Uuid)",
    ]
    for pattern in patterns:
        match = re.search(pattern, text)
        if match:
            return match.group(1)
    raise RuntimeError("UUID database type not found")


def json_db_type(text: str) -> str:
    for candidate in ("JSONB", "JSON"):
        if re.search(rf"mapped_column\({candidate}\b", text):
            return candidate
    raise RuntimeError("JSON database type not found")


def patch_models() -> tuple[str, str, str, str]:
    path = find_domain_models()
    text = path.read_text(encoding="utf-8")
    uuid_type = uuid_db_type(text)
    json_type = json_db_type(text)
    dataset_table = re.search(
        r'class DatasetRevision\([^)]*\):.*?__tablename__\s*=\s*["\']([^"\']+)',
        text,
        flags=re.S,
    ).group(1)
    alpha_table = re.search(
        r'class AlphaQualification\([^)]*\):.*?__tablename__\s*=\s*["\']([^"\']+)',
        text,
        flags=re.S,
    ).group(1)
    candidate_table = re.search(
        r'class PortfolioCandidate\([^)]*\):.*?__tablename__\s*=\s*["\']([^"\']+)',
        text,
        flags=re.S,
    ).group(1)

    text = insert_class_fields(
        text,
        "DatasetRevision",
        "catalog_uri:",
        f'''
provider_name: Mapped[str | None] = mapped_column(String(200), nullable=True)
source_license: Mapped[str | None] = mapped_column(Text, nullable=True)
catalog_uri: Mapped[str | None] = mapped_column(Text, nullable=True)
nautilus_data_type: Mapped[str | None] = mapped_column(String(100), nullable=True)
instrument_scope: Mapped[list[str]] = mapped_column({json_type}, default=list, nullable=False)
schema_revision: Mapped[str | None] = mapped_column(String(128), nullable=True)
quality_result: Mapped[dict] = mapped_column({json_type}, default=dict, nullable=False)
point_in_time_result: Mapped[dict] = mapped_column({json_type}, default=dict, nullable=False)
ingested_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
''',
    )
    text = insert_class_fields(
        text,
        "AlphaQualification",
        "source_experiment_id:",
        f'''
source_experiment_id: Mapped[UUID | None] = mapped_column(
    {uuid_type}, ForeignKey("search_ledger_entries.id", ondelete="RESTRICT"), nullable=True
)
''',
    )
    text = insert_class_fields(
        text,
        "PortfolioCandidate",
        "simulation_experiment_id:",
        f'''
simulation_experiment_id: Mapped[UUID | None] = mapped_column(
    {uuid_type}, ForeignKey("search_ledger_entries.id", ondelete="RESTRICT"), nullable=True
)
''',
    )

    if "class SearchLedgerEntry(" not in text:
        marker = "class Event("
        position = text.find(marker)
        if position < 0:
            position = len(text)
        block = f'''
class SearchLedgerEntry(Base):
    __tablename__ = "search_ledger_entries"

    id: Mapped[UUID] = mapped_column({uuid_type}, primary_key=True, default=uuid4)
    program_id: Mapped[UUID] = mapped_column(
        {uuid_type}, ForeignKey("research_programs.id", ondelete="CASCADE"), nullable=False
    )
    branch_id: Mapped[UUID | None] = mapped_column(
        {uuid_type}, ForeignKey("research_branches.id", ondelete="SET NULL"), nullable=True
    )
    mission_id: Mapped[UUID | None] = mapped_column(
        {uuid_type}, ForeignKey("research_missions.id", ondelete="SET NULL"), nullable=True
    )
    dataset_revision_id: Mapped[UUID] = mapped_column(
        {uuid_type}, ForeignKey("{dataset_table}.id", ondelete="RESTRICT"), nullable=False
    )
    parent_entry_id: Mapped[UUID | None] = mapped_column(
        {uuid_type}, ForeignKey("search_ledger_entries.id", ondelete="SET NULL"), nullable=True
    )
    mode: Mapped[str] = mapped_column(String(30), nullable=False)
    state: Mapped[str] = mapped_column(String(30), nullable=False)
    runtime_name: Mapped[str] = mapped_column(String(80), nullable=False)
    runtime_version: Mapped[str | None] = mapped_column(String(80), nullable=True)
    remote_run_id: Mapped[str | None] = mapped_column(String(240), nullable=True)
    request_json: Mapped[dict] = mapped_column({json_type}, default=dict, nullable=False)
    evidence_json: Mapped[dict] = mapped_column({json_type}, default=dict, nullable=False)
    disclosure_json: Mapped[dict] = mapped_column({json_type}, default=dict, nullable=False)
    failure_code: Mapped[str | None] = mapped_column(String(100), nullable=True)
    failure_message: Mapped[str | None] = mapped_column(Text, nullable=True)
    started_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    finished_at: Mapped[datetime | None] = mapped_column(DateTime(timezone=True), nullable=True)
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), default=datetime.utcnow, nullable=False
    )


'''
        text = text[:position] + block + text[position:]
    path.write_text(text, encoding="utf-8")

    models_path = ROOT / "backend/src/db/models.py"
    models = models_path.read_text(encoding="utf-8")
    if "SearchLedgerEntry" not in models:
        import_match = re.search(r"from db\.[\w.]+ import \((.*?)\)", models, flags=re.S)
        if not import_match:
            raise RuntimeError("db.models domain import list not found")
        body = import_match.group(1).rstrip() + "\n    SearchLedgerEntry,"
        models = models[: import_match.start(1)] + body + models[import_match.end(1) :]
        if "__all__" in models:
            all_match = re.search(r"__all__\s*=\s*\[(.*?)\]", models, flags=re.S)
            if all_match:
                body = all_match.group(1).rstrip() + '\n    "SearchLedgerEntry",\n'
                models = models[: all_match.start(1)] + body + models[all_match.end(1) :]
        models_path.write_text(models, encoding="utf-8")
    return dataset_table, alpha_table, candidate_table, uuid_type


def migration_head(migrations: Path) -> str:
    revisions: set[str] = set()
    parents: set[str] = set()
    for path in migrations.glob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        revision = None
        parent_values: list[str] = []
        for node in tree.body:
            if not isinstance(node, ast.Assign):
                continue
            for target in node.targets:
                if isinstance(target, ast.Name) and target.id == "revision":
                    if isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
                        revision = node.value.value
                if isinstance(target, ast.Name) and target.id == "down_revision":
                    if isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
                        parent_values.append(node.value.value)
                    elif isinstance(node.value, (ast.Tuple, ast.List)):
                        parent_values.extend(
                            item.value
                            for item in node.value.elts
                            if isinstance(item, ast.Constant) and isinstance(item.value, str)
                        )
        if revision:
            revisions.add(revision)
            parents.update(parent_values)
    heads = revisions.difference(parents)
    if len(heads) != 1:
        raise RuntimeError(f"expected one migration head, got {sorted(heads)}")
    return heads.pop()


def create_migration(dataset_table: str, alpha_table: str, candidate_table: str) -> None:
    migrations = ROOT / "backend/migrations/versions"
    if not migrations.exists():
        migrations = ROOT / "backend/alembic/versions"
    if not migrations.exists():
        raise RuntimeError("Alembic versions directory not found")
    destination = migrations / "0006_nautilus_first_runtime.py"
    if destination.exists():
        return
    down_revision = migration_head(migrations)
    content = f'''"""Adopt the remote Nautilus-first research runtime.

Revision ID: 0006_nautilus_first_runtime
Revises: {down_revision}
"""

from alembic import op
import sqlalchemy as sa
from sqlalchemy.dialects import postgresql

revision = "0006_nautilus_first_runtime"
down_revision = "{down_revision}"
branch_labels = None
depends_on = None


def upgrade() -> None:
    op.create_table(
        "search_ledger_entries",
        sa.Column("id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("program_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("branch_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("mission_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("dataset_revision_id", postgresql.UUID(as_uuid=True), nullable=False),
        sa.Column("parent_entry_id", postgresql.UUID(as_uuid=True), nullable=True),
        sa.Column("mode", sa.String(length=30), nullable=False),
        sa.Column("state", sa.String(length=30), nullable=False),
        sa.Column("runtime_name", sa.String(length=80), nullable=False),
        sa.Column("runtime_version", sa.String(length=80), nullable=True),
        sa.Column("remote_run_id", sa.String(length=240), nullable=True),
        sa.Column("request_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("evidence_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("disclosure_json", postgresql.JSONB(astext_type=sa.Text()), nullable=False),
        sa.Column("failure_code", sa.String(length=100), nullable=True),
        sa.Column("failure_message", sa.Text(), nullable=True),
        sa.Column("started_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("finished_at", sa.DateTime(timezone=True), nullable=True),
        sa.Column("created_at", sa.DateTime(timezone=True), server_default=sa.text("now()"), nullable=False),
        sa.ForeignKeyConstraint(["branch_id"], ["research_branches.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(["dataset_revision_id"], ["{dataset_table}.id"], ondelete="RESTRICT"),
        sa.ForeignKeyConstraint(["mission_id"], ["research_missions.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(["parent_entry_id"], ["search_ledger_entries.id"], ondelete="SET NULL"),
        sa.ForeignKeyConstraint(["program_id"], ["research_programs.id"], ondelete="CASCADE"),
        sa.PrimaryKeyConstraint("id"),
    )
    op.create_index(
        "ix_search_ledger_program_created", "search_ledger_entries", ["program_id", "created_at"]
    )
    op.create_index(
        "ix_search_ledger_mission_state", "search_ledger_entries", ["mission_id", "state"]
    )
    for name, type_ in (
        ("provider_name", sa.String(length=200)),
        ("source_license", sa.Text()),
        ("catalog_uri", sa.Text()),
        ("nautilus_data_type", sa.String(length=100)),
        ("schema_revision", sa.String(length=128)),
    ):
        op.add_column("{dataset_table}", sa.Column(name, type_, nullable=True))
    op.add_column(
        "{dataset_table}",
        sa.Column("instrument_scope", postgresql.JSONB(astext_type=sa.Text()), server_default=sa.text("'[]'::jsonb"), nullable=False),
    )
    op.add_column(
        "{dataset_table}",
        sa.Column("quality_result", postgresql.JSONB(astext_type=sa.Text()), server_default=sa.text("'{{}}'::jsonb"), nullable=False),
    )
    op.add_column(
        "{dataset_table}",
        sa.Column("point_in_time_result", postgresql.JSONB(astext_type=sa.Text()), server_default=sa.text("'{{}}'::jsonb"), nullable=False),
    )
    op.add_column("{dataset_table}", sa.Column("ingested_at", sa.DateTime(timezone=True), nullable=True))
    op.add_column(
        "{alpha_table}",
        sa.Column("source_experiment_id", postgresql.UUID(as_uuid=True), nullable=True),
    )
    op.create_foreign_key(
        "fk_alpha_qualification_source_experiment",
        "{alpha_table}",
        "search_ledger_entries",
        ["source_experiment_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.add_column(
        "{candidate_table}",
        sa.Column("simulation_experiment_id", postgresql.UUID(as_uuid=True), nullable=True),
    )
    op.create_foreign_key(
        "fk_portfolio_candidate_simulation_experiment",
        "{candidate_table}",
        "search_ledger_entries",
        ["simulation_experiment_id"],
        ["id"],
        ondelete="RESTRICT",
    )


def downgrade() -> None:
    op.drop_constraint(
        "fk_portfolio_candidate_simulation_experiment", "{candidate_table}", type_="foreignkey"
    )
    op.drop_column("{candidate_table}", "simulation_experiment_id")
    op.drop_constraint(
        "fk_alpha_qualification_source_experiment", "{alpha_table}", type_="foreignkey"
    )
    op.drop_column("{alpha_table}", "source_experiment_id")
    for column in (
        "ingested_at",
        "point_in_time_result",
        "quality_result",
        "instrument_scope",
        "schema_revision",
        "nautilus_data_type",
        "catalog_uri",
        "source_license",
        "provider_name",
    ):
        op.drop_column("{dataset_table}", column)
    op.drop_index("ix_search_ledger_mission_state", table_name="search_ledger_entries")
    op.drop_index("ix_search_ledger_program_created", table_name="search_ledger_entries")
    op.drop_table("search_ledger_entries")
'''
    destination.write_text(content, encoding="utf-8")


def strip_event_dependency() -> None:
    path = ROOT / "backend/src/quant_runtime/ledger.py"
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        "from db.models import DatasetRevision, Event, ResearchMission, SearchLedgerEntry",
        "from db.models import DatasetRevision, ResearchMission, SearchLedgerEntry",
    )
    tree = ast.parse(text)
    removals: list[tuple[int, int]] = []
    for node in ast.walk(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)) and node.name == "_event":
            removals.append((node.lineno, node.end_lineno or node.lineno))
        if (
            isinstance(node, ast.Expr)
            and isinstance(node.value, ast.Call)
            and isinstance(node.value.func, ast.Name)
            and node.value.func.id == "_event"
        ):
            removals.append((node.lineno, node.end_lineno or node.lineno))
    lines = text.splitlines(keepends=True)
    for start, end in sorted(removals, reverse=True):
        del lines[start - 1 : end]
    path.write_text("".join(lines), encoding="utf-8")


def patch_mission_runner() -> None:
    path = ROOT / "backend/src/runners/research_missions.py"
    text = path.read_text(encoding="utf-8")
    import_line = "from quant_runtime.workspace import execute_workspace_experiments\n"
    if import_line not in text:
        insert_at = 0
        for match in re.finditer(r"^(?:from|import) .+\n", text, flags=re.M):
            insert_at = match.end()
        text = text[:insert_at] + import_line + text[insert_at:]

    if "CANDIDATE_BUNDLE_V2_EXPERIMENT_CONTRACT" not in text:
        tree = ast.parse(text)
        instruction_assign = None
        for node in ast.walk(tree):
            if not isinstance(node, ast.Assign):
                continue
            if any(isinstance(target, ast.Name) and target.id == "developer_instructions" for target in node.targets):
                instruction_assign = node
                break
        if instruction_assign is not None:
            lines = text.splitlines(keepends=True)
            indent = re.match(r"\s*", lines[instruction_assign.lineno - 1]).group(0)
            block = indent + '''developer_instructions += """

CANDIDATE_BUNDLE_V2_EXPERIMENT_CONTRACT
- QuaZonai Core is the control plane; NautilusTrader runs on a remote research instance.
- Write the executable Nautilus strategy source under strategy/ and one or more protocol-v1 JSON contracts under experiments/.
- Every contract must include a governed DISCOVERY DatasetRevision id, remote catalog key, instrument scope, and the exact StrategyArtifact source bundle.
- Do not call the remote runtime directly and do not fabricate orders, fills, positions, PnL, or statistics. The parent worker executes contracts with credentials unavailable to this Codex process.
- Failed attempts remain part of the Search Ledger. RESULT.md must distinguish proposed logic from runtime evidence.
- Never request or emit broker credentials, account state, live orders, or TradingNode controls.
"""
'''
            lines.insert(instruction_assign.end_lineno, block)
            text = "".join(lines)

    if "execute_workspace_experiments(" not in text:
        tree = ast.parse(text)
        target = None
        for node in ast.walk(tree):
            if not isinstance(node, ast.Assign):
                continue
            if not any(isinstance(item, ast.Name) and item.id == "result" for item in node.targets):
                continue
            if isinstance(node.value, ast.Call) and isinstance(node.value.func, ast.Attribute):
                if node.value.func.attr == "run":
                    target = node
                    break
        if target is None:
            raise RuntimeError("research Mission thread.run assignment not found")
        lines = text.splitlines(keepends=True)
        indent = re.match(r"\s*", lines[target.lineno - 1]).group(0)
        block = indent + '''executed_experiment_ids: set[UUID] = set()
''' + indent + '''executed_experiment_ids.update(
''' + indent + '''    execute_workspace_experiments(
''' + indent + '''        settings,
''' + indent + '''        workspace=workspace,
''' + indent + '''        mission_id=mission.id,
''' + indent + '''        program_id=mission.program_id,
''' + indent + '''        branch_id=mission.branch_id,
''' + indent + '''        already_executed=executed_experiment_ids,
''' + indent + '''    )
''' + indent + ''')
''' + indent + '''if executed_experiment_ids:
''' + indent + '''    result = thread.run(
''' + indent + '''        "Read evidence/INDEX.json and every new evidence/*.json file. Compare the real "
''' + indent + '''        "Nautilus orders, fills, positions, PnL and statistics, then update RESULT.md. "
''' + indent + '''        "Do not create additional experiment contracts in this final evidence turn."
''' + indent + '''    )
'''
        lines.insert(target.end_lineno, block)
        text = "".join(lines)

    for key in (
        "QUAZONAI_NAUTILUS_RESEARCH_URL",
        "QUAZONAI_NAUTILUS_RESEARCH_TOKEN",
        "QUAZONAI_NAUTILUS_SEALED_URL",
        "QUAZONAI_NAUTILUS_SEALED_TOKEN",
    ):
        if f'"{key}": ""' not in text:
            anchor = '"DATABASE_URL": "",'
            if anchor in text:
                text = text.replace(anchor, anchor + f'\n            "{key}": "",', 1)
    path.write_text(text, encoding="utf-8")


def patch_ci() -> None:
    path = ".github/workflows/ci.yml"
    text = read(path)
    text = text.replace("mcp_gateway|nautilus_trader|", "mcp_gateway|")
    if "check_quant_runtime_boundary.py" not in text:
        anchor = "          test -z \"$hits\"\n"
        if anchor not in text:
            anchor = "          test -z \"${hits}\"\n"
        if anchor not in text:
            raise RuntimeError("ownership grep terminator not found")
        text = text.replace(
            anchor,
            anchor + "          python tools/check_quant_runtime_boundary.py\n",
            1,
        )
    if "nautilus-runtime:" not in text:
        text = text.rstrip() + '''

  nautilus-runtime:
    name: Real remote Nautilus runtime integration
    runs-on: ubuntu-latest
    timeout-minutes: 25
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
          cache: pip
          cache-dependency-path: nautilus_runtime/pyproject.toml
      - name: Install pinned remote runtime
        run: python -m pip install -e 'nautilus_runtime[dev]'
      - name: Boundary and real BacktestNode integration
        run: |
          python tools/check_quant_runtime_boundary.py
          pytest -q nautilus_runtime/tests
'''
    write(path, text)


def patch_environment() -> None:
    path = ROOT / ".env.example"
    if not path.exists():
        path = ROOT / "env.example"
    if path.exists():
        text = path.read_text(encoding="utf-8")
        marker = "# Remote NautilusTrader runtime (Issue 22)"
        if marker not in text:
            text = text.rstrip() + '''

# Remote NautilusTrader runtime (Issue 22)
# These are service credentials for research gateways, never broker credentials.
QUAZONAI_NAUTILUS_RESEARCH_URL=https://nautilus-research.example.internal
QUAZONAI_NAUTILUS_RESEARCH_TOKEN=
QUAZONAI_NAUTILUS_RESEARCH_EXPECTED_VERSION=1.231.0
QUAZONAI_NAUTILUS_RESEARCH_TIMEOUT_SECONDS=120
QUAZONAI_NAUTILUS_SEALED_URL=https://nautilus-sealed.example.internal
QUAZONAI_NAUTILUS_SEALED_TOKEN=
QUAZONAI_NAUTILUS_SEALED_EXPECTED_VERSION=1.231.0
QUAZONAI_NAUTILUS_SEALED_TIMEOUT_SECONDS=120
'''
            path.write_text(text + "\n", encoding="utf-8")


def patch_docs() -> None:
    section = '''
## Nautilus-first remote quant runtime (Issue 22)

> 本节是 QuaZonai 与量化运行时边界的当前事实源；如与旧章节冲突，以本节为准。

QuaZonai 是研究控制平面，负责 Idea、Research Charter、Mission DAG、Dataset Revision 治理、Search Ledger、独立评估、Alpha Qualification、Portfolio Candidate、审批、Candidate Bundle、Forward Evidence 与 Degradation Monitoring。QuaZonai Core 不安装、不启动、不嵌入 NautilusTrader，也不保存或代理券商密钥、账户状态、订单、成交、持仓或 TradingNode 控制。

NautilusTrader `1.231.0` 运行在独立远程实例，通过版本化、鉴权的 HTTP Gateway 向 Core 暴露且仅暴露：ParquetDataCatalog 入库/验证、Discovery Backtest、隔离的 Sealed Backtest、Candidate Bundle conformance。远程实例拥有市场数据解析、Instrument 定义、交易所/账户模型、订单/成交/持仓/PnL/统计语义。Research、Sealed Evaluation、Paper、Live 是权限和凭据相互隔离的部署；Paper/Live 复用 Candidate Bundle 中同一 `strategy.whl` 与 config，但 Live broker adapter 和凭据只在对应远程 Runtime 注入。

```text
Idea -> Mission worktree -> strategy source + experiment contracts
     -> parent worker (DB/runtime token) -> Remote Nautilus Research Gateway
     -> ParquetDataCatalog -> BacktestNode -> structured evidence
     -> Search Ledger -> independent Sealed Gateway disclosure
     -> Alpha Qualification -> portfolio optimization + Nautilus simulation
     -> approval -> Nautilus-native Candidate Bundle
     -> independent Paper Runtime -> Forward Evidence -> degradation -> new Mission
```

Codex Mission 子进程没有数据库、Gateway token 或 broker secret。它只能在 worktree 中声明受 schema 约束的实验；父 Worker 校验 Dataset Revision 分区和 point-in-time/quality 状态，提交远程实验并把结构化证据写回 `evidence/`。失败实验也永久保留在 Search Ledger。Sealed Gateway 只返回允许披露的聚合指标，Core 不接收其订单、成交和持仓明细。

Candidate Bundle v2 至少包含锁定的 `nautilus_trader==1.231.0`、研究阶段相同的 `strategy.whl`、strategy/actor/data/instrument/backtest/venue/risk 配置、live-node template、真实订单/成交/持仓/PnL/统计 fixture、Discovery/Sealed/Portfolio 证据与完整 lineage；它不包含 broker adapter、broker credential 或任何 Core-owned execution shim。
'''
    append_once("DESIGN.md", "## Nautilus-first remote quant runtime (Issue 22)", section)
    append_once(
        "OPERATIONS.md",
        "## Remote Nautilus runtime operations (Issue 22)",
        '''
## Remote Nautilus runtime operations (Issue 22)

- Research 与 Sealed Gateway 必须独立部署，分别配置 URL/token；生产环境必须使用 HTTPS/mTLS 边界，Core 中的 token 只能调用 research-only API。
- Gateway 镜像必须精确安装 `nautilus_trader==1.231.0`，持久化 ParquetDataCatalog，禁止暴露 live/order-management endpoint。
- 数据接入先调用 catalog ingest/validate，再把 `catalog_uri`、provider/license、Instrument scope、schema revision、quality 与 point-in-time 结果写入 Dataset Revision。
- 升级 Nautilus 版本时必须同时更新 pin、协议契约、真实 BacktestNode CI、Candidate Bundle conformance fixture；禁止静默漂移。
- `QUAZONAI_NAUTILUS_SEALED_*` 只能提供给 sealed evaluator worker，不得提供给 Research Mission/Codex 子进程。
''',
    )
    append_once(
        "CLI.md",
        "## Remote Nautilus contract artifacts",
        '''
## Remote Nautilus contract artifacts

Research Mission 在 `experiments/*.json` 写 protocol-v1 `BacktestExperimentRequest`，在 `strategy/` 写同一 StrategyArtifact source bundle。父 Worker 执行后写 `evidence/<experiment-id>.json` 和 `evidence/INDEX.json`。Gateway 运维入口为独立包 `nautilus_runtime/` 的 `quazonai-nautilus-gateway`；它不是 QuaZonai Core 子命令，也不得由 Core 启停。
''',
    )
    append_once(
        "README.md",
        "## Remote NautilusTrader requirement",
        '''
## Remote NautilusTrader requirement

QuaZonai 采用 Nautilus-first 架构，但 Core 不嵌入 NautilusTrader。完整研究流需要部署 `nautilus_runtime/` 中的独立 Gateway，精确版本为 `1.231.0`，并通过 `QUAZONAI_NAUTILUS_RESEARCH_*` / `QUAZONAI_NAUTILUS_SEALED_*` 连接远程实例。Core compose 不会启动该运行时。
''',
    )
    append_once(
        "AGENTS.md",
        "## Issue 22 effective Nautilus ownership boundary",
        '''
## Issue 22 effective Nautilus ownership boundary

本节覆盖旧版“禁止 Nautilus”或“仅输出 TargetPortfolioFrame”的描述。允许在协议、文档、Candidate Bundle 与独立 `nautilus_runtime/` 服务中使用 NautilusTrader；`backend/src` 可调用远程 Gateway，但严禁导入 `nautilus_trader` 或拥有 live broker/order/account control。唯一可晋级证据来自 Search Ledger 中真实远程 Nautilus 运行；Candidate Bundle 必须复用相同 StrategyArtifact。`tools/check_quant_runtime_boundary.py` 是精确 AST ownership 检查，禁止恢复全仓字符串黑名单。
''',
    )


def main() -> None:
    add_pyproject_entries()
    replace_candidate_packages()
    dataset_table, alpha_table, candidate_table, _ = patch_models()
    create_migration(dataset_table, alpha_table, candidate_table)
    strip_event_dependency()
    patch_mission_runner()
    patch_ci()
    patch_environment()
    patch_docs()
    app_path = ROOT / "nautilus_runtime/src/quazonai_nautilus_gateway/app.py"
    app_path.write_text(
        app_path.read_text(encoding="utf-8").replace(
            '"/var/lib/quazonai-nautilus"', '"/tmp/quazonai-nautilus"'
        ),
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
