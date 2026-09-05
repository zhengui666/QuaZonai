"""Export pip's native wheel integrity hashes as a platform-specific lock.

Input wheels must first be downloaded with --require-hashes against the complete
upstream-resolved lock. This is dependency packaging, not a QZ business hash gate.
No application data, secret, identity or approval uses these dependency hashes.
"""

from pathlib import Path
import subprocess
import sys

from packaging.utils import parse_wheel_filename


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: lock_science_runtime.py VERIFIED_WHEEL_DIR NEW_LOCK_PATH")
    wheel_directory, output = map(Path, sys.argv[1:])
    wheels = sorted(wheel_directory.glob("*.whl"))
    if not wheels:
        raise SystemExit("verified wheel directory is empty")
    lines: dict[str, str] = {}
    for wheel in wheels:
        name, version, _, _ = parse_wheel_filename(wheel.name)
        if name in lines:
            raise SystemExit("multiple wheels for the same distribution")
        hashed = subprocess.run(
            [sys.executable, "-m", "pip", "hash", "--algorithm", "sha256", str(wheel)],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        integrity = [line for line in hashed.stdout.splitlines() if line.startswith("--hash=sha256:")]
        if len(integrity) != 1:
            raise SystemExit("unexpected pip hash output")
        lines[name] = f"{name}=={version} {integrity[0]}"
    with output.open("x", encoding="utf-8") as handle:
        handle.write("# Platform lock: Linux x86_64, CPython 3.12.12; tested on Ubuntu 24.04.\n")
        handle.write("# Generated from wheels verified by pip --require-hashes; do not edit manually.\n")
        handle.write("\n".join(lines[name] for name in sorted(lines)) + "\n")


if __name__ == "__main__":
    main()
