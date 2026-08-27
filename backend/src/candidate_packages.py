"""Candidate Bundle v2 public entry points.

The historical module name is retained only because API rows are still called
CandidatePackage.  The generated artifact is exclusively the Nautilus-native
Candidate Bundle contract.
"""

from candidate_bundles import (
    BUNDLE_CONTRACT_VERSION,
    BuiltCandidateBundle,
    BuiltCandidatePackage,
    build_candidate_bundle,
    build_candidate_package,
    validate_candidate_bundle,
    validate_candidate_package,
)

CandidatePackageBuild = BuiltCandidateBundle
CandidatePackageBuildResult = BuiltCandidateBundle

__all__ = [
    "BUNDLE_CONTRACT_VERSION",
    "BuiltCandidateBundle",
    "BuiltCandidatePackage",
    "CandidatePackageBuild",
    "CandidatePackageBuildResult",
    "build_candidate_bundle",
    "build_candidate_package",
    "validate_candidate_bundle",
    "validate_candidate_package",
]
