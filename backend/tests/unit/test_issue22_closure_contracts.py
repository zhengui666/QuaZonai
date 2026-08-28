from __future__ import annotations

from types import SimpleNamespace
from uuid import uuid4

import pytest

from candidate_bundles import _strategy_wheel_filename
from errors import QfError
from quant_runtime.promotion import _assert_qualification_replay, _qualification_contract


def test_candidate_strategy_wheel_uses_installable_pep427_filename() -> None:
    candidate_id = uuid4()
    filename = _strategy_wheel_filename(candidate_id)
    assert filename.startswith("quazonai_candidate_strategy-0.0.")
    assert filename.endswith("-py3-none-any.whl")
    assert "/" not in filename


def test_alpha_qualification_replay_is_bound_to_full_request() -> None:
    sealed_revision_id = uuid4()
    contract = _qualification_contract(
        sealed_dataset_revision_id=sealed_revision_id,
        name="Qualified alpha",
        role="PRIMARY_ALPHA",
    )
    existing = SimpleNamespace(metrics={"qualification_contract": contract})
    _assert_qualification_replay(existing, contract)  # type: ignore[arg-type]

    changed = _qualification_contract(
        sealed_dataset_revision_id=uuid4(),
        name="Qualified alpha",
        role="PRIMARY_ALPHA",
    )
    with pytest.raises(QfError) as raised:
        _assert_qualification_replay(existing, changed)  # type: ignore[arg-type]
    assert raised.value.code == "ALPHA_QUALIFICATION_CONTRACT_REUSED"
