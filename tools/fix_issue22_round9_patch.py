from __future__ import annotations

from pathlib import Path

path = Path("tools/issue22_round9_patch.py")
text = path.read_text(encoding="utf-8")
old = '''replace_once(
    "backend/src/api/research_runtime.py",
    '        if _pending_simulation_experiment_id(receipt) != experiment_id:\\n',
    '        receipt_simulation_id, _ = _pending_simulation_experiment_ids(\\n'
    '            receipt, require_portfolio_sealed=True\\n'
    '        )\\n'
    '        if receipt_simulation_id != experiment_id:\\n',
)
'''
new = '''replace_once(
    "backend/src/api/research_runtime.py",
    '        if _pending_simulation_experiment_id(receipt) != experiment_id:\\n'
    '            return\\n'
    '        if receipt.status_code != _SIMULATION_PENDING_STATUS:\\n'
    '            return\\n'
    '        receipt.response_json = {\\n'
    '            "state": "RETRYABLE",\\n'
    '            "simulation_experiment_id": str(experiment_id),\\n',
    '        receipt_simulation_id, portfolio_sealed_experiment_id = _pending_simulation_experiment_ids(\\n'
    '            receipt, require_portfolio_sealed=True\\n'
    '        )\\n'
    '        if receipt_simulation_id != experiment_id:\\n'
    '            return\\n'
    '        if receipt.status_code != _SIMULATION_PENDING_STATUS:\\n'
    '            return\\n'
    '        assert portfolio_sealed_experiment_id is not None\\n'
    '        receipt.response_json = {\\n'
    '            "state": "RETRYABLE",\\n'
    '            "simulation_experiment_id": str(experiment_id),\\n'
    '            "portfolio_sealed_experiment_id": str(portfolio_sealed_experiment_id),\\n',
)
replace_once(
    "backend/src/api/research_runtime.py",
    '        if _pending_simulation_experiment_id(receipt) != experiment_id:\\n'
    '            raise QfError(\\n'
    '                "IDEMPOTENCY_RECEIPT_CONFLICT",\\n'
    '                "Candidate simulation receipt changed experiment identity.",\\n'
    '                409,\\n'
    '            )\\n',
    '        receipt_simulation_id, _ = _pending_simulation_experiment_ids(\\n'
    '            receipt, require_portfolio_sealed=True\\n'
    '        )\\n'
    '        if receipt_simulation_id != experiment_id:\\n'
    '            raise QfError(\\n'
    '                "IDEMPOTENCY_RECEIPT_CONFLICT",\\n'
    '                "Candidate simulation receipt changed experiment identity.",\\n'
    '                409,\\n'
    '            )\\n',
)
'''
count = text.count(old)
if count != 1:
    raise RuntimeError(f"expected one ambiguous round9 block, found {count}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
