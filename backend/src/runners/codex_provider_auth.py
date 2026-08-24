"""One-shot Codex model-provider auth helper.

The Codex App Server executes this command before its first provider request. The
actual provider credential stays in the trusted Mission runner process and is
returned over a short-lived Unix-domain socket; it is never placed in the App
Server environment or command line.
"""

from __future__ import annotations

import argparse
import socket
from pathlib import Path


MAX_TOKEN_BYTES = 32 * 1024


def fetch_token(socket_path: Path) -> str:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(5.0)
        client.connect(str(socket_path))
        client.sendall(b"TOKEN\n")
        chunks: list[bytes] = []
        total = 0
        while True:
            chunk = client.recv(4096)
            if not chunk:
                break
            total += len(chunk)
            if total > MAX_TOKEN_BYTES:
                raise RuntimeError("Codex provider token broker returned too much data")
            chunks.append(chunk)
    token = b"".join(chunks).decode("utf-8").strip()
    if not token:
        raise RuntimeError("Codex provider token broker returned an empty token")
    return token


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Fetch one Codex provider bearer token")
    parser.add_argument("socket_path")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    print(fetch_token(Path(args.socket_path)), flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
