"""Validate fixed L2 Parquet and publish it through the pinned importer plugin."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
from collections.abc import Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session, sessionmaker

from quantfoundry.api.credentials import decrypt_credential_secrets
from quantfoundry.db.models import (
    CatalogDataset,
    CredentialSet,
    DataSource,
    Job,
    PluginRelease,
    PluginRuntimeBundle,
    Run,
)
from quantfoundry.db.session import create_database_engine, create_session_factory
from quantfoundry.errors import QfError
from quantfoundry.events import append_event
from quantfoundry.plugins.runtime import resolve_plugin_path
from quantfoundry.settings import Settings

EXPECTED_COLUMNS = (
    "event_id",
    "event_index",
    "is_snapshot",
    "action",
    "side",
    "price",
    "size",
    "order_id",
    "sequence",
    "ts_event_ns",
    "ts_init_ns",
)


@dataclass(frozen=True, slots=True)
class ImportContext:
    run_id: UUID
    dataset_id: UUID
    source: DataSource
    bundle: PluginRuntimeBundle
    release: PluginRelease
    credential: CredentialSet | None
    instrument_id: str
    metadata: dict[str, Any]
    catalog_path: str


def _validate_decimal(value: object, *, field: str, row: int) -> None:
    if not isinstance(value, str):
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            f"{field} must be stored as a UTF-8 decimal string.",
            422,
            {"row": row},
        )
    try:
        parsed = Decimal(value)
    except InvalidOperation as exc:
        raise QfError(
            "PARQUET_L2_SEMANTIC_INVALID",
            f"{field} is not a valid decimal value.",
            422,
            {"row": row},
        ) from exc
    if not parsed.is_finite() or parsed < 0:
        raise QfError(
            "PARQUET_L2_SEMANTIC_INVALID",
            f"{field} must be finite and non-negative.",
            422,
            {"row": row},
        )


def validate_parquet(path: Path) -> int:
    try:
        import pyarrow as pa
        import pyarrow.parquet as pq
    except ImportError as exc:
        raise QfError(
            "RESEARCH_RUNTIME_UNAVAILABLE",
            "PyArrow is not installed in the finite-worker runtime.",
            503,
        ) from exc

    try:
        parquet = pq.ParquetFile(path)
    except Exception as exc:
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            "Uploaded file is not readable Parquet.",
            422,
        ) from exc

    schema = parquet.schema_arrow
    if tuple(schema.names) != EXPECTED_COLUMNS:
        raise QfError(
            "PARQUET_L2_SCHEMA_INVALID",
            "Parquet columns do not match the fixed L2 schema.",
            422,
            {"expected": list(EXPECTED_COLUMNS), "actual": schema.names},
        )
    expected_types = {
        "event_id": pa.uint64(),
        "event_index": pa.uint32(),
        "is_snapshot": pa.bool_(),
        "price": pa.string(),
        "size": pa.string(),
        "order_id": pa.uint64(),
        "sequence": pa.uint64(),
        "ts_event_ns": pa.uint64(),
        "ts_init_ns": pa.uint64(),
    }
    for name, expected in expected_types.items():
        if schema.field(name).type != expected:
            raise QfError(
                "PARQUET_L2_SCHEMA_INVALID",
                "Parquet column has an invalid physical type.",
                422,
                {
                    "column": name,
                    "expected": str(expected),
                    "actual": str(schema.field(name).type),
                },
            )
    for name in ("action", "side"):
        kind = schema.field(name).type
        if not (pa.types.is_string(kind) or pa.types.is_dictionary(kind)):
            raise QfError(
                "PARQUET_L2_SCHEMA_INVALID",
                f"{name} must be a UTF-8 string or dictionary-encoded UTF-8 string.",
                422,
            )

    row_count = 0
    previous_event_id: int | None = None
    previous_event_index: int | None = None
    previous_sequence: int | None = None
    for batch in parquet.iter_batches(batch_size=65_536, columns=list(EXPECTED_COLUMNS)):
        columns = {
            name: batch.column(index).to_pylist()
            for index, name in enumerate(EXPECTED_COLUMNS)
        }
        for offset in range(batch.num_rows):
            row = row_count + offset
            event_id = int(columns["event_id"][offset])
            event_index = int(columns["event_index"][offset])
            sequence = int(columns["sequence"][offset])
            action = str(columns["action"][offset]).upper()
            side = str(columns["side"][offset]).upper()
            is_snapshot = bool(columns["is_snapshot"][offset])

            if previous_event_id is None or event_id != previous_event_id:
                if previous_event_id is not None and event_id <= previous_event_id:
                    raise QfError(
                        "PARQUET_L2_SEMANTIC_INVALID",
                        "event_id values must increase when a new event begins.",
                        422,
                        {"row": row},
                    )
                if event_index != 0:
                    raise QfError(
                        "PARQUET_L2_SEMANTIC_INVALID",
                        "Every event must begin at event_index 0.",
                        422,
                        {"row": row},
                    )
                if is_snapshot and action != "CLEAR":
                    raise QfError(
                        "PARQUET_L2_SEMANTIC_INVALID",
                        "Every snapshot event must begin with CLEAR.",
                        422,
                        {"row": row},
                    )
            elif previous_event_index is None or event_index != previous_event_index + 1:
                raise QfError(
                    "PARQUET_L2_SEMANTIC_INVALID",
                    "event_index must be contiguous within one event.",
                    422,
                    {"row": row},
                )

            if action not in {"CLEAR", "ADD", "UPDATE", "DELETE"}:
                raise QfError(
                    "PARQUET_L2_SEMANTIC_INVALID",
                    "action is not one of CLEAR, ADD, UPDATE, DELETE.",
                    422,
                    {"row": row},
                )
            if (action == "CLEAR" and side != "NONE") or (
                action != "CLEAR" and side not in {"BUY", "SELL"}
            ):
                raise QfError(
                    "PARQUET_L2_SEMANTIC_INVALID",
                    "side must be NONE for CLEAR and BUY or SELL otherwise.",
                    422,
                    {"row": row},
                )
            if previous_sequence is not None and sequence < previous_sequence:
                raise QfError(
                    "PARQUET_L2_SEMANTIC_INVALID",
                    "sequence must be non-decreasing.",
                    422,
                    {"row": row},
                )
            _validate_decimal(columns["price"][offset], field="price", row=row)
            _validate_decimal(columns["size"][offset], field="size", row=row)
            if int(columns["ts_init_ns"][offset]) < int(columns["ts_event_ns"][offset]):
                raise QfError(
                    "PARQUET_L2_SEMANTIC_INVALID",
                    "ts_init_ns must not precede ts_event_ns.",
                    422,
                    {"row": row},
                )
            previous_event_id = event_id
            previous_event_index = event_index
            previous_sequence = sequence
        row_count += batch.num_rows
    if row_count == 0:
        raise QfError("PARQUET_L2_SEMANTIC_INVALID", "Parquet dataset has no rows.", 422)
    return row_count


def _bundle_python(settings: Settings, bundle: PluginRuntimeBundle) -> Path:
    root = resolve_plugin_path(settings.plugin_root, bundle.environment_path)
    python = root / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    if not python.is_file():
        raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Runtime bundle Python is missing.", 503)
    return python


def _load_context(
    settings: Settings, job_id: UUID
) -> tuple[ImportContext, sessionmaker[Session]]:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Import job does not exist.", 404)
        run = session.execute(
            select(Run).where(Run.id == job.resource_id).with_for_update()
        ).scalar_one()
        if run.type != "PARQUET_IMPORT" or run.state not in {"QUEUED", "RUNNING"}:
            raise QfError("RUN_INVALID_STATE", "Run is not a queued Parquet import.", 409)
        dataset = session.execute(
            select(CatalogDataset).where(CatalogDataset.run_id == run.id).with_for_update()
        ).scalar_one()
        source = session.get(DataSource, dataset.data_source_id)
        bundle = session.get(PluginRuntimeBundle, run.runtime_bundle_id)
        if source is None or bundle is None or bundle.state != "READY":
            raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Import dependencies are unavailable.", 503)
        release = session.get(PluginRelease, source.plugin_release_id)
        if release is None or release.state not in {"ACTIVE", "DRAINING", "INACTIVE"}:
            raise QfError("PLUGIN_RUNTIME_UNAVAILABLE", "Importer release is unavailable.", 503)
        credential = (
            session.get(CredentialSet, source.credential_set_id)
            if source.credential_set_id is not None
            else None
        )
        run.state = "RUNNING"
        run.started_at = run.started_at or datetime.now(UTC)
        dataset.started_at = dataset.started_at or datetime.now(UTC)
        context = ImportContext(
            run_id=run.id,
            dataset_id=dataset.id,
            source=source,
            bundle=bundle,
            release=release,
            credential=credential,
            instrument_id=dataset.instrument_id,
            metadata=dict(dataset.dataset_metadata),
            catalog_path=dataset.catalog_path,
        )
        for item in (source, bundle, release, credential):
            if item is not None:
                session.expunge(item)
    return context, factory


def run_import(settings: Settings, job_id: UUID) -> None:
    context, factory = _load_context(settings, job_id)
    raw_path = settings.import_root / str(context.run_id) / "upload.parquet"
    temporary_catalog = settings.catalog_root / "staging" / str(context.dataset_id)
    final_catalog = settings.catalog_root / context.catalog_path
    try:
        row_count = validate_parquet(raw_path)
        shutil.rmtree(temporary_catalog, ignore_errors=True)
        temporary_catalog.parent.mkdir(parents=True, exist_ok=True)
        temporary_catalog.mkdir()

        with factory() as session:
            secrets = (
                decrypt_credential_secrets(session, settings, context.credential)
                if context.credential is not None
                else {}
            )
        payload = json.dumps(
            {
                "action": "import_catalog",
                "plugin_id": context.release.plugin_id,
                "public_config": context.source.config,
                "secret_config": secrets,
                "source_path": str(raw_path),
                "catalog_path": str(temporary_catalog),
                "instrument_id": context.instrument_id,
                "metadata": context.metadata,
            },
            separators=(",", ":"),
        )
        result = subprocess.run(
            [
                str(_bundle_python(settings, context.bundle)),
                "-m",
                "quantfoundry.plugins.runtime_call",
            ],
            input=payload,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=settings.parquet_import_timeout_seconds,
            env=os.environ.copy(),
        )
        plugin_result = json.loads(result.stdout)
        if not isinstance(plugin_result, dict):
            raise QfError(
                "PLUGIN_RUNTIME_INVALID",
                "Importer plugin result must be a JSON object.",
                500,
            )
        final_catalog.parent.mkdir(parents=True, exist_ok=True)
        if final_catalog.exists():
            raise QfError("RESOURCE_CONFLICT", "Catalog dataset destination already exists.", 409)
        os.replace(temporary_catalog, final_catalog)

        with factory.begin() as session:
            run = session.get(Run, context.run_id)
            dataset = session.get(CatalogDataset, context.dataset_id)
            assert run is not None and dataset is not None
            dataset.state = "READY"
            dataset.row_count = row_count
            dataset.ended_at = datetime.now(UTC)
            dataset.dataset_metadata = {
                **dataset.dataset_metadata,
                "import_summary": plugin_result.get("summary", {}),
            }
            run.state = "SUCCEEDED"
            run.finished_at = datetime.now(UTC)
            run.summary = {
                **run.summary,
                "dataset_id": str(dataset.id),
                "row_count": row_count,
            }
            append_event(
                session,
                kind="PARQUET_IMPORT_SUCCEEDED",
                aggregate_type="run",
                aggregate_id=run.id,
                payload={"dataset_id": str(dataset.id), "row_count": row_count},
            )
    except Exception as exc:
        shutil.rmtree(temporary_catalog, ignore_errors=True)
        with factory.begin() as session:
            run = session.get(Run, context.run_id)
            dataset = session.get(CatalogDataset, context.dataset_id)
            if run is not None:
                run.state = "FAILED"
                run.finished_at = datetime.now(UTC)
                run.error_code = getattr(exc, "code", type(exc).__name__)
                run.error_message = str(exc)[-4000:]
            if dataset is not None:
                dataset.state = "FAILED"
                dataset.ended_at = datetime.now(UTC)
            append_event(
                session,
                kind="PARQUET_IMPORT_FAILED",
                aggregate_type="run",
                aggregate_id=context.run_id,
                payload={"error_code": getattr(exc, "code", type(exc).__name__)},
            )
        raise
    finally:
        shutil.rmtree(raw_path.parent, ignore_errors=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one Parquet import job")
    parser.add_argument("job_id")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    settings = Settings.from_env()
    settings.ensure_worker_directories()
    run_import(settings, UUID(args.job_id))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
