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
from api.codex_auth import router as codex_auth_router
from api.configuration import router as configuration_router
from api.domain import router as domain_router
from api.events import router as events_router
from api.plugins import router as plugins_router
from api.research import router as research_router
from api.system import router as system_router
from db.session import create_database_engine, create_session_factory
from errors import QfError, install_error_handlers
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
from codex_chatgpt_auth import initialize_codex_auth
from settings import Settings


_WORKBENCH_FRAME_HEADERS = {
    "Content-Security-Policy": "frame-ancestors 'none'",
    "X-Frame-Options": "DENY",
}
_BROWSER_PROTECTED_CACHE_CONTROL = "private, no-store"


def _install_frontend(app: FastAPI, frontend_dist: Path) -> None:
    root = frontend_dist.resolve()
    index = root / "index.html"
    if not index.is_file():
        return

    def file_headers(candidate: Path) -> dict[str, str]:
        headers = dict(_WORKBENCH_FRAME_HEADERS)
        relative = candidate.relative_to(root)
        if relative.as_posix() in {"index.html", "sw.js", "manifest.webmanifest"}:
            headers["Cache-Control"] = "no-cache"
        elif relative.parts and relative.parts[0] == "assets" and candidate.name:
            headers["Cache-Control"] = "public, max-age=31536000, immutable"
        return headers

    @app.get("/{path:path}", include_in_schema=False)
    def frontend(path: str) -> FileResponse:
        if path == "api" or path.startswith("api/"):
            raise HTTPException(status_code=404, detail="Not Found")
        candidate = (root / path).resolve()
        if not candidate.is_relative_to(root):
            raise HTTPException(status_code=404, detail="Not Found")
        if candidate.is_file():
            return FileResponse(candidate, headers=file_headers(candidate))
        return FileResponse(index, headers=file_headers(index))


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
    """Mark a browser-authenticated API response as ineligible for shared storage.

    Streaming endpoints already supply ``no-store`` together with transport-specific
    headers. Leave those responses untouched so this middleware does not dilute their
    streaming cache semantics.
    """
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
        if (
            not settings.auth_enabled
            or not path.startswith("/api/v1/")
            or is_operator_auth_exempt(request.method, path)
        ):
            return await call_next(request)

        runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime
        # Capture this before credential validation. A logout that occurs after
        # validation but before the endpoint starts must still invalidate an
        # already-admitted EventSource request on its next poll.
        admission_generation = runtime.stream_generation()
        # A credentialed logout invalidates process-wide browser issuance; every
        # logout also changes only its caller's sealed browser epoch. Capture the
        # latter from this request so a delayed renewal can never become valid in
        # that browser after its own logout.
        renewal_cookie_issuance = runtime.cookie_issuance()
        renewal_browser_epoch = browser_cookie_epoch(request, settings)
        authorization = request.headers.get("authorization")
        if authorization is not None:
            machine_identity = authenticate_machine(settings, authorization)
            if machine_identity is None:
                return _auth_error_response(
                    QfError("AUTH_REQUIRED", "Operator authentication is required.", 401)
                )
            request.state.operator = machine_identity
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
    initialize_codex_auth(app.state.session_factory, runtime_settings)

    install_error_handlers(app)
    _install_operator_auth(app)
    app.include_router(auth_router)
    app.include_router(system_router)
    app.include_router(configuration_router)
    app.include_router(research_router)
    app.include_router(domain_router)
    app.include_router(plugins_router)
    app.include_router(credentials_router)
    app.include_router(codex_auth_router)
    app.include_router(events_router)

    @app.get("/api/v1/openapi.json", include_in_schema=False)
    def openapi_schema() -> dict[str, object]:
        return app.openapi()

    _install_frontend(app, runtime_settings.frontend_dist)
    return app
