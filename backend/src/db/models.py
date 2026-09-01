"""All QuaZonai persistence models.

Importing this module registers every table on ``Base.metadata``.
"""

from db.auth_models import MobileOperatorDevice, OperatorAuthConfiguration
from db.base import Base, TimestampMixin
from db.domain_models import (
    AlphaQualification,
    ApprovalSnapshot,
    CandidatePackage,
    DatasetRevision,
    DownstreamSystem,
    ForwardEvidenceEpisode,
    GovernedDataSource,
    HandoffOffer,
    IdeaContribution,
    MarketUniverseVersion,
    PortfolioCandidate,
    PortfolioMandate,
    PortfolioProgram,
    PublicMutationReceipt,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)
from db.plugin_models import (
    CredentialSecret,
    CredentialSet,
    PluginArtifact,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
)
from db.quant_runtime_models import (
    ArchiveManifest,
    ArchiveManifestShard,
    CapitalContextVersion,
    EvaluationEpisode,
    NautilusCatalogBinding,
    QuantRuntimeRun,
    SearchLedgerEntry,
)
from db.runtime_models import Event, Job, RuntimeConfiguration

__all__ = [
    "Base",
    "TimestampMixin",
    "MobileOperatorDevice",
    "OperatorAuthConfiguration",
    "PluginRelease",
    "PluginArtifact",
    "PluginRuntimeBundle",
    "PluginRuntimeBundleMember",
    "CredentialSet",
    "CredentialSecret",
    "RuntimeConfiguration",
    "Job",
    "Event",
    "PublicMutationReceipt",
    "ResearchCharter",
    "ResearchProgram",
    "IdeaContribution",
    "ResearchBranch",
    "ResearchMission",
    "MarketUniverseVersion",
    "GovernedDataSource",
    "DatasetRevision",
    "NautilusCatalogBinding",
    "ArchiveManifest",
    "ArchiveManifestShard",
    "QuantRuntimeRun",
    "EvaluationEpisode",
    "CapitalContextVersion",
    "SearchLedgerEntry",
    "AlphaQualification",
    "PortfolioMandate",
    "PortfolioProgram",
    "PortfolioCandidate",
    "DownstreamSystem",
    "ApprovalSnapshot",
    "CandidatePackage",
    "HandoffOffer",
    "ForwardEvidenceEpisode",
]
