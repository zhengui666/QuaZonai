"""Stable QuaZonai error envelope."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

from fastapi import FastAPI, Request
from fastapi.exception_handlers import request_validation_exception_handler
from fastapi.exceptions import RequestValidationError
from fastapi.responses import JSONResponse, Response
from pydantic import BaseModel, ConfigDict, Field

_AUTH_LOGIN_PATHS = frozenset(
    {
        "/api/v1/auth/login",
        "/api/v1/auth/setup/confirm",
    }
)
_AUTH_PATH_PREFIX = "/api/v1/auth/"
_CODEX_AUTH_PATH_PREFIX = "/api/v1/system/codex-auth"
_RUNTIME_CONFIGURATION_PATH = "/api/v1/system/runtime-configuration"
_NO_STORE_HEADERS = {"Cache-Control": "no-store", "Pragma": "no-cache"}


class ErrorBody(BaseModel):
    model_config = ConfigDict(extra="forbid")

    code: str
    message: str
    details: dict[str, Any] = Field(default_factory=dict)


class ErrorEnvelope(BaseModel):
    model_config = ConfigDict(extra="forbid")

    error: ErrorBody


@dataclass(slots=True)
class QfError(Exception):
    code: str
    message: str
    status_code: int = 400
    details: dict[str, Any] = field(default_factory=dict)

    def __str__(self) -> str:
        return f"{self.code}: {self.message}"


def _error_response(
    *,
    code: str,
    message: str,
    status_code: int,
    details: dict[str, Any] | None = None,
    headers: dict[str, str] | None = None,
) -> JSONResponse:
    payload = ErrorEnvelope(
        error=ErrorBody(
            code=code,
            message=message,
            details=details or {},
        )
    )
    return JSONResponse(
        status_code=status_code,
        content=payload.model_dump(mode="json"),
        headers=headers,
    )


def install_error_handlers(app: FastAPI) -> None:
    @app.exception_handler(QfError)
    async def handle_qf_error(request: Request, exc: QfError) -> JSONResponse:
        headers = (
            _NO_STORE_HEADERS
            if request.url.path.startswith((_AUTH_PATH_PREFIX, _CODEX_AUTH_PATH_PREFIX))
            else None
        )
        return _error_response(
            code=exc.code,
            message=exc.message,
            status_code=exc.status_code,
            details=exc.details,
            headers=headers,
        )

    @app.exception_handler(RequestValidationError)
    async def handle_request_validation(
        request: Request,
        exc: RequestValidationError,
    ) -> Response:
        if request.url.path in _AUTH_LOGIN_PATHS:
            # FastAPI's default validation envelope may include rejected input values.
            # Login failures must not echo submitted TOTP or legacy authentication material.
            # Every schema/format failure intentionally has the same public shape.
            return _error_response(
                code="AUTH_INVALID",
                message="Operator authentication failed.",
                status_code=401,
                headers=_NO_STORE_HEADERS,
            )
        if request.url.path.startswith(_CODEX_AUTH_PATH_PREFIX):
            # Codex authentication endpoints may carry or reject credential-shaped
            # fields. Validation happens before endpoint-local no-store handling,
            # so never use FastAPI's envelope here: it can reflect rejected input.
            return _error_response(
                code="CODEX_CHATGPT_AUTH_INVALID_REQUEST",
                message="ChatGPT authentication request is invalid.",
                status_code=422,
                headers=_NO_STORE_HEADERS,
            )
        if request.url.path == _RUNTIME_CONFIGURATION_PATH:
            # Runtime configuration accepts a write-only provider API key. Do
            # not let FastAPI's generic validation envelope reflect that input.
            return _error_response(
                code="RUNTIME_CONFIGURATION_INVALID",
                message="Runtime configuration is invalid.",
                status_code=422,
                headers=_NO_STORE_HEADERS,
            )
        return await request_validation_exception_handler(request, exc)
