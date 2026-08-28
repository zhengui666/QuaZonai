"""Authenticated remote HTTP surface; deliberately no live-trading endpoints."""

from __future__ import annotations

import hmac
import os
from pathlib import Path
from typing import Annotated, Any, Literal

from fastapi import Depends, FastAPI, Header, HTTPException, status
from fastapi.responses import JSONResponse

from quazonai_nautilus_gateway.engine import GatewayContractError, NautilusGatewayEngine
from quazonai_nautilus_gateway.models import (
    BacktestExperimentRequest,
    CandidateVerificationRequest,
    CatalogIngestRequest,
    CatalogValidationRequest,
    ExperimentMode,
)

GatewayRole = Literal["RESEARCH", "SEALED"]


def _authorize(authorization: Annotated[str | None, Header()] = None) -> None:
    configured = os.getenv("NAUTILUS_GATEWAY_TOKEN", "")
    if not configured:
        if os.getenv("NAUTILUS_GATEWAY_ALLOW_ANONYMOUS", "false").lower() in {
            "1",
            "true",
            "yes",
        }:
            return
        raise HTTPException(
            status_code=status.HTTP_503_SERVICE_UNAVAILABLE,
            detail="gateway token missing",
        )
    expected = f"Bearer {configured}"
    if authorization is None or not hmac.compare_digest(authorization, expected):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="invalid bearer token",
        )


def _configured_role(explicit: GatewayRole | None) -> GatewayRole:
    raw = explicit or os.getenv("NAUTILUS_GATEWAY_ROLE", "RESEARCH").strip().upper()
    if raw not in {"RESEARCH", "SEALED"}:
        raise RuntimeError("NAUTILUS_GATEWAY_ROLE must be RESEARCH or SEALED")
    return raw  # type: ignore[return-value]


def _require_role(role: GatewayRole, expected: GatewayRole) -> None:
    if role != expected:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail="operation unavailable on this gateway role",
        )


def create_app(
    *,
    data_root: Path | None = None,
    role: GatewayRole | None = None,
) -> FastAPI:
    """Create an API without touching the filesystem until an operation needs the engine."""
    root = data_root or Path(os.getenv("NAUTILUS_GATEWAY_DATA_ROOT", "/tmp/quazonai-nautilus"))
    gateway_role = _configured_role(role)
    engine_instance: NautilusGatewayEngine | None = None

    def engine() -> NautilusGatewayEngine:
        nonlocal engine_instance
        if engine_instance is None:
            engine_instance = NautilusGatewayEngine(root)
        return engine_instance

    app = FastAPI(title="QuaZonai Remote Nautilus Gateway", version="1")

    @app.exception_handler(GatewayContractError)
    async def contract_error(_: Any, __: GatewayContractError) -> JSONResponse:
        # Contract failures may originate while loading user-supplied strategy code.
        # Never reflect exception internals, filesystem paths, or stack-derived text.
        return JSONResponse(
            status_code=422,
            content={
                "code": "CONTRACT_INVALID",
                "detail": "request violates the remote Nautilus runtime contract",
            },
        )

    @app.get("/healthz", include_in_schema=False)
    def health() -> dict[str, str]:
        return {"status": "ok", "role": gateway_role}

    @app.get("/v1/capabilities", dependencies=[Depends(_authorize)])
    def capabilities() -> dict[str, Any]:
        return engine().capabilities()

    @app.post("/v1/catalogs/ingest", dependencies=[Depends(_authorize)])
    def ingest(
        request: CatalogIngestRequest,
        idempotency_key: Annotated[str | None, Header(alias="Idempotency-Key")] = None,
    ) -> dict[str, Any]:
        _require_role(gateway_role, "RESEARCH")
        if idempotency_key is not None and idempotency_key != str(request.request_id):
            raise GatewayContractError("idempotency key does not match request_id")
        return engine().ingest(request)

    @app.post("/v1/catalogs/validate", dependencies=[Depends(_authorize)])
    def validate_catalog(request: CatalogValidationRequest) -> dict[str, Any]:
        _require_role(gateway_role, "RESEARCH")
        return engine().validate_catalog(request)

    @app.post("/v1/backtests", dependencies=[Depends(_authorize)])
    def run_backtest(
        request: BacktestExperimentRequest,
        idempotency_key: Annotated[str | None, Header(alias="Idempotency-Key")] = None,
    ) -> dict[str, Any]:
        _require_role(gateway_role, "RESEARCH")
        if request.mode == ExperimentMode.SEALED:
            raise GatewayContractError("sealed mode is disclosure-only")
        if idempotency_key != str(request.experiment_id):
            raise GatewayContractError(
                "backtest idempotency key must equal experiment_id"
            )
        return engine().run_backtest_idempotent(request)

    @app.post("/v1/sealed-backtests", dependencies=[Depends(_authorize)])
    def run_sealed_backtest(
        request: BacktestExperimentRequest,
        idempotency_key: Annotated[str | None, Header(alias="Idempotency-Key")] = None,
    ) -> dict[str, Any]:
        _require_role(gateway_role, "SEALED")
        if idempotency_key != str(request.experiment_id):
            raise GatewayContractError(
                "sealed backtest idempotency key must equal experiment_id"
            )
        return engine().run_sealed_backtest_idempotent(request)

    @app.post("/v1/candidates/verify", dependencies=[Depends(_authorize)])
    def verify_candidate(request: CandidateVerificationRequest) -> dict[str, Any]:
        _require_role(gateway_role, "RESEARCH")
        return engine().verify_candidate(request)

    return app


def run() -> None:
    import uvicorn

    uvicorn.run(
        create_app(),
        host=os.getenv("NAUTILUS_GATEWAY_HOST", "0.0.0.0"),
        port=int(os.getenv("NAUTILUS_GATEWAY_PORT", "8080")),
        proxy_headers=True,
    )


# ASGI convenience object. Engine initialization remains lazy so importing this
# module in non-root tooling does not create runtime directories.
app = create_app()
