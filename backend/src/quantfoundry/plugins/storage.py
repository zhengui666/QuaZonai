"""Streaming storage helpers for plugin wheels."""

from __future__ import annotations

import os
from pathlib import Path

from fastapi import UploadFile

from quantfoundry.errors import QfError

MAX_CHUNK_BYTES = 1024 * 1024


def validate_upload_filename(filename: str | None) -> str:
    if not filename:
        raise QfError("PLUGIN_ARTIFACT_INVALID", "Uploaded wheel has no filename.", 422)
    basename = Path(filename).name
    if basename != filename or basename in {"", ".", ".."} or not basename.endswith(".whl"):
        raise QfError(
            "PLUGIN_ARTIFACT_INVALID",
            "Plugin wheel filename must be a basename ending in .whl.",
            422,
            {"filename": filename},
        )
    return basename


async def stream_upload(
    upload: UploadFile,
    destination: Path,
    *,
    max_bytes: int,
) -> int:
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_name(f".{destination.name}.uploading")
    received = 0
    try:
        with temporary.open("xb") as target:
            while True:
                chunk = await upload.read(MAX_CHUNK_BYTES)
                if not chunk:
                    break
                received += len(chunk)
                if received > max_bytes:
                    raise QfError(
                        "PLUGIN_ARTIFACT_INVALID",
                        "Plugin wheel exceeds the configured upload limit.",
                        413,
                        {"max_bytes": max_bytes},
                    )
                target.write(chunk)
            target.flush()
            os.fsync(target.fileno())
        if received == 0:
            raise QfError(
                "PLUGIN_ARTIFACT_INVALID",
                "Plugin wheel is empty.",
                422,
            )
        os.replace(temporary, destination)
        return received
    finally:
        await upload.close()
        temporary.unlink(missing_ok=True)
