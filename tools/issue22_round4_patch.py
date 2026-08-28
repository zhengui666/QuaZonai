from __future__ import annotations

from pathlib import Path


def must_replace(path: str, old: str, new: str, *, count: int = 1) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if text.count(old) < count:
        raise SystemExit(f"patch anchor missing in {path}: {old[:80]!r}")
    target.write_text(text.replace(old, new, count), encoding="utf-8")


must_replace(
    "backend/src/candidate_bundles.py",
    '''    strategy_module = artifact.strategy_path.split(":", 1)[0].replace(".", "/") + ".py"\n    config_module = artifact.config_path.split(":", 1)[0].replace(".", "/") + ".py"\n    if strategy_module not in files or config_module not in files:\n        raise QfError(\n            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",\n            "Strategy and config import paths must resolve inside the supplied source bundle.",\n            422,\n        )\n''',
    '''    def module_source_exists(import_path: str) -> bool:\n        module = import_path.split(":", 1)[0].replace(".", "/")\n        return f"{module}.py" in files or f"{module}/__init__.py" in files\n\n    if not module_source_exists(artifact.strategy_path) or not module_source_exists(\n        artifact.config_path\n    ):\n        raise QfError(\n            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",\n            "Strategy and config import paths must resolve inside the supplied source bundle.",\n            422,\n        )\n''',
)

must_replace(
    "backend/src/api/domain.py",
    '''                    DownstreamSystem.environment_type == "PAPER",\n                    DownstreamSystem.enabled.is_(True),\n                    DownstreamSystem.preflight_state == "READY",\n                    DownstreamSystem.service_token_ciphertext.is_not(None),\n''',
    '''                    DownstreamSystem.environment_type == "PAPER",\n                    DownstreamSystem.enabled.is_(True),\n                    DownstreamSystem.preflight_state == "READY",\n                    DownstreamSystem.package_contract_version == "2",\n                    DownstreamSystem.service_token_ciphertext.is_not(None),\n''',
)

must_replace(
    "backend/src/quant_runtime/degradation.py",
    "from sqlalchemy import select\n",
    "from sqlalchemy import exists, or_, select\n",
)

must_replace(
    "backend/src/quant_runtime/degradation.py",
    '''    episodes = list(\n        session.scalars(\n            select(ForwardEvidenceEpisode)\n            .where(ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE")\n            .order_by(ForwardEvidenceEpisode.created_at.asc())\n        )\n    )\n''',
    '''    handled_episode = exists(\n        select(DegradationFollowup.id).where(\n            DegradationFollowup.forward_evidence_episode_id == ForwardEvidenceEpisode.id\n        )\n    )\n    episodes = list(\n        session.scalars(\n            select(ForwardEvidenceEpisode)\n            .where(\n                ForwardEvidenceEpisode.state == "FEEDBACK_COMPLETE",\n                or_(\n                    ForwardEvidenceEpisode.evidence["degraded"].as_boolean().is_(True),\n                    ForwardEvidenceEpisode.evidence["degradation_state"]\n                    .as_string()\n                    .in_(sorted(_DEGRADATION_STATES)),\n                ),\n                ~handled_episode,\n            )\n            .order_by(ForwardEvidenceEpisode.created_at.asc())\n        )\n    )\n''',
)

must_replace(
    "backend/src/quant_runtime/workspace.py",
    "import json\nimport os\nimport stat\n",
    "import json\nimport math\nimport os\nimport re\nimport stat\n",
)

must_replace(
    "backend/src/quant_runtime/workspace.py",
    '''MAX_EXPERIMENTS_PER_ROUND = 20\nCATALOG_URI_PREFIX = "nautilus-catalog://"\n\n\n''',
    '''MAX_EXPERIMENTS_PER_ROUND = 20\nCATALOG_URI_PREFIX = "nautilus-catalog://"\n_DEGRADATION_DISCLOSURE_NUMERICS = {\n    "return",\n    "drawdown",\n    "max_drawdown",\n    "sharpe",\n    "sharpe_ratio",\n    "volatility",\n    "turnover",\n    "tracking_error",\n    "capacity_ratio",\n    "cost",\n    "slippage",\n}\n_DEGRADATION_REASON_CODE = re.compile(r"[A-Z0-9][A-Z0-9_:-]{0,79}")\n\n\ndef _degradation_disclosure(evidence: object) -> dict[str, Any]:\n    """Expose only capability-safe, typed forward metrics to the Mission workspace."""\n    if not isinstance(evidence, dict):\n        return {}\n    result: dict[str, Any] = {}\n    degraded = evidence.get("degraded")\n    if isinstance(degraded, bool):\n        result["degraded"] = degraded\n    state = evidence.get("degradation_state")\n    if isinstance(state, str):\n        normalized_state = state.strip().upper()\n        if normalized_state in {"DEGRADED", "FAILED"}:\n            result["degradation_state"] = normalized_state\n    for key in sorted(_DEGRADATION_DISCLOSURE_NUMERICS):\n        value = evidence.get(key)\n        if isinstance(value, bool) or not isinstance(value, (int, float)):\n            continue\n        numeric = float(value)\n        if math.isfinite(numeric):\n            result[key] = value\n    reason_codes = evidence.get("reason_codes")\n    if isinstance(reason_codes, list):\n        safe_codes = [\n            item\n            for item in reason_codes[:50]\n            if isinstance(item, str) and _DEGRADATION_REASON_CODE.fullmatch(item)\n        ]\n        if safe_codes:\n            result["reason_codes"] = safe_codes\n    return result\n\n\n''',
)

must_replace(
    "backend/src/quant_runtime/workspace.py",
    '''                        "evidence": episode.evidence,\n''',
    '''                        "disclosure": _degradation_disclosure(episode.evidence),\n''',
)

Path("backend/tests/unit/test_issue22_codex_round4.py").write_text(
    '''from __future__ import annotations\n\nfrom dataclasses import dataclass\nimport io\nimport zipfile\nfrom uuid import uuid4\n\nfrom candidate_bundles import build_candidate_bundle\nfrom quant_runtime.workspace import _degradation_disclosure\n\n\n@dataclass\nclass _Member:\n    alpha_id: object\n    instrument_id: str\n    weight: str\n\n\n@dataclass\nclass _Candidate:\n    id: object\n    portfolio_program_id: object\n    simulation_experiment_id: object\n    members: list[_Member]\n    metrics: dict\n\n\ndef _package_candidate() -> _Candidate:\n    experiment_id = uuid4()\n    dataset_revision_id = uuid4()\n    strategy_source = """from nautilus_trader.config import StrategyConfig\nfrom nautilus_trader.trading.strategy import Strategy\n\nclass PackageConfig(StrategyConfig, frozen=True):\n    pass\n\nclass PackageStrategy(Strategy):\n    def __init__(self, config: PackageConfig):\n        super().__init__(config)\n"""\n    return _Candidate(\n        id=uuid4(),\n        portfolio_program_id=uuid4(),\n        simulation_experiment_id=experiment_id,\n        members=[_Member(alpha_id=uuid4(), instrument_id="EUR/USD.SIM", weight="1.0")],\n        metrics={\n            "nautilus": {\n                "strategy_artifact": {\n                    "artifact_id": "package-strategy-v1",\n                    "kind": "SOURCE_BUNDLE",\n                    "strategy_path": "package_strategy:PackageStrategy",\n                    "config_path": "package_strategy:PackageConfig",\n                    "config": {},\n                    "source_files": {"package_strategy/__init__.py": strategy_source},\n                    "requirements": ["nautilus_trader==1.231.0"],\n                },\n                "evidence": {\n                    "experiment_id": str(experiment_id),\n                    "orders": [{"order_id": "O-1"}],\n                    "fills": [{"trade_id": "T-1"}],\n                    "positions": [{"position_id": "P-1"}],\n                    "pnl": {"USD": {"PnL": "12.5"}},\n                    "statistics": {"total_orders": 1, "total_positions": 1},\n                },\n                "dataset_revision_ids": [str(dataset_revision_id)],\n                "instrument_scope": ["EUR/USD.SIM"],\n                "data_requirements": {"data_type": "QuoteTick"},\n                "backtest_run_config": {\n                    "catalog_key": "prices-v1",\n                    "catalog_uri": "nautilus-catalog://prices-v1",\n                },\n                "venue_config": {},\n                "risk_config": {},\n            }\n        },\n    )\n\n\ndef test_candidate_bundle_supports_package_entry_points() -> None:\n    candidate = _package_candidate()\n    built = build_candidate_bundle(object(), candidate=candidate)\n    assert built.validation_summary["valid"] is True\n    with zipfile.ZipFile(io.BytesIO(built.archive_bytes)) as archive:\n        wheel_name = built.manifest["strategy"]["wheel"]\n        with zipfile.ZipFile(io.BytesIO(archive.read(wheel_name))) as wheel:\n            assert "package_strategy/__init__.py" in wheel.namelist()\n\n\ndef test_degradation_disclosure_is_capability_filtered() -> None:\n    disclosure = _degradation_disclosure(\n        {\n            "degraded": True,\n            "degradation_state": "degraded",\n            "return": -0.12,\n            "max_drawdown": -0.31,\n            "reason_codes": ["RETURN_BREAKDOWN", "DRAWDOWN_LIMIT", "sk-secret"],\n            "api_key": "must-not-leak",\n            "access_token": "must-not-leak",\n            "private_payload": {"account": "must-not-leak"},\n        }\n    )\n    assert disclosure == {\n        "degradation_state": "DEGRADED",\n        "degraded": True,\n        "max_drawdown": -0.31,\n        "return": -0.12,\n        "reason_codes": ["RETURN_BREAKDOWN", "DRAWDOWN_LIMIT"],\n    }\n''',
    encoding="utf-8",
)

Path("backend/tests/integration/test_issue22_readiness_v2.py").write_text(
    '''from __future__ import annotations\n\nfrom fastapi.testclient import TestClient\nfrom sqlalchemy import Engine\n\nfrom main import create_app\nfrom settings import Settings\n\n\ndef test_paper_readiness_requires_candidate_bundle_v2(\n    engine: Engine,\n    settings: Settings,\n) -> None:\n    client = TestClient(create_app(settings=settings, engine=engine))\n    legacy = client.post(\n        "/api/v1/downstream-systems",\n        headers={"Idempotency-Key": "paper-v1"},\n        json={\n            "name": "Legacy Paper",\n            "environment_type": "PAPER",\n            "package_contract_version": "1",\n            "feedback_contract_version": "1",\n            "enabled": True,\n        },\n    )\n    assert legacy.status_code == 201, legacy.text\n    assert client.get("/api/v1/readiness").json()["PAPER_HANDOFF_READY"] is False\n\n    current = client.post(\n        "/api/v1/downstream-systems",\n        headers={"Idempotency-Key": "paper-v2"},\n        json={\n            "name": "Bundle v2 Paper",\n            "environment_type": "PAPER",\n            "package_contract_version": "2",\n            "feedback_contract_version": "1",\n            "enabled": True,\n        },\n    )\n    assert current.status_code == 201, current.text\n    assert client.get("/api/v1/readiness").json()["PAPER_HANDOFF_READY"] is True\n''',
    encoding="utf-8",
)
