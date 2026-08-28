from pathlib import Path

def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if old not in text:
        raise SystemExit(f"required pattern missing in {path}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")

replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '        instrument_definition=instrument.to_dict(),\n',
    '        instrument_definition=type(instrument).to_dict(instrument),\n',
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    """    quality_score = 0.0
    if passed:
        quality_score = 0.60
        if sharpe is not None and sharpe >= 1.0:
            quality_score += 0.10
        if max_drawdown is not None and max_drawdown >= -0.10:
            quality_score += 0.05
        if profit_factor is not None and profit_factor >= 1.50:
            quality_score += 0.05
        quality_score = min(0.80, quality_score)

    return {
        "passed": passed,
        "quality_score": quality_score,
        "performance": {
            "pnl_totals": pnl_totals,
            "sharpe_ratio": sharpe,
            "max_drawdown": max_drawdown,
            "profit_factor": profit_factor,
        },
        "order_count": len(raw.get("orders") or []),
        "fill_count": len(raw.get("fills") or []),
        "position_count": len(raw.get("positions") or []),
        "policy_checks": {
            "transaction_evidence": trade_evidence,
            "positive_total_pnl": pnl_pass,
            "non_negative_sharpe_when_available": sharpe_pass,
            "max_drawdown_floor": drawdown_pass,
            "profit_factor_floor_when_available": profit_factor_pass,
        },
        "policy": "SEALED_PERFORMANCE_RISK_V1",
    }
""",
    """    reason_codes: list[str] = []
    if not trade_evidence:
        reason_codes.append("TRANSACTION_EVIDENCE_MISSING")
    if not pnl_pass:
        reason_codes.append("TOTAL_PNL_POLICY_FAILED")
    if not sharpe_pass:
        reason_codes.append("SHARPE_POLICY_FAILED")
    if not drawdown_pass:
        reason_codes.append("DRAWDOWN_POLICY_FAILED")
    if not profit_factor_pass:
        reason_codes.append("PROFIT_FACTOR_POLICY_FAILED")
    if passed:
        reason_codes = ["SEALED_POLICY_PASSED"]

    return {
        "passed": passed,
        "quality_tier": "QUALIFIED" if passed else "REJECTED",
        "reason_codes": reason_codes,
        "policy_checks": {
            "transaction_evidence": trade_evidence,
            "positive_total_pnl": pnl_pass,
            "non_negative_sharpe_when_available": sharpe_pass,
            "max_drawdown_floor": drawdown_pass,
            "profit_factor_floor_when_available": profit_factor_pass,
        },
        "policy": "SEALED_LEVEL1_POLICY_V1",
    }
""",
)

replace_once(
    "backend/src/quant_runtime/promotion.py",
    '        quality = min(1.0, 0.5 + min(float(disclosure.get("fill_count", 0)), 50.0) / 100.0)\n',
    """        quality_tier = str(disclosure.get("quality_tier", ""))
        quality_by_tier = {"QUALIFIED": 0.65}
        if quality_tier not in quality_by_tier:
            raise QfError(
                "SEALED_DISCLOSURE_INVALID",
                "Sealed Level-1 disclosure did not return a recognized qualification category.",
                500,
            )
        quality = quality_by_tier[quality_tier]
""",
)

replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    """                    {
                        "passed": True,
                        "order_count": 1,
                        "fill_count": 1,
                        "position_count": 1,
                        "policy": "AGGREGATES_ONLY_V1",
                    }
""",
    """                    {
                        "passed": True,
                        "quality_tier": "QUALIFIED",
                        "reason_codes": ["SEALED_POLICY_PASSED"],
                        "policy_checks": {
                            "transaction_evidence": True,
                            "positive_total_pnl": True,
                            "non_negative_sharpe_when_available": True,
                            "max_drawdown_floor": True,
                            "profit_factor_floor_when_available": True,
                        },
                        "policy": "SEALED_LEVEL1_POLICY_V1",
                    }
""",
)
replace_once(
    "backend/tests/integration/test_nautilus_promotion.py",
    '        assert sealed_entry.disclosure_json["policy"] == "AGGREGATES_ONLY_V1"\n',
    '        assert sealed_entry.disclosure_json["policy"] == "SEALED_LEVEL1_POLICY_V1"\n',
)

replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    """    assert disclosure["policy"] == "SEALED_PERFORMANCE_RISK_V1"
    assert disclosure["passed"] is True, disclosure
    assert disclosure["quality_score"] >= 0.60
    assert disclosure["fill_count"] >= 2
    assert "statistics" not in disclosure
    assert "pnl_summary" not in disclosure
""",
    """    assert disclosure["policy"] == "SEALED_LEVEL1_POLICY_V1"
    assert disclosure["passed"] is True, disclosure
    assert disclosure["quality_tier"] == "QUALIFIED"
    assert disclosure["reason_codes"] == ["SEALED_POLICY_PASSED"]
    for forbidden in (
        "quality_score",
        "performance",
        "order_count",
        "fill_count",
        "position_count",
        "statistics",
        "pnl_summary",
    ):
        assert forbidden not in disclosure
""",
)

replace_once(
    "backend/src/runners/research_missions.py",
    """        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS": "",
    }
""",
    """        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS": "",
        "QUAZONAI_NAUTILUS_RESEARCH_URL": "",
        "QUAZONAI_NAUTILUS_RESEARCH_TOKEN": "",
        "QUAZONAI_NAUTILUS_RESEARCH_EXPECTED_VERSION": "",
        "QUAZONAI_NAUTILUS_RESEARCH_TIMEOUT_SECONDS": "",
        "QUAZONAI_NAUTILUS_RESEARCH_ALLOW_INSECURE_HTTP": "",
        "QUAZONAI_NAUTILUS_SEALED_URL": "",
        "QUAZONAI_NAUTILUS_SEALED_TOKEN": "",
        "QUAZONAI_NAUTILUS_SEALED_EXPECTED_VERSION": "",
        "QUAZONAI_NAUTILUS_SEALED_TIMEOUT_SECONDS": "",
        "QUAZONAI_NAUTILUS_SEALED_ALLOW_INSECURE_HTTP": "",
    }
""",
)

replace_once(
    "backend/tests/integration/test_runtime_configuration.py",
    """    assert config.env["QUAZONAI_MASTER_KEY"] == ""
    joined = "\\n".join(config.config_overrides)
""",
    """    assert config.env["QUAZONAI_MASTER_KEY"] == ""
    assert config.env["QUAZONAI_NAUTILUS_RESEARCH_TOKEN"] == ""
    assert config.env["QUAZONAI_NAUTILUS_SEALED_TOKEN"] == ""
    assert config.env["QUAZONAI_NAUTILUS_RESEARCH_URL"] == ""
    assert config.env["QUAZONAI_NAUTILUS_SEALED_URL"] == ""
    joined = "\\n".join(config.config_overrides)
""",
)

replace_once(
    "backend/src/candidate_bundles.py",
    """            "access_token",
            "auth_token",
            "broker_token",
""",
    """            "access_token",
            "auth_token",
            "token",
            "broker_token",
""",
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    """            "access_token",
            "auth_token",
            "broker_token",
""",
    """            "access_token",
            "auth_token",
            "token",
            "broker_token",
""",
)

replace_once(
    "backend/src/quant_runtime/client.py",
    """        return self._parse(response, BacktestEvidence)

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> SealedBacktestResult:
""",
    """        result = self._parse(response, BacktestEvidence)
        if result.experiment_id != request.experiment_id or result.mode != request.mode:
            raise QfError(
                "NAUTILUS_RUNTIME_RESULT_IDENTITY_MISMATCH",
                "Remote backtest result does not match the submitted experiment identity.",
                502,
            )
        return result

    def run_sealed_backtest(self, request: BacktestExperimentRequest) -> SealedBacktestResult:
""",
)
replace_once(
    "backend/src/quant_runtime/client.py",
    """        return self._parse(response, SealedBacktestResult)

    def verify_candidate(
""",
    """        result = self._parse(response, SealedBacktestResult)
        if result.experiment_id != request.experiment_id or result.mode != ExperimentMode.SEALED:
            raise QfError(
                "NAUTILUS_RUNTIME_RESULT_IDENTITY_MISMATCH",
                "Remote sealed result does not match the submitted experiment identity.",
                502,
            )
        return result

    def verify_candidate(
""",
)
replace_once(
    "backend/src/quant_runtime/client.py",
    """        return self._parse(response, CandidateVerificationResult)
""",
    """        result = self._parse(response, CandidateVerificationResult)
        if result.candidate_id != request.candidate_id:
            raise QfError(
                "NAUTILUS_RUNTIME_CANDIDATE_IDENTITY_MISMATCH",
                "Remote conformance result does not match the submitted Candidate identity.",
                502,
            )
        return result
""",
)

print("issue22 closure patch applied")
