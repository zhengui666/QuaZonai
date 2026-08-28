from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"expected exactly one match in {path}, found {count}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


# 1) Preserve long/short signs when deriving the published allocation from the
# exact transaction-level Nautilus simulation that backs the Candidate.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    "    return result if math.isfinite(result) else None\n\n\ndef _executed_instrument_weights(\n",
    "    return result if math.isfinite(result) else None\n\n\ndef _exposure_side_sign(value: object) -> float | None:\n"
    "    token = str(value or \"\").strip().upper()\n"
    "    if token in {\"LONG\", \"BUY\"}:\n"
    "        return 1.0\n"
    "    if token in {\"SHORT\", \"SELL\"}:\n"
    "        return -1.0\n"
    "    return None\n\n\ndef _executed_instrument_weights(\n",
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    "        last_prices[instrument_id] = price\n"
    "        fill_notionals[instrument_id] += abs(quantity * price)\n",
    "        last_prices[instrument_id] = price\n"
    "        side_sign = _exposure_side_sign(fill.get(\"side\"))\n"
    "        signed_quantity = abs(quantity) * side_sign if side_sign is not None else quantity\n"
    "        fill_notionals[instrument_id] += signed_quantity * price\n",
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    "        position_notionals[instrument_id] += abs(quantity * price)\n",
    "        side_sign = _exposure_side_sign(position.get(\"side\"))\n"
    "        signed_quantity = abs(quantity) * side_sign if side_sign is not None else quantity\n"
    "        position_notionals[instrument_id] += signed_quantity * price\n",
)
replace_once(
    "backend/src/quant_runtime/promotion.py",
    "    notionals = position_notionals if sum(position_notionals.values()) > 0 else fill_notionals\n"
    "    positive = {key: value for key, value in notionals.items() if value > 0}\n"
    "    total = sum(positive.values())\n"
    "    if total <= 0:\n"
    "        raise QfError(\n"
    "            \"PORTFOLIO_ALLOCATION_EVIDENCE_MISSING\",\n"
    "            \"Multi-instrument Candidate requires executed allocation evidence from Nautilus.\",\n"
    "            422,\n"
    "        )\n"
    "    return {key: value / total for key, value in positive.items()}\n",
    "    position_gross = sum(abs(value) for value in position_notionals.values())\n"
    "    notionals = position_notionals if position_gross > 0 else fill_notionals\n"
    "    nonzero = {key: value for key, value in notionals.items() if abs(value) > 0}\n"
    "    gross = sum(abs(value) for value in nonzero.values())\n"
    "    if gross <= 0:\n"
    "        raise QfError(\n"
    "            \"PORTFOLIO_ALLOCATION_EVIDENCE_MISSING\",\n"
    "            \"Multi-instrument Candidate requires executed signed allocation evidence from Nautilus.\",\n"
    "            422,\n"
    "        )\n"
    "    return {key: value / gross for key, value in nonzero.items()}\n",
)

# A simulated recommendation is not the approved portfolio baseline. The pointer
# is advanced only after the human approval and remote conformance gate succeed.
replace_once(
    "backend/src/quant_runtime/promotion.py",
    "        portfolio_program.current_candidate_id = candidate.id\n"
    "        portfolio_program.state = \"CANDIDATE_READY\"\n",
    "        portfolio_program.state = \"CANDIDATE_READY\"\n",
)

# 2) Approval target validity and handoff claimability must share the same
# decision-time boundary. Rejections preserve the exact report that was reviewed.
replace_once(
    "backend/src/api/domain.py",
    "    return decision_at + timedelta(seconds=seconds)\n\n\ndef _verify_candidate_bundle_remotely",
    "    return decision_at + timedelta(seconds=seconds)\n\n\ndef _handoff_claim_deadline(\n"
    "    decision_at: datetime, target_effective_until: datetime\n"
    ") -> datetime:\n"
    "    return min(decision_at + timedelta(days=7), target_effective_until)\n\n\ndef _apply_rejection_decision(\n"
    "    approval: ApprovalSnapshot, payload: ApprovalRejectInput, decision_at: datetime\n"
    ") -> None:\n"
    "    approval.state = \"REJECTED\"\n"
    "    approval.revision += 1\n"
    "    approval.updated_at = decision_at\n"
    "    approval.changes_summary = {\n"
    "        **(approval.changes_summary or {}),\n"
    "        \"approval_decision\": {\n"
    "            \"outcome\": \"REJECT\",\n"
    "            \"reason_code\": payload.reason_code,\n"
    "            \"note\": payload.note,\n"
    "            \"decided_at\": decision_at.isoformat(),\n"
    "        },\n"
    "    }\n\n\ndef _verify_candidate_bundle_remotely",
)
replace_once(
    "backend/src/api/domain.py",
    "                persisted_bundle_path = persist_candidate_bundle(\n"
    "                    request.app.state.settings,\n"
    "                    bundle_id,\n"
    "                    built.archive_bytes,\n"
    "                )\n"
    "                package = CandidateBundle(\n",
    "                persisted_bundle_path = persist_candidate_bundle(\n"
    "                    request.app.state.settings,\n"
    "                    bundle_id,\n"
    "                    built.archive_bytes,\n"
    "                )\n"
    "                portfolio_program = session.execute(\n"
    "                    select(PortfolioProgram)\n"
    "                    .where(PortfolioProgram.id == candidate.portfolio_program_id)\n"
    "                    .with_for_update()\n"
    "                ).scalar_one_or_none()\n"
    "                if portfolio_program is None:\n"
    "                    raise QfError(\n"
    "                        \"PORTFOLIO_PROGRAM_NOT_FOUND\",\n"
    "                        \"Approved Candidate lost its Portfolio Program.\",\n"
    "                        500,\n"
    "                    )\n"
    "                portfolio_program.current_candidate_id = candidate.id\n"
    "                package = CandidateBundle(\n",
)
replace_once(
    "backend/src/api/domain.py",
    "                    claim_deadline=_now() + timedelta(days=7),\n",
    "                    claim_deadline=_handoff_claim_deadline(\n"
    "                        decision_at, approval.expires_at\n"
    "                    ),\n",
)
replace_once(
    "backend/src/api/domain.py",
    "            approval.state = \"REJECTED\"\n"
    "            approval.revision += 1\n"
    "            approval.human_report = {\n"
    "                \"decision\": \"REJECT\",\n"
    "                \"reason_code\": payload.reason_code,\n"
    "                \"note\": payload.note,\n"
    "            }\n",
    "            _apply_rejection_decision(approval, payload, _now())\n",
)

# 3) The independent Gateway must enforce the same collision-free wheel release
# convention as Core, without importing Core.
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "\n\nclass NautilusGatewayEngine:\n",
    "\n\ndef _candidate_strategy_wheel_path(candidate_id: UUID) -> str:\n"
    "    return (\n"
    "        \"strategy/quazonai_candidate_strategy-\"\n"
    "        f\"0.0.{candidate_id.int}-py3-none-any.whl\"\n"
    "    )\n\n\nclass NautilusGatewayEngine:\n",
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "        expected_wheel = (\n"
    "            \"strategy/quazonai_candidate_strategy-\"\n"
    "            f\"0.0.{request.candidate_id.int % 1_000_000}-py3-none-any.whl\"\n"
    "        )\n",
    "        expected_wheel = _candidate_strategy_wheel_path(request.candidate_id)\n",
)

# Regression coverage: signed exposures, target/claim validity, approved baseline,
# immutable reviewed report, and Gateway wheel identity.
replace_once(
    "backend/tests/unit/test_issue22_codex_round7.py",
    "def test_approval_freezes_real_simulation_summaries() -> None:\n",
    "def test_short_exposure_sign_is_preserved_in_published_weights() -> None:\n"
    "    weights = _executed_instrument_weights(\n"
    "        {\n"
    "            \"fills\": [\n"
    "                {\"instrument_id\": \"A.SIM\", \"quantity\": \"10\", \"price\": \"100\", \"side\": \"BUY\"},\n"
    "                {\"instrument_id\": \"B.SIM\", \"quantity\": \"5\", \"price\": \"100\", \"side\": \"SELL\"},\n"
    "            ],\n"
    "            \"positions\": [\n"
    "                {\"instrument_id\": \"A.SIM\", \"quantity\": \"10\", \"side\": \"LONG\", \"closed_at\": None},\n"
    "                {\"instrument_id\": \"B.SIM\", \"quantity\": \"5\", \"side\": \"SHORT\", \"closed_at\": None},\n"
    "            ],\n"
    "        },\n"
    "        [\"A.SIM\", \"B.SIM\"],\n"
    "    )\n"
    "    assert weights[\"A.SIM\"] == pytest.approx(2 / 3)\n"
    "    assert weights[\"B.SIM\"] == pytest.approx(-1 / 3)\n"
    "    assert sum(abs(value) for value in weights.values()) == pytest.approx(1.0)\n\n\ndef test_approval_freezes_real_simulation_summaries() -> None:\n",
)

replace_once(
    "backend/tests/integration/test_domain_api.py",
    "            public_config={\n"
    "                \"feedback_contract\": {\n",
    "            public_config={\n"
    "                \"target_validity_seconds\": 120,\n"
    "                \"feedback_contract\": {\n",
)
replace_once(
    "backend/tests/integration/test_domain_api.py",
    "        session.add(candidate)\n"
    "        session.flush()\n"
    "        program.current_candidate_id = candidate.id\n"
    "        approval = ApprovalSnapshot(\n",
    "        session.add(candidate)\n"
    "        session.flush()\n"
    "        approval = ApprovalSnapshot(\n",
)
replace_once(
    "backend/tests/integration/test_domain_api.py",
    "            recommendation_rationale=\"Independent evidence materially improves the frontier.\",\n"
    "            human_report={},\n",
    "            recommendation_rationale=\"Independent evidence materially improves the frontier.\",\n"
    "            human_report={\n"
    "                \"summary\": \"Reviewed Paper recommendation.\",\n"
    "                \"selected_alpha_id\": str(alpha_id),\n"
    "            },\n",
)
replace_once(
    "backend/tests/integration/test_domain_api.py",
    "    handoff = handoffs.json()[0]\n"
    "    assert handoff[\"state\"] == \"AVAILABLE\"\n\n"
    "    unauthorized = client.post(\n",
    "    handoff = handoffs.json()[0]\n"
    "    assert handoff[\"state\"] == \"AVAILABLE\"\n"
    "    assert datetime.fromisoformat(handoff[\"claim_deadline\"]) == datetime.fromisoformat(\n"
    "        approved.json()[\"expires_at\"]\n"
    "    )\n\n"
    "    factory = create_session_factory(engine)\n"
    "    with factory() as session:\n"
    "        approval_row = session.get(ApprovalSnapshot, UUID(approval_id))\n"
    "        assert approval_row is not None\n"
    "        candidate_row = session.get(PortfolioCandidate, approval_row.candidate_id)\n"
    "        assert candidate_row is not None\n"
    "        program_row = session.get(PortfolioProgram, candidate_row.portfolio_program_id)\n"
    "        assert program_row is not None\n"
    "        assert program_row.current_candidate_id == candidate_row.id\n\n"
    "    unauthorized = client.post(\n",
)
replace_once(
    "backend/tests/integration/test_domain_api.py",
    "    first = client.post(f\"/api/v1/approvals/{first_id}/reject\", headers=headers, json=body)\n"
    "    assert first.status_code == 200, first.text\n"
    "    second = client.post(f\"/api/v1/approvals/{second_id}/reject\", headers=headers, json=body)\n",
    "    first = client.post(f\"/api/v1/approvals/{first_id}/reject\", headers=headers, json=body)\n"
    "    assert first.status_code == 200, first.text\n"
    "    assert first.json()[\"human_report\"][\"summary\"] == \"Reviewed Paper recommendation.\"\n"
    "    assert first.json()[\"changes_summary\"][\"approval_decision\"][\"outcome\"] == \"REJECT\"\n"
    "    factory = create_session_factory(engine)\n"
    "    with factory() as session:\n"
    "        rejected = session.get(ApprovalSnapshot, UUID(first_id))\n"
    "        assert rejected is not None\n"
    "        rejected_candidate = session.get(PortfolioCandidate, rejected.candidate_id)\n"
    "        assert rejected_candidate is not None\n"
    "        rejected_program = session.get(PortfolioProgram, rejected_candidate.portfolio_program_id)\n"
    "        assert rejected_program is not None\n"
    "        assert rejected_program.current_candidate_id is None\n"
    "    second = client.post(f\"/api/v1/approvals/{second_id}/reject\", headers=headers, json=body)\n",
)

replace_once(
    "nautilus_runtime/tests/test_gateway_api.py",
    "from quazonai_nautilus_gateway.engine import GatewayContractError, NautilusGatewayEngine\n",
    "from quazonai_nautilus_gateway.engine import (\n"
    "    GatewayContractError,\n"
    "    NautilusGatewayEngine,\n"
    "    _candidate_strategy_wheel_path,\n"
    ")\n",
)
with Path("nautilus_runtime/tests/test_gateway_api.py").open("a", encoding="utf-8") as stream:
    stream.write(
        "\n\ndef test_candidate_wheel_identity_uses_full_uuid_integer() -> None:\n"
        "    candidate_id = UUID(int=1_000_001)\n"
        "    assert _candidate_strategy_wheel_path(candidate_id) == (\n"
        "        \"strategy/quazonai_candidate_strategy-0.0.1000001-py3-none-any.whl\"\n"
        "    )\n"
    )
replace_once(
    "nautilus_runtime/tests/test_gateway_api.py",
    "from uuid import uuid4\n",
    "from uuid import UUID, uuid4\n",
)
