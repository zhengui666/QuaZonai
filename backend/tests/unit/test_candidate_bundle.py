from __future__ import annotations

from uuid import uuid4

import pytest

from candidate_packages import build_candidate_package, create_candidate_package
from errors import QfError
from settings import Settings


def test_legacy_raw_candidate_package_writers_fail_closed(settings: Settings) -> None:
    with pytest.raises(QfError, match="CANDIDATE_PACKAGE_TRUSTED_BUILD_REQUIRED"):
        build_candidate_package(
            settings,
            candidate=object(),
            package_id=uuid4(),
            package_revision=1,
        )
    with pytest.raises(QfError, match="CANDIDATE_PACKAGE_TRUSTED_BUILD_REQUIRED"):
        create_candidate_package(session=object(), settings=settings, candidate_id=uuid4())
