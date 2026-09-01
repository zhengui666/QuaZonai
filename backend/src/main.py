"""FastAPI application entry point for the QuaZonai research workbench."""

from __future__ import annotations

import re
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any

from fastapi import FastAPI, HTTPException, Request, Response
from fastapi.openapi.utils import get_openapi
from fastapi.responses import FileResponse, JSONResponse
from fastapi.routing import APIRoute
from sqlalchemy import Engine

from quazonai import __version__
from api.auth import router as auth_router
from api.credentials import router as credentials_router
from api.domain import router as domain_router
from api.events import router as events_router
from api.mobile_auth import router as mobile_auth_router
from api.plugins import router as plugins_router
from api.quant_runtime import router as quant_runtime_router
from api.system import router as system_router
from db.session import create_database_engine, create_session_factory
from errors import QfError, install_error_handlers
from mobile_auth import authenticate_mobile_access
from operator_auth import (
    OperatorAuthRuntime,
    STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
    authenticate_browser,
    authenticate_machine,
    browser_cookie_epoch,
    is_operator_auth_exempt,
    require_same_origin,
)
from operator_auth_store import initialize_operator_auth
from settings import Settings


_WORKBENCH_FRAME_HEADERS = {
    "Content-Security-Policy": "frame-ancestors 'none'",
    "X-Frame-Options": "DENY",
}
_BROWSER_PROTECTED_CACHE_CONTROL = "private, no-store"
_NATIVE_PUBLIC_ROUTES = frozenset(
    {
        ("GET", "/api/v1/client/bootstrap"),
        ("GET", "/api/v1/openapi.json"),
        ("POST", "/api/v1/auth/mobile/login"),
        ("POST", "/api/v1/auth/mobile/refresh"),
    }
)

_BROWSER_AUTH_PUBLIC_ROUTES = frozenset(
    {
        ("GET", "/api/v1/auth/bootstrap"),
        ("POST", "/api/v1/auth/login"),
        ("GET", "/api/v1/auth/session"),
        ("POST", "/api/v1/auth/logout"),
        ("POST", "/api/v1/auth/setup/start"),
        ("POST", "/api/v1/auth/setup/confirm"),
    }
)

_AUTH_LOGIN_ROUTES = frozenset(
    {
        ("POST", "/api/v1/auth/login"),
        ("POST", "/api/v1/auth/mobile/login"),
    }
)
_DOWNSTREAM_BEARER_ROUTES = frozenset(
    {
        ("POST", "/api/v1/handoffs/{handoff_id}/claim"),
        ("POST", "/api/v1/handoffs/{handoff_id}/accept"),
        ("POST", "/api/v1/handoffs/{handoff_id}/reject"),
        ("GET", "/api/v1/handoffs/{handoff_id}/package"),
        ("POST", "/api/v1/handoffs/{handoff_id}/feedback"),
    }
)


def _stable_operation_id(route: APIRoute) -> str:
    """Generate wire-stable operation IDs from method and canonical route path."""
    method = sorted(route.methods or {"GET"})[0].lower()
    slug = re.sub(r"[^A-Za-z0-9]+", "_", route.path_format).strip("_")
    return f"{method}_{slug}"


def _install_openapi_contract(app: FastAPI) -> None:
    def custom_openapi() -> dict[str, Any]:
        if app.openapi_schema is not None:
            return app.openapi_schema
        schema = get_openapi(title=app.title, version=app.version, routes=app.routes)
        components = schema.setdefault("components", {})
        schemas = components.setdefault("schemas", {})
        schemas.update(
            {
                "ErrorBody": {
                    "properties": {
                        "code": {"type": "string", "title": "Code"},
                        "message": {"type": "string", "title": "Message"},
                        "details": {
                            "type": "object",
                            "additionalProperties": True,
                            "title": "Details",
                        },
                    },
                    "additionalProperties": False,
                    "type": "object",
                    "required": ["code", "message"],
                    "title": "ErrorBody",
                },
                "ErrorEnvelope": {
                    "properties": {
                        "error": {"$ref": "#/components/schemas/ErrorBody"},
                    },
                    "additionalProperties": False,
                    "type": "object",
                    "required": ["error"],
                    "title": "ErrorEnvelope",
                },
            }
        )
        schemes = components.setdefault("securitySchemes", {})
        schemes.update(
            {
                "BrowserSession": {
                    "type": "apiKey",
                    "in": "cookie",
                    "name": "quazonai_session",
                },
                "MachineBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "QUAZONAI_API_TOKEN for machine automation only.",
                },
                "MobileAccessBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Short-lived native operator access credential.",
                },
                "MobileRefreshBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Rotating trusted-device refresh credential.",
                },
                "DownstreamBearer": {
                    "type": "http",
                    "scheme": "bearer",
                    "description": "Service token issued once to the selected downstream system.",
                },
            }
        )
        for path, path_item in schema.get("paths", {}).items():
            if not isinstance(path_item, dict):
                continue
            for method, operation in path_item.items():
                if method.upper() not in {"GET", "POST", "PUT", "PATCH", "DELETE"}:
                    continue
                if not isinstance(operation, dict):
                    continue
                pair = (method.upper(), path)
                if pair == ("POST", "/api/v1/auth/mobile/refresh"):
                    operation["security"] = [{"MobileRefreshBearer": []}]
                elif pair in _DOWNSTREAM_BEARER_ROUTES:
                    operation["security"] = [{"DownstreamBearer": []}]
                elif pair in _NATIVE_PUBLIC_ROUTES or pair in _BROWSER_AUTH_PUBLIC_ROUTES or pair in {
                    ("GET", "/api/v1/system/health"),
                    ("POST", "/api/v1/auth/login"),
                    ("GET", "/api/v1/auth/session"),
                    ("POST", "/api/v1/auth/logout"),
                }:
                    operation["security"] = []
                else:
                    operation["security"] = [
                        {"BrowserSession": []},
                        {"MachineBearer": []},
                        {"MobileAccessBearer": []},
                    ]

                responses = operation.setdefault("responses", {})
                if pair in _AUTH_LOGIN_ROUTES:
                    responses.pop("422", None)
                    responses["401"] = {
                        "description": "Invalid operator credentials.",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/ErrorEnvelope"}
                            }
                        },
                    }
                elif "422" in responses:
                    responses["422"] = {
                        "description": "Request validation failed.",
                        "content": {
                            "application/json": {
                                "schema": {"$ref": "#/components/schemas/ErrorEnvelope"}
                            }
                        },
                    }
        schemas.pop("HTTPValidationError", None)
        schemas.pop("ValidationError", None)
        app.openapi_schema = schema
        return schema

    app.openapi = custom_openapi  # type: ignore[method-assign]


def _install_frontend(app: FastAPI, frontend_dist: Path) -> None:
    root = frontend_dist.resolve()
    index = root / "index.html"
    if not index.is_file():
        return

    @app.get("/{path:path}", include_in_schema=False)
    def frontend(path: str) -> FileResponse:
        if path.startswith("api/"):
            raise HTTPException(status_code=404, detail="Not Found")
        candidate = (root / path).resolve()
        if candidate.is_relative_to(root) and candidate.is_file():
            return FileResponse(candidate, headers=_WORKBENCH_FRAME_HEADERS)
        return FileResponse(index, headers=_WORKBENCH_FRAME_HEADERS)


def _auth_error_response(exc: QfError) -> JSONResponse:
    return JSONResponse(
        status_code=exc.status_code,
        content={
            "error": {
                "code": exc.code,
                "message": exc.message,
                "details": exc.details,
            }
        },
        headers={"Cache-Control": "no-store", "Pragma": "no-cache"},
    )


def _response_already_prevents_storage(response: Response) -> bool:
    return any(
        directive.partition("=")[0].strip().casefold() == "no-store"
        for directive in response.headers.get("Cache-Control", "").split(",")
    )


def _prevent_shared_caching_of_browser_response(response: Response) -> None:
    if _response_already_prevents_storage(response):
        return
    response.headers["Cache-Control"] = _BROWSER_PROTECTED_CACHE_CONTROL
    vary = [field.strip() for field in response.headers.get("Vary", "").split(",")]
    if not any(field.casefold() == "cookie" for field in vary):
        vary.append("Cookie")
    response.headers["Vary"] = ", ".join(field for field in vary if field)


def _install_operator_auth(app: FastAPI) -> None:
    @app.middleware("http")
    async def operator_auth_middleware(
        request: Request,
        call_next: Callable[[Request], Awaitable[Response]],
    ) -> Response:
        settings: Settings = request.app.state.settings
        path = request.url.path
        pair = (request.method.upper(), path)
        if (
            not settings.auth_enabled
            or not path.startswith("/api/v1/")
            or pair in _NATIVE_PUBLIC_ROUTES
            or is_operator_auth_exempt(request.method, path)
        ):
            return await call_next(request)

        runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
        admission_generation = runtime.stream_generation()
        renewal_cookie_issuance = runtime.cookie_issuance()
        renewal_browser_epoch = browser_cookie_epoch(request, settings)
        authorization = request.headers.get("authorization")
        if authorization is not None:
            mobile_identity = authenticate_mobile_access(
                settings,
                request.app.state.session_factory,
                authorization,
            )
            identity = mobile_identity or authenticate_machine(settings, authorization)
            if identity is None:
                return _auth_error_response(
                    QfError("AUTH_REQUIRED", "Operator authentication is required.", 401)
                )
            request.state.operator = identity
            setattr(
                request.state,
                STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
                admission_generation,
            )
            return await call_next(request)

        browser_identity = authenticate_browser(request, settings)
        if browser_identity is None:
            return _auth_error_response(
                QfError("AUTH_REQUIRED", "Operator authentication is required.", 401)
            )
        try:
            require_same_origin(request, settings)
        except QfError as exc:
            return _auth_error_response(exc)

        request.state.operator = browser_identity
        setattr(
            request.state,
            STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE,
            admission_generation,
        )
        response = await call_next(request)
        _prevent_shared_caching_of_browser_response(response)
        if browser_identity.renew_session:
            runtime.renew_session_if_current(
                response,
                settings,
                cookie_issuance=renewal_cookie_issuance,
                browser_epoch=renewal_browser_epoch,
            )
        return response


def create_app(*, settings: Settings | None = None, engine: Engine | None = None) -> FastAPI:
    runtime_settings = settings or Settings.from_env()
    runtime_settings.validate_operator_auth()
    runtime_engine = engine or create_database_engine(runtime_settings)

    app = FastAPI(
        title="QuaZonai API",
        version=__version__,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
        generate_unique_id_function=_stable_operation_id,
    )
    app.state.settings = runtime_settings
    app.state.engine = runtime_engine
    app.state.session_factory = create_session_factory(runtime_engine)
    app.state.operator_auth_runtime = OperatorAuthRuntime()
    canonical_secret = initialize_operator_auth(
        app.state.session_factory,
        runtime_settings,
    )
    app.state.operator_auth_runtime.set_totp_secret(canonical_secret)

    install_error_handlers(app)
    _install_operator_auth(app)
    app.include_router(auth_router)
    app.include_router(mobile_auth_router)
    app.include_router(system_router)
    app.include_router(domain_router)
    app.include_router(quant_runtime_router)
    app.include_router(plugins_router)
    app.include_router(credentials_router)
    app.include_router(events_router)
    _install_openapi_contract(app)

    @app.get("/api/v1/openapi.json", include_in_schema=False)
    def openapi_schema() -> dict[str, object]:
        return app.openapi()

    _install_frontend(app, runtime_settings.frontend_dist)
    return app
