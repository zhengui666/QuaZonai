"""Reusable RFC 6238 verification shared by browser and native operator auth."""

from __future__ import annotations

import secrets
import time
from collections.abc import Callable

import pyotp

from operator_auth import OperatorAuthRuntime
from settings import Settings


def constant_time_text_equal(left: str, right: str) -> bool:
    """Compare user-supplied Unicode credentials without raising on bad text."""
    try:
        left_bytes = left.encode("utf-8")
        right_bytes = right.encode("utf-8")
    except UnicodeEncodeError:
        return False
    return secrets.compare_digest(left_bytes, right_bytes)


def matching_totp_step(
    settings: Settings,
    code: str,
    *,
    secret: str | None = None,
    wall_clock: Callable[[], float] = time.time,
) -> tuple[int, int] | None:
    """Return the accepted RFC 6238 step within the deployment's ±1 window."""
    if len(code) != 6 or any(character < "0" or character > "9" for character in code):
        return None
    credential = secret if secret is not None else settings.operator_totp_secret
    if credential is None:
        return None
    totp = pyotp.TOTP(credential)
    current_step = int(wall_clock()) // totp.interval
    supplied = code.encode("ascii")
    for step in (current_step - 1, current_step, current_step + 1):
        expected = totp.at(step * totp.interval).encode("ascii")
        if secrets.compare_digest(expected, supplied):
            return step, current_step
    return None


def verify_totp_once(
    settings: Settings,
    runtime: OperatorAuthRuntime,
    code: str,
    *,
    wall_clock: Callable[[], float] = time.time,
) -> bool:
    """Verify a TOTP and atomically consume its time-step exactly once."""
    if not settings.auth_enabled:
        return False
    credential = runtime.totp_secret() or settings.operator_totp_secret
    matched = matching_totp_step(settings, code, secret=credential, wall_clock=wall_clock)
    if matched is None:
        return False
    step, current_step = matched
    return runtime.consume_totp_step(
        step,
        current_step=current_step,
        replay_key=credential or "",
    )
