from __future__ import annotations

import pytest
from sqlalchemy import func, select
from sqlalchemy.orm import Session

from db.models import PortfolioCandidate
from errors import QfError
from portfolio_service import assemble_portfolio_candidate


def test_legacy_raw_portfolio_writer_is_fail_closed(engine) -> None:
    with Session(engine) as session:
        with pytest.raises(QfError, match="PORTFOLIO_TRUSTED_INPUT_REQUIRED") as raised:
            assemble_portfolio_candidate(
                session,
                object(),
                (),
                expected_returns=(0.1,),
                covariance=((1.0,),),
                capital=1,
            )

        assert raised.value.code == "PORTFOLIO_TRUSTED_INPUT_REQUIRED"
        assert session.scalar(select(func.count()).select_from(PortfolioCandidate)) == 0
