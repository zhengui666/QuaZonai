#!/usr/bin/env python3
"""Export/check the canonical FastAPI OpenAPI wire contract."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path

os.environ.setdefault("QUAZONAI_ENV", "test")
os.environ.setdefault("QUAZONAI_DATABASE_URL", "sqlite+pysqlite:///:memory:")
os.environ.setdefault("QUAZONAI_ALEMBIC_URL", "sqlite+pysqlite:///:memory:")

from main import create_app  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "contracts" / "openapi" / "quazonai-v1.json"
SWIFT_OUTPUT = ROOT / "ios" / "Packages" / "QuaZonaiAPI" / "Sources" / "QuaZonaiAPI" / "openapi.yaml"


def rendered_contract() -> str:
    schema = create_app().openapi()
    return json.dumps(schema, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    rendered = rendered_contract()
    if args.write:
        OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        OUTPUT.write_text(rendered, encoding="utf-8")
        SWIFT_OUTPUT.parent.mkdir(parents=True, exist_ok=True)
        SWIFT_OUTPUT.write_text(rendered, encoding="utf-8")
    if args.check:
        for path in (OUTPUT, SWIFT_OUTPUT):
            if not path.is_file() or path.read_text(encoding="utf-8") != rendered:
                print(f"OpenAPI drift: run tools/export_openapi.py --write ({path})")
                return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
