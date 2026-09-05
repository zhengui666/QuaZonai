"""Export pip-verified wheels; check the direct input/lock contract offline.

This deliberately supports this repository's single-platform exact-pin format,
not pip's complete requirements language. PEP 508/440 parsing is delegated to
packaging. Wheel integrity remains pip's responsibility; there is no QZ business
hash gate. --check neither resolves, downloads, installs nor rewrites anything.
"""

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

from packaging.requirements import Requirement
from packaging.utils import canonicalize_name, parse_wheel_filename
from packaging.version import Version

ROOTS_HEADER = "# Direct requirements v1: "


def exact_pin(text: str) -> tuple[str, Version]:
    """Reject unsupported requirements rather than silently mischecking them."""
    requirement = Requirement(text)
    pins = list(requirement.specifier)
    if (
        requirement.url is not None
        or requirement.marker is not None
        or requirement.extras
        or len(pins) != 1
        or pins[0].operator != "=="
        or "*" in pins[0].version
    ):
        raise ValueError(f"expected an unconditional exact distribution pin: {text!r}")
    return canonicalize_name(requirement.name), Version(pins[0].version)


def collect_pins(lines: list[str]) -> dict[str, Version]:
    pins: dict[str, Version] = {}
    for text in lines:
        name, version = exact_pin(text)
        if name in pins:
            raise ValueError(f"duplicate distribution: {name}")
        pins[name] = version
    if not pins:
        raise ValueError("direct dependency set must not be empty")
    return pins


def read_inputs(path: Path) -> dict[str, Version]:
    return collect_pins(
        [
            line.strip()
            for line in path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        ]
    )


def roots_header(pins: dict[str, Version]) -> str:
    return ROOTS_HEADER + json.dumps([f"{name}=={pins[name]}" for name in sorted(pins)])


def check_lock(input_path: Path, lock_path: Path) -> None:
    inputs = read_inputs(input_path)
    recorded_roots: dict[str, Version] | None = None
    locked: dict[str, Version] = {}
    for raw in lock_path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if line.startswith(ROOTS_HEADER):
            if recorded_roots is not None:
                raise ValueError("duplicate direct dependency header")
            values = json.loads(line.removeprefix(ROOTS_HEADER))
            if not isinstance(values, list) or any(not isinstance(value, str) for value in values):
                raise ValueError("direct dependency header must be a JSON array of pins")
            recorded_roots = collect_pins(values)
        elif line and not line.startswith("#"):
            # The exporter emits one exact pin and one native wheel hash per line.
            fields = line.split()
            if len(fields) != 2 or re.fullmatch(r"--hash=sha256:[0-9a-f]{64}", fields[1]) is None:
                raise ValueError("unsupported platform-lock line; regenerate with the exporter")
            name, version = exact_pin(fields[0])
            if name in locked:
                raise ValueError(f"duplicate locked distribution: {name}")
            locked[name] = version
    if recorded_roots is None:
        raise ValueError("missing direct dependency header; regenerate the platform lock")
    if recorded_roots != inputs:
        raise ValueError("requirements.in changed; regenerate and review the platform lock")
    for name, version in inputs.items():
        if locked.get(name) != version:
            raise ValueError(f"direct dependency absent or stale in platform lock: {name}=={version}")


def export_lock(wheel_directory: Path, input_path: Path, output: Path) -> None:
    """Only export a wheel directory already verified against a resolved lock."""
    inputs = read_inputs(input_path)
    wheels = sorted(wheel_directory.glob("*.whl"))
    if not wheels:
        raise ValueError("verified wheel directory is empty")
    lines: dict[str, str] = {}
    versions: dict[str, Version] = {}
    for wheel in wheels:
        name, version, _, _ = parse_wheel_filename(wheel.name)
        if name in lines:
            raise ValueError("multiple wheels for the same distribution")
        hashed = subprocess.run(
            [sys.executable, "-m", "pip", "hash", "--algorithm", "sha256", str(wheel)],
            check=True,
            capture_output=True,
            text=True,
            timeout=30,
        )
        integrity = [line for line in hashed.stdout.splitlines() if line.startswith("--hash=sha256:")]
        if len(integrity) != 1:
            raise ValueError("unexpected pip hash output")
        versions[name] = version
        lines[name] = f"{name}=={version} {integrity[0]}"
    for name, version in inputs.items():
        if versions.get(name) != version:
            raise ValueError(f"verified wheels do not contain input pin: {name}=={version}")
    with output.open("x", encoding="utf-8") as handle:
        handle.write("# Platform lock: Linux x86_64, CPython 3.12.12; tested on Ubuntu 24.04.\n")
        handle.write("# Generated from wheels verified by pip --require-hashes; do not edit manually.\n")
        handle.write(roots_header(inputs) + "\n")
        handle.write("\n".join(lines[name] for name in sorted(lines)) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("check", help="offline, read-only input-to-lock consistency check")
    check.add_argument("input", type=Path)
    check.add_argument("lock", type=Path)
    export = commands.add_parser("export", help="export previously verified platform wheels")
    export.add_argument("wheels", type=Path)
    export.add_argument("input", type=Path)
    export.add_argument("output", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "check":
            check_lock(args.input, args.lock)
            print("direct dependency input matches the committed platform lock")
        else:
            export_lock(args.wheels, args.input, args.output)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"science lock: {error}\n")


if __name__ == "__main__":
    main()
