"""FastAPI application entry point for the QuaZonai research workbench."""

from __future__ import annotations

from fastapi import FastAPI
from sqlalchemy import Engine

from quazonai import __version__
from api.credentials import router as credentials_router
from api.domain import router as domain_router
from api.events import router as events_router
from api.plugins import router as plugins_router
from api.system import router as system_router
from db.session import create_database_engine, create_session_factory
from errors import install_error_handlers
from settings import Settings


def create_app(*, settings: Settings | None = None, engine: Engine | None = None) -> FastAPI:
    runtime_settings = settings or Settings.from_env()
    runtime_engine = engine or create_database_engine(runtime_settings)

    app = FastAPI(
        title="QuaZonai Research Intelligence API",
        version=__version__,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )
    app.state.settings = runtime_settings
    app.state.engine = runtime_engine
    app.state.session_factory = create_session_factory(runtime_engine)

    install_error_handlers(app)
    app.include_router(system_router)
    app.include_router(domain_router)
    app.include_router(plugins_router)
    app.include_router(credentials_router)
    app.include_router(events_router)

    @app.get("/api/v1/openapi.json", include_in_schema=False)
    def openapi_schema() -> dict[str, object]:
        return app.openapi()

    return app
