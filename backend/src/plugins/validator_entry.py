"""Short-lived plugin descriptor loader used inside validation environments."""

from __future__ import annotations

import argparse
import json
from collections.abc import Callable
from importlib import metadata
from typing import Any

from plugins.contract import DescriptorSnapshot


def _materialize(value: Any) -> Any:
    current = value
    if isinstance(current, type):
        current = current()
    elif callable(current) and not hasattr(current, "descriptor"):
        current = current()
    return current


def load_snapshot(*, plugin_id: str, version: str) -> DescriptorSnapshot:
    candidates = [
        item
        for item in metadata.entry_points(group="quazonai.plugins")
        if item.name == plugin_id
    ]
    if len(candidates) != 1:
        raise RuntimeError(
            f"expected one quazonai.plugins entry point named {plugin_id!r}; "
            f"found {len(candidates)}"
        )

    loaded = _materialize(candidates[0].load())
    if isinstance(loaded, DescriptorSnapshot):
        snapshot = loaded
    else:
        descriptor: Callable[[], Any] | None = getattr(loaded, "descriptor", None)
        if descriptor is None or not callable(descriptor):
            raise RuntimeError("plugin entry point must expose descriptor()")
        snapshot = DescriptorSnapshot.model_validate(descriptor())

    if snapshot.plugin_id != plugin_id:
        raise RuntimeError(
            f"descriptor plugin_id {snapshot.plugin_id!r} does not match entry point {plugin_id!r}"
        )
    if snapshot.version != version:
        raise RuntimeError(
            f"descriptor version {snapshot.version!r} does not match wheel version {version!r}"
        )
    if snapshot.api_version != "1":
        raise RuntimeError(f"unsupported plugin api_version: {snapshot.api_version!r}")
    return snapshot


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Validate one QuaZonai plugin")
    parser.add_argument("--plugin-id", required=True)
    parser.add_argument("--version", required=True)
    return parser


def main() -> int:
    args = build_parser().parse_args()
    snapshot = load_snapshot(plugin_id=args.plugin_id, version=args.version)
    print(json.dumps(snapshot.model_dump(mode="json"), separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
