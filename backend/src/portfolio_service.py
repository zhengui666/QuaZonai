"""Retired raw Portfolio writer.

Portfolio Candidates now require a complete, trusted relational input.  This
compatibility symbol intentionally denies all caller-supplied optimizer inputs.
"""

from __future__ import annotations

from typing import NoReturn

from errors import QfError


def assemble_portfolio_candidate(*_: object, **__: object) -> NoReturn:
    """Reject the former raw-input Candidate construction API."""
    raise QfError(
        "PORTFOLIO_TRUSTED_INPUT_REQUIRED",
        "Portfolio Candidates may only be assembled from a complete trusted PortfolioAssemblyInput.",
        409,
    )
