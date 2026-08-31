#!/usr/bin/env python3
"""Generate deterministic, dependency-free iOS release artwork used by CI archives."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "ios" / "Resources" / "Assets.xcassets" / "AppIcon.appiconset" / "AppIcon-1024.png"


def chunk(kind: bytes, payload: bytes) -> bytes:
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload) & 0xFFFFFFFF)


def main() -> int:
    size = 1024
    rows = []
    for y in range(size):
        row = bytearray([0])
        for x in range(size):
            # Original QZ mark: dark research-grid background with a bright diagonal signal.
            signal = max(0.0, 1.0 - abs((x - y) - 64) / 170.0)
            radial = max(0.0, 1.0 - (((x - 512) ** 2 + (y - 512) ** 2) ** 0.5) / 720.0)
            r = int(min(255, 24 + 64 * radial + 115 * signal))
            g = int(min(255, 30 + 83 * radial + 72 * signal))
            b = int(min(255, 43 + 98 * radial + 35 * signal))
            row.extend((r, g, b, 255))
        rows.append(bytes(row))
    raw = b"".join(rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT.write_bytes(png)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
