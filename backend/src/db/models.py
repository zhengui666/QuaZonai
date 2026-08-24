"""All QuaZonai persistence models.

Importing this module registers every table on ``Base.metadata``.
"""

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
from db.runtime_models import Event, Job

__all__ = [
    "Base",
    "TimestampMixin",
    "PluginRelease",
    "PluginArtifact",
    "PluginRuntimeBundle",
    "PluginRuntimeBundleMember",
    "CredentialSet",
    "CredentialSecret",
    "Job",
    "Event",
    "PublicMutationReceipt",
    "ResearchCharter",
    "ResearchProgram",
    "ResearchBranch",
    "ResearchMission",
    "MarketUniverseVersion",
    "GovernedDataSource",
    "DatasetRevision",
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
