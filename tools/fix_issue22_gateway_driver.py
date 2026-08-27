from __future__ import annotations

from pathlib import Path

path = Path(__file__).with_name("apply_issue22_gateway_closure.py")
content = path.read_text(encoding="utf-8")
old = '''replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    \'\'\'        instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
\'\'\',
    \'\'\'        instrument_ids=["EUR/USD.SIM"],
\'\'\',
)
'''
new = '''replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    \'\'\'        dataset_revision_id=uuid4(),
        catalog_key="integration-fx-quotes",
        instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
        strategy=StrategyArtifact(
\'\'\',
    \'\'\'        dataset_revision_id=uuid4(),
        catalog_key="integration-fx-quotes",
        instrument_ids=["EUR/USD.SIM"],
        strategy=StrategyArtifact(
\'\'\',
)
'''
if content.count(old) != 1:
    raise RuntimeError(f"expected one gateway driver block, found {content.count(old)}")
path.write_text(content.replace(old, new, 1), encoding="utf-8")
