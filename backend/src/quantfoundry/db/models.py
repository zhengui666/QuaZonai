"""All QuantFoundry persistence models.

Importing this module registers every table on ``Base.metadata``.
"""

from quantfoundry.db.agent_models import (
    AgentArtifact,
    AgentImpactToken,
    McpTaskBinding,
    OperationReceipt,
)
from quantfoundry.db.base import Base, TimestampMixin
from quantfoundry.db.deployment_models import (
    Deployment,
    DeploymentGeneration,
    DeploymentInstrument,
    DeploymentUniverseRevision,
)
from quantfoundry.db.plugin_models import (
    CatalogDataset,
    CredentialSecret,
    CredentialSet,
    DataSource,
    ExecutionConnection,
    PluginArtifact,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from quantfoundry.db.research_models import (
    Approval,
    Experiment,
    Report,
    ResearchCase,
    ResearchSectionRevision,
    Run,
    Strategy,
    StrategyVersion,
)
from quantfoundry.db.risk_models import (
    RiskAccount,
    RiskEvent,
    RiskOpenOrder,
    RiskPosition,
    RiskReservation,
)
from quantfoundry.db.runtime_models import Event, Job

__all__ = [
    "Base",
    "TimestampMixin",
    "PluginRelease",
    "PluginArtifact",
    "PluginRuntimeBundle",
    "PluginRuntimeBundleMember",
    "CredentialSet",
    "CredentialSecret",
    "DataSource",
    "ExecutionConnection",
    "CatalogDataset",
    "Strategy",
    "StrategyVersion",
    "ResearchCase",
    "ResearchSectionRevision",
    "Experiment",
    "Run",
    "Report",
    "Approval",
    "Deployment",
    "DeploymentGeneration",
    "DeploymentUniverseRevision",
    "DeploymentInstrument",
    "Job",
    "Event",
    "RiskAccount",
    "RiskPosition",
    "RiskOpenOrder",
    "RiskReservation",
    "RiskEvent",
    "OperationReceipt",
    "AgentArtifact",
    "AgentImpactToken",
    "McpTaskBinding",
]
