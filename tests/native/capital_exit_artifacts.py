#!/usr/bin/env python3
"""Resolve only this checkout's two Job test artifacts from one Cargo JSON build.

This reads compiler evidence; it never searches target directories or runs tests.
"""
import json
import os
from pathlib import Path
import sys


def resolve_artifacts(messages, root):
    root = root.resolve()
    manifest = root / "apps/job/Cargo.toml"
    expected = (
        ("native_capital_exit", "test", root / "apps/job/tests/native_capital_exit.rs"),
        ("job", "lib", root / "apps/job/src/lib.rs"),
    )
    paths = [set(), set()]
    finished = 0
    for message in messages:
        if message.get("reason") == "build-finished":
            if message.get("success") is not True:
                raise ValueError("Cargo did not finish successfully")
            finished += 1
        target = message.get("target", {})
        if message.get("reason") != "compiler-artifact":
            continue
        for index, (name, kind, source) in enumerate(expected):
            if target.get("name") != name or target.get("kind") != [kind]:
                continue
            if (Path(message.get("manifest_path", "")).resolve() != manifest
                    or Path(target.get("src_path", "")).resolve() != source
                    or message.get("profile", {}).get("test") is not True
                    or not {"native-paper-test", "native-sandbox-test"}.issubset(
                        message.get("features", []))):
                continue
            executable = message.get("executable")
            if executable:
                paths[index].add(Path(executable).resolve(strict=True))
    if finished != 1 or any(len(group) != 1 for group in paths):
        raise ValueError("expected one successful Cargo build and exactly one current-source artifact per Job test target")
    result = [next(iter(group)) for group in paths]
    if result[0] == result[1]:
        raise ValueError("Job integration and lib test artifacts must be different")
    if any(not path.is_file() or not os.access(path, os.X_OK) or "\n" in str(path)
           for path in result):
        raise ValueError("Cargo-reported Job test artifact is not executable")
    return result


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: capital_exit_artifacts.py <cargo-jsonl>")
    messages = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
    try:
        artifacts = resolve_artifacts(messages, Path(__file__).resolve().parents[2])
    except (ValueError, OSError) as error:
        raise SystemExit(str(error)) from error
    print("\n".join(str(path) for path in artifacts))
