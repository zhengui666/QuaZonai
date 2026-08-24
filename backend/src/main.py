"""FastAPI application entry point."""

from __future__ import annotations

from collections.abc import Awaitable, Callable

from fastapi import FastAPI, Request, Response
from sqlalchemy import Engine

from quazonai import __version__
from api.agent import router as agent_router
from api.agent_actions import router as agent_actions_router
from api.credentials import router as credentials_router
from api.datasets import router as datasets_router
from api.deployments import router as deployments_router
from api.events import router as events_router
from api.integrations import router as integrations_router
from api.plugins import router as plugins_router
from api.research import router as research_router
from api.risk import router as risk_router
from api.strategies import router as strategies_router
from api.system import router as system_router
from db.session import create_database_engine, create_session_factory
from errors import install_error_handlers
from settings import Settings

_GATEWAY_HEADER_ALIASES = {
    b"x-qz-internal-token": b"x-quazonai-internal-token",
    b"x-qz-agent-issuer": b"x-quazonai-agent-issuer",
    b"x-qz-agent-subject": b"x-quazonai-agent-subject",
    b"x-qz-agent-client-id": b"x-quazonai-agent-client-id",
    b"x-qz-agent-scopes": b"x-quazonai-agent-scopes",
    b"x-qz-upload-offset": b"x-quazonai-upload-offset",
}


def create_app(*, settings: Settings | None = None, engine: Engine | None = None) -> FastAPI:
    runtime_settings = settings or Settings.from_env()
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

    @app.middleware("http")
    async def normalize_gateway_headers(
        request: Request,
        call_next: Callable[[Request], Awaitable[Response]],
    ) -> Response:
        """Normalize the MCP edge's canonical X-QZ headers for the Core dependency."""
        headers = list(request.scope["headers"])
        present = dict(headers)
        for canonical, internal in _GATEWAY_HEADER_ALIASES.items():
            if canonical in present and internal not in present:
                headers.append((internal, present[canonical]))
        request.scope["headers"] = headers
        return await call_next(request)

    install_error_handlers(app)
    app.include_router(system_router)
    app.include_router(plugins_router)
    app.include_router(credentials_router)
    app.include_router(integrations_router)
    app.include_router(datasets_router)
    app.include_router(strategies_router)
    app.include_router(research_router)
    app.include_router(deployments_router)
    app.include_router(risk_router)
    app.include_router(events_router)
    app.include_router(agent_router)
    app.include_router(agent_actions_router)

    @app.get("/api/v1/openapi.json", include_in_schema=False)
    def openapi_schema() -> dict[str, object]:
        return app.openapi()

    return app
