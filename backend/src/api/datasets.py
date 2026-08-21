"""Historical Dataset staging and catalog registration API."""

from __future__ import annotations

import json
import os
import shutil
from pathlib import Path
from typing import Any
from uuid import UUID, uuid4

from fastapi import APIRouter, Depends, File, Form, Request, UploadFile
from pydantic import BaseModel, ConfigDict
from sqlalchemy import select
from sqlalchemy.orm import Session

from api.dependencies import get_session
from db.models import (
    CatalogDataset,
    DataSource,
    PluginRelease,
    PluginRuntimeBundle,
    PluginRuntimeBundleMember,
    Run,
)
from errors import QfError
from events import append_event
from jobs import enqueue_job

router = APIRouter(prefix="/api/v1", tags=["datasets"])
CHUNK_BYTES = 1024 * 1024


class DatasetView(BaseModel):
    model_config = ConfigDict(extra="forbid")

    id: UUID
    data_source_id: UUID
    instrument_id: str
    catalog_path: str
    metadata: dict[str, Any]
    row_count: int | None
    state: str
    run_id: UUID | None


class DatasetImportResponse(BaseModel):
    dataset: DatasetView
    run_id: UUID
    state: str


def _view(item: CatalogDataset) -> DatasetView:
    return DatasetView(
        id=item.id,
        data_source_id=item.data_source_id,
        instrument_id=item.instrument_id,
        catalog_path=item.catalog_path,
        metadata=item.dataset_metadata,
        row_count=item.row_count,
        state=item.state,
        run_id=item.run_id,
    )


def _ready_bundle(session: Session, release_id: UUID) -> PluginRuntimeBundle:
    item = session.execute(
        select(PluginRuntimeBundle)
        .join(
            PluginRuntimeBundleMember,
            PluginRuntimeBundleMember.runtime_bundle_id == PluginRuntimeBundle.id,
        )
        .where(
            PluginRuntimeBundleMember.plugin_release_id == release_id,
            PluginRuntimeBundle.state == "READY",
        )
        .order_by(PluginRuntimeBundle.ready_at.desc())
        .limit(1)
    ).scalar_one_or_none()
    if item is None:
        raise QfError(
            "PLUGIN_RUNTIME_UNAVAILABLE",
            "Dataset import requires a ready runtime bundle for its data plugin.",
            503,
            {"plugin_release_id": str(release_id)},
        )
    return item


async def _stream(upload: UploadFile, destination: Path, limit: int) -> int:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.uploading")
    total = 0
    try:
        with temporary.open("xb") as target:
            while True:
                chunk = await upload.read(CHUNK_BYTES)
                if not chunk:
                    break
                total += len(chunk)
                if total > limit:
                    raise QfError(
                        "PARQUET_UPLOAD_TOO_LARGE",
                        "Parquet upload exceeds the configured limit.",
                        413,
                        {"max_bytes": limit},
                    )
                target.write(chunk)
            target.flush()
            os.fsync(target.fileno())
        if total == 0:
            raise QfError("PARQUET_L2_SCHEMA_INVALID", "Parquet upload is empty.", 422)
        os.replace(temporary, destination)
        return total
    finally:
        await upload.close()
        temporary.unlink(missing_ok=True)


@router.get("/catalog-datasets", response_model=list[DatasetView])
def list_datasets(session: Session = Depends(get_session)) -> list[DatasetView]:
    return [
        _view(item)
        for item in session.scalars(select(CatalogDataset).order_by(CatalogDataset.id.asc()))
    ]


@router.get("/catalog-datasets/{dataset_id}", response_model=DatasetView)
def show_dataset(
    dataset_id: UUID,
    session: Session = Depends(get_session),
) -> DatasetView:
    item = session.get(CatalogDataset, dataset_id)
    if item is None:
        raise QfError("DATASET_UNKNOWN", "Catalog dataset does not exist.", 404)
    return _view(item)


@router.post(
    "/data-sources/{source_id}/imports/parquet-l2",
    response_model=DatasetImportResponse,
    status_code=202,
)
async def import_parquet_l2(
    source_id: UUID,
    request: Request,
    file: UploadFile = File(...),
    instrument_id: str = Form(...),
    source_label: str = Form(...),
    metadata_json: str = Form(default="{}"),
    session: Session = Depends(get_session),
) -> DatasetImportResponse:
    filename = file.filename or ""
    if Path(filename).name != filename or not filename.endswith(".parquet"):
        await file.close()
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            "Dataset upload must be a .parquet basename file.",
            422,
        )
    try:
        metadata = json.loads(metadata_json)
    except json.JSONDecodeError as exc:
        await file.close()
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            "Dataset metadata must be valid JSON.",
            422,
        ) from exc
    if not isinstance(metadata, dict):
        await file.close()
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            "Dataset metadata must be a JSON object.",
            422,
        )

    source = session.get(DataSource, source_id)
    if source is None:
        await file.close()
        raise QfError("DATA_SOURCE_UNKNOWN", "Data source does not exist.", 404)
    if source.state != "ACTIVE":
        await file.close()
        raise QfError(
            "DATA_SOURCE_INACTIVE",
            "Only ACTIVE data sources can import a dataset.",
            409,
        )
    release = session.get(PluginRelease, source.plugin_release_id)
    if release is None or "HISTORICAL_IMPORT" not in release.descriptor_snapshot.get(
        "capabilities", []
    ):
        await file.close()
        raise QfError(
            "CAPABILITY_MISMATCH",
            "Data source plugin does not provide historical import.",
            422,
        )
    bundle = _ready_bundle(session, source.plugin_release_id)
    source_id_value = source.id
    bundle_id = bundle.id
    session.rollback()

    run_id = uuid4()
    dataset_id = uuid4()
    upload_path = request.app.state.settings.import_root / str(run_id) / "upload.parquet"
    try:
        size_bytes = await _stream(
            file,
            upload_path,
            request.app.state.settings.max_parquet_upload_bytes,
        )
        data_type = str(metadata.get("data_type", "OrderBookDeltas")).strip()
        if not data_type:
            raise QfError(
                "PARQUET_L2_SCHEMA_INVALID",
                "Dataset data_type must not be empty.",
                422,
            )
        dataset_metadata = {
            **metadata,
            "source_label": source_label.strip(),
            "upload_size_bytes": size_bytes,
            "data_type": data_type,
        }
        with session.begin():
            run = Run(
                id=run_id,
                experiment_id=None,
                runtime_bundle_id=bundle_id,
                type="PARQUET_IMPORT",
                state="QUEUED",
                summary={
                    "data_source_id": str(source_id_value),
                    "instrument_id": instrument_id.strip(),
                    "source_label": source_label.strip(),
                },
            )
            dataset = CatalogDataset(
                id=dataset_id,
                data_source_id=source_id_value,
                instrument_id=instrument_id.strip(),
                catalog_path=str(Path("datasets") / str(dataset_id)),
                dataset_metadata=dataset_metadata,
                state="IMPORTING",
                run_id=run_id,
            )
            session.add_all([run, dataset])
            enqueue_job(
                session,
                kind="PARQUET_IMPORT",
                resource_type="run",
                resource_id=run_id,
                payload={"dataset_id": str(dataset_id)},
            )
            append_event(
                session,
                kind="PARQUET_IMPORT_QUEUED",
                aggregate_type="run",
                aggregate_id=run_id,
                payload={"dataset_id": str(dataset_id), "instrument_id": instrument_id},
                actor_kind="LOCAL_OPERATOR",
            )
        return DatasetImportResponse(dataset=_view(dataset), run_id=run.id, state=run.state)
    except Exception:
        session.rollback()
        shutil.rmtree(upload_path.parent, ignore_errors=True)
        raise
