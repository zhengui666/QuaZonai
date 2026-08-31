#!/usr/bin/env python3
"""Materialize deterministic binary assets that cannot be stored by text-only agents."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ICON = ROOT / "Issue36Resources" / "Assets.xcassets" / "AppIcon.appiconset" / "AppIcon.png"


def chunk(kind: bytes, payload: bytes) -> bytes:
    return (
        struct.pack(">I", len(payload))
        + kind
        + payload
        + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)
    )


def render_icon(size: int = 1024) -> bytes:
    rows: list[bytes] = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            nx = x / (size - 1)
            ny = y / (size - 1)
            glow = max(0.0, 1.0 - ((nx - 0.68) ** 2 + (ny - 0.28) ** 2) ** 0.5 * 1.7)
            red = int(20 + 38 * nx + 30 * glow)
            green = int(22 + 74 * (1 - ny) + 88 * glow)
            blue = int(38 + 88 * nx + 106 * glow)
            row.extend((min(red, 255), min(green, 255), min(blue, 255), 255))
        rows.append(bytes(row))
    signature = b"\x89PNG\r\n\x1a\n"
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return signature + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(b"".join(rows), 9)) + chunk(b"IEND", b"")


def main() -> None:
    ICON.parent.mkdir(parents=True, exist_ok=True)
    ICON.write_bytes(render_icon())
    print(f"materialized {ICON.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
