from __future__ import annotations

from datetime import UTC, datetime, timedelta
from pathlib import Path
from types import SimpleNamespace

import pytest

from api.domain import FeedbackInput, _validate_complete_feedback
from errors import QfError
from quant_runtime.workspace import write_parent_owned_workspace_file


def test_parent_owned_workspace_write_refuses_symlink(tmp_path: Path) -> None:
    target = tmp_path / "outside.txt"
    target.write_text("outside", encoding="utf-8")
    link = tmp_path / "DATASETS.json"
    link.symlink_to(target)
    with pytest.raises(QfError) as raised:
        write_parent_owned_workspace_file(link, "{}")
    assert raised.value.code == "MISSION_WORKSPACE_PATH_UNSAFE"
    assert target.read_text(encoding="utf-8") == "outside"


def test_complete_feedback_must_be_post_accept_and_not_future() -> None:
    accepted = datetime(2026, 8, 29, 1, 0, tzinfo=UTC)
    handoff = SimpleNamespace(
        accepted_at=accepted,
        feedback_contract_snapshot={
            "minimum_observation_duration_seconds": 60,
            "minimum_valid_sample_size": 1,
            "required_fields": ["return"],
        },
    )
    before_accept = FeedbackInput(
        state="FEEDBACK_COMPLETE",
        observation_start=accepted - timedelta(minutes=5),
        observation_end=accepted + timedelta(minutes=5),
        sample_size=2,
        evidence={"return": 0.01},
    )
    with pytest.raises(QfError):
        _validate_complete_feedback(
            handoff, before_accept, received_at=accepted + timedelta(minutes=10)
        )
    future = FeedbackInput(
        state="FEEDBACK_COMPLETE",
        observation_start=accepted,
        observation_end=accepted + timedelta(minutes=20),
        sample_size=2,
        evidence={"return": 0.01},
    )
    with pytest.raises(QfError):
        _validate_complete_feedback(
            handoff, future, received_at=accepted + timedelta(minutes=10)
        )
