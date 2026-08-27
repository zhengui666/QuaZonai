"""Remote NautilusTrader quant-runtime boundary.

QuaZonai owns research governance.  The remote runtime owns market-data,
backtest and transaction semantics.  This package intentionally contains no
``nautilus_trader`` imports so Core can remain on its own Python/runtime cadence.
"""

from quant_runtime.client import NautilusQuantRuntime, RemoteNautilusConfig
from quant_runtime.contracts import (
    BacktestEvidence,
    BacktestExperimentRequest,
    CatalogIngestRequest,
    CatalogIngestResult,
    CatalogValidationRequest,
    CatalogValidationResult,
    RuntimeCapabilities,
    SealedBacktestResult,
    StrategyArtifact,
)

__all__ = [
    "BacktestEvidence",
    "BacktestExperimentRequest",
    "CatalogIngestRequest",
    "CatalogIngestResult",
    "CatalogValidationRequest",
    "CatalogValidationResult",
    "NautilusQuantRuntime",
    "RemoteNautilusConfig",
    "RuntimeCapabilities",
    "SealedBacktestResult",
    "StrategyArtifact",
]
