"""Promotion guards derived from canonical Nautilus evidence."""

from __future__ import annotations

from typing import Any

from db.models import SearchLedgerEntry
from errors import QfError


def require_transaction_evidence(entry: SearchLedgerEntry) -> dict[str, Any]:
    """Return evidence only when a successful run contains transaction semantics."""
    if entry.state != "SUCCEEDED" or entry.mode == "SEALED":
        raise QfError(
            "NAUTILUS_EVIDENCE_NOT_ELIGIBLE",
            "Only successful non-sealed runs can supply raw transaction evidence.",
            422,
        )
    evidence = entry.evidence_json
    required = ("orders", "fills", "positions", "pnl", "statistics")
    missing = [name for name in required if name not in evidence]
    if missing:
        raise QfError(
            "NAUTILUS_EVIDENCE_INCOMPLETE",
            "Run evidence is missing canonical transaction fields.",
            422,
            {"missing": missing},
        )
    if not isinstance(evidence["statistics"], dict):
        raise QfError(
            "NAUTILUS_EVIDENCE_INCOMPLETE",
            "Run statistics must be structured.",
            422,
        )
    return evidence


def candidate_evidence_payload(entry: SearchLedgerEntry) -> dict[str, Any]:
    evidence = require_transaction_evidence(entry)
    return {
        "experiment_id": str(entry.id),
        "runtime_name": entry.runtime_name,
        "runtime_version": entry.runtime_version,
        "remote_run_id": entry.remote_run_id,
        "orders": evidence["orders"],
        "fills": evidence["fills"],
        "positions": evidence["positions"],
        "pnl": evidence["pnl"],
        "statistics": evidence["statistics"],
    }
