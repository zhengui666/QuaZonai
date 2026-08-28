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
text = text.replace(old, new, 1)

old = '''replace_once(
    "backend/src/quant_runtime/promotion.py",
    '                "remote_run_id": simulation.remote_run_id,\\n'
    '            },\\n',
    '                "remote_run_id": simulation.remote_run_id,\\n'
    '                "portfolio_sealed_experiment_id": str(portfolio_sealed.id),\\n'
    '                "portfolio_sealed_passed": True,\\n'
    '            },\\n',
)
'''
new = '''replace_once(
    "backend/src/quant_runtime/promotion.py",
    '                "remote_run_id": simulation.remote_run_id,\\n'
    '                "order_count": len(simulation_evidence.get("orders", [])),\\n',
    '                "remote_run_id": simulation.remote_run_id,\\n'
    '                "portfolio_sealed_experiment_id": str(portfolio_sealed.id),\\n'
    '                "portfolio_sealed_dataset_revision_id": str(portfolio_sealed.dataset_revision_id),\\n'
    '                "portfolio_sealed_passed": True,\\n'
    '                "order_count": len(simulation_evidence.get("orders", [])),\\n',
)
'''
count = text.count(old)
if count != 1:
    raise RuntimeError(f"expected one approval evidence round9 block, found {count}")
text = text.replace(old, new, 1)

old = '''# Sealed worker qualification unit test payloads need durable id where present.
for test_path in [
    "backend/tests/integration/test_issue22_readiness_v2.py",
    "backend/tests/unit/test_issue22_codex_round5.py",
    "backend/tests/unit/test_jobs.py",
]:
    path = Path(test_path)
    if path.exists():
        text = path.read_text(encoding="utf-8")
        needle = '"sealed_dataset_revision_id": str('
        if needle in text and '"sealed_experiment_id"' not in text:
            # Best-effort broad fixture addition after each sealed dataset line.
            lines = text.splitlines(keepends=True)
            output: list[str] = []
            for line in lines:
                output.append(line)
                if needle in line:
                    indent = line[: len(line) - len(line.lstrip())]
                    output.append(f'{indent}"sealed_experiment_id": str(uuid4()),\\n')
            path.write_text("".join(output), encoding="utf-8")
'''
new = '''# Sealed worker qualification unit fixture keeps the durable child identity in the job payload.
replace_once(
    "backend/tests/unit/test_jobs.py",
    '            payload={"sealed_dataset_revision_id": str(uuid4())},\\n',
    '            payload={\\n'
    '                "sealed_dataset_revision_id": str(uuid4()),\\n'
    '                "sealed_experiment_id": str(uuid4()),\\n'
    '            },\\n',
)
'''
count = text.count(old)
if count != 1:
    raise RuntimeError(f"expected one broad sealed fixture patch, found {count}")
text = text.replace(old, new, 1)

path.write_text(text, encoding="utf-8")
