"""FastAPI application entry point for the QuaZonai research workbench."""

from __future__ import annotations

from collections.abc import Awaitable, Callable
from pathlib import Path

from fastapi import FastAPI, HTTPException, Request, Response
from fastapi.responses import FileResponse, JSONResponse
from sqlalchemy import Engine

from quazonai import __version__
from api.auth import router as auth_router
from api.credentials import router as credentials_router
from api.domain import router as domain_router
from api.events import router as events_router
from api.plugins import router as plugins_router
from api.system import router as system_router
from db.session import create_database_engine, create_session_factory
from errors import QfError, install_error_handlers
from operator_auth import (
    authenticate_browser,
    authenticate_machine,
    is_operator_auth_exempt,
    require_same_origin,
    set_session_cookie,
)
from settings import Settings


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
            return FileResponse(candidate)
        return FileResponse(index)


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
    )


def _install_operator_auth(app: FastAPI) -> None:
    @app.middleware("http")
    async def operator_auth_middleware(
        request: Request,
        call_next: Callable[[Request], Awaitable[Response]],
    ) -> Response:
        settings: Settings = request.app.state.settings
        path = request.url.path
        if (
            not settings.auth_enabled
            or not path.startswith("/api/v1/")
            or is_operator_auth_exempt(request.method, path)
        ):
            return await call_next(request)

        machine_identity = authenticate_machine(
            settings,
            request.headers.get("authorization"),
        )
        if machine_identity is not None:
            request.state.operator = machine_identity
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
        response = await call_next(request)
        if browser_identity.renew_session:
            set_session_cookie(response, settings)
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
    )
    app.state.settings = runtime_settings
    app.state.engine = runtime_engine
    app.state.session_factory = create_session_factory(runtime_engine)

    install_error_handlers(app)
    _install_operator_auth(app)
    app.include_router(auth_router)
    app.include_router(system_router)
    app.include_router(domain_router)
    app.include_router(plugins_router)
    app.include_router(credentials_router)
    app.include_router(events_router)

    @app.get("/api/v1/openapi.json", include_in_schema=False)
    def openapi_schema() -> dict[str, object]:
        return app.openapi()

    _install_frontend(app, runtime_settings.frontend_dist)
    return app
