from __future__ import annotations

from pathlib import Path

from quantfoundry.strategy_contract import validate_strategy_source


def test_strategy_contract_is_checked_in_child_process(
    tmp_path: Path,
    monkeypatch,
) -> None:
    package = tmp_path / "fake-runtime" / "nautilus_trader"
    package.mkdir(parents=True)
    (package / "__init__.py").write_text("", encoding="utf-8")
    (package / "config.py").write_text(
        "class StrategyConfig:\n"
        "    def __init__(self, **kwargs): self.values = kwargs\n",
        encoding="utf-8",
    )
    (package / "trading.py").write_text(
        "class Strategy:\n"
        "    def __init__(self, config): self.config = config\n"
        "    def dispose(self): pass\n",
        encoding="utf-8",
    )
    monkeypatch.setenv("PYTHONPATH", str(package.parent))
    source = (
        "from nautilus_trader.config import StrategyConfig\n"
        "from nautilus_trader.trading import Strategy as BaseStrategy\n"
        "class Config(StrategyConfig):\n"
        "    pass\n"
        "class Strategy(BaseStrategy):\n"
        "    pass\n"
        "OBJECTIVE_DIRECTIONS = ('maximize', 'minimize')\n"
        "def suggest(trial): return {}\n"
        "def objectives(result): return (1.0, 2.0)\n"
    )

    result = validate_strategy_source(
        source,
        {"alpha": 1},
        staging_root=tmp_path / "validation",
        timeout_seconds=10,
    )

    assert result.objective_directions == ("maximize", "minimize")
