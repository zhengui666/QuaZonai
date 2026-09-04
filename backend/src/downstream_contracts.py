"""Pure downstream preflight contract rules shared by configuration and approval."""

from __future__ import annotations

from datetime import UTC, datetime
from typing import Any

from db.domain_models import DownstreamSystem, PreflightReceipt
from errors import QfError


def feedback_contract_snapshot(downstream: DownstreamSystem, purpose: str) -> dict[str, Any]:
    """Freeze the explicitly registered feedback contract or reject it fail-closed."""
    public_config = downstream.public_config
    configured = public_config.get("feedback_contract") if isinstance(public_config, dict) else None
    if not isinstance(configured, dict) or not configured:
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "Downstream feedback_contract must be explicitly configured.",
            422,
        )

    required_fields = configured.get("required_fields")
    if (
        not isinstance(required_fields, list)
        or not required_fields
        or any(not isinstance(field, str) or not field.strip() for field in required_fields)
    ):
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "feedback_contract.required_fields must be a nonempty list of strings.",
            422,
        )

    minimum_duration = configured.get("minimum_observation_duration_seconds")
    minimum_sample = configured.get("minimum_valid_sample_size")
    if (
        isinstance(minimum_duration, bool)
        or not isinstance(minimum_duration, int)
        or isinstance(minimum_sample, bool)
        or not isinstance(minimum_sample, int)
        or minimum_duration < 0
        or minimum_sample < 1
    ):
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "Feedback contract minimums must be explicit valid integers.",
            422,
        )

    accepted_packages = configured.get("accepted_package_contracts")
    if (
        not isinstance(accepted_packages, list)
        or not accepted_packages
        or any(
            not isinstance(contract, str) or not contract.strip()
            for contract in accepted_packages
        )
        or downstream.package_contract_version not in accepted_packages
    ):
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "feedback_contract.accepted_package_contracts must explicitly accept the downstream package contract.",
            422,
        )

    accepted_arrow = configured.get("accepted_arrow_contracts")
    if (
        not isinstance(accepted_arrow, list)
        or not accepted_arrow
        or any(not isinstance(contract, str) or not contract.strip() for contract in accepted_arrow)
    ):
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "feedback_contract.accepted_arrow_contracts must be a nonempty list of strings.",
            422,
        )

    disclosure_policy = configured.get("disclosure_policy")
    if not isinstance(disclosure_policy, str) or not disclosure_policy.strip():
        raise QfError(
            "FEEDBACK_CONTRACT_INVALID",
            "feedback_contract.disclosure_policy must be a nonempty string.",
            422,
        )

    return {
        "feedback_contract_version_id": downstream.feedback_contract_version,
        "purpose": purpose,
        "minimum_observation_duration_seconds": minimum_duration,
        "minimum_valid_sample_size": minimum_sample,
        "required_fields": required_fields,
        "accepted_package_contracts": accepted_packages,
        "accepted_arrow_contracts": accepted_arrow,
        "disclosure_policy": disclosure_policy,
    }


def is_current_downstream_preflight(
    receipt: PreflightReceipt | None,
    downstream: DownstreamSystem,
    now: datetime,
) -> bool:
    """Return whether a receipt still proves this downstream's current readiness."""
    try:
        feedback_contract_snapshot(downstream, downstream.environment_type)
    except QfError:
        return False
    if receipt is None:
        return False
    valid_until = receipt.valid_until
    # SQLite returns timezone-aware columns as naive values. The only writer
    # accepts UTC, so restore that storage representation before comparison.
    if valid_until.tzinfo is None:
        valid_until = valid_until.replace(tzinfo=UTC)
    if now.tzinfo is None:
        now = now.replace(tzinfo=UTC)
    return bool(
        downstream.enabled
        and downstream.preflight_state == "READY"
        and isinstance(downstream.compatibility, list)
        and receipt.resource_type == "DOWNSTREAM_SYSTEM"
        and receipt.resource_id == downstream.id
        and receipt.resource_revision == downstream.revision
        and receipt.status == "READY"
        and receipt.contract_version == downstream.feedback_contract_version
        and receipt.capabilities == downstream.compatibility
        and valid_until > now
    )
