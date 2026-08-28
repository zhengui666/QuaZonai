"""Local sealed-host catalog provisioning; never exposed over HTTP."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
from typing import Sequence

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine
from quazonai_nautilus_gateway.models import CatalogIngestRequest

def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Provision a sealed catalog locally on the remote Gateway host")
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--data-root", type=Path, default=None)
    args = parser.parse_args(argv)
    role = os.getenv("NAUTILUS_GATEWAY_ROLE", "").strip().upper()
    if role != "SEALED":
        raise SystemExit("NAUTILUS_GATEWAY_ROLE=SEALED is required")
    root = args.data_root or Path(os.getenv("NAUTILUS_GATEWAY_DATA_ROOT", "/tmp/quazonai-nautilus"))
    request = CatalogIngestRequest.model_validate(json.loads(args.input.read_text(encoding="utf-8")))
    result = NautilusGatewayEngine(root).ingest(request)
    print(json.dumps({"catalog_key": result["catalog_key"], "catalog_uri": result["catalog_uri"]}))
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
