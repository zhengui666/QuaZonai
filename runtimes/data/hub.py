#!/usr/bin/env python3
"""Select public daily market-data files using a declarative source profile."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import datetime
import fnmatch
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request


HUB = "https://huggingface.co"


def relative_path(value):
    path = PurePosixPath(value)
    if (not value or path.is_absolute() or ".." in path.parts or "\\" in value
            or any(ord(c) < 32 for c in value)):
        raise ValueError("invalid repository file path")
    return value


def read_json(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        return json.load(response)


def select_files(profile, siblings, start, end, kinds, path_prefixes=None):
    """Every declared daily path must be accounted for, including missing partitions."""
    if start >= end:
        raise ValueError("start-date must precede exclusive end-date")
    streams = [s for s in profile["streams"] if not kinds or s["kind"] in kinds]
    if not streams or (kinds and set(kinds) - {s["kind"] for s in streams}):
        raise ValueError("unknown or empty stream selection")
    prefixes = sorted(set(path_prefixes or []))
    for prefix in prefixes:
        relative_path(prefix)
        if not prefix.endswith("/") or not any(
                template.startswith(prefix) for stream in streams for template in stream["paths"]):
            raise ValueError("path-prefix must select a declared stream directory")
    paths = {relative_path(f["rfilename"]): f for f in siblings}
    selected = {}
    missing = []
    day = start
    while day < end:
        for stream in streams:
            for template in stream["paths"]:
                if prefixes and not any(template.startswith(prefix) for prefix in prefixes):
                    continue
                pattern = relative_path(template.format(date=day.strftime(stream["date_format"])))
                matches = ([p for p in paths if fnmatch.fnmatchcase(p, pattern)]
                           if any(c in pattern for c in "*?[") else
                           [pattern] if pattern in paths else [])
                if not matches:
                    missing.append({"stream": stream["name"], "date": day.isoformat(), "pattern": pattern})
                for path in matches:
                    if path in selected and selected[path]["stream"] != stream["name"]:
                        raise ValueError("file matches more than one stream")
                    selected[path] = {"path": path, "size": paths[path]["size"], "stream": stream["name"]}
        day += datetime.timedelta(days=1)
    for path in ("README.md", "LICENSE", "LICENSE.txt"):
        if path in paths:
            selected[path] = {"path": path, "size": paths[path]["size"], "stream": None}
    if not any(f["stream"] for f in selected.values()):
        raise ValueError("no market-data files match the selection")
    return sorted(selected.values(), key=lambda f: f["path"]), missing


def plan(profile, start, end, kinds, revision=None, path_prefixes=None):
    repo = profile["repository"]
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repo, flags=re.ASCII):
        raise ValueError("repository must be a public Hugging Face dataset ID")
    requested = revision or profile.get("revision", "main")
    api = f"{HUB}/api/datasets/{repo}/revision/{urllib.parse.quote(requested, safe='')}?blobs=true"
    metadata = read_json(api)
    commit = metadata["sha"]
    if metadata.get("private") or metadata.get("gated"):
        raise ValueError("source must be public and ungated")
    terms = (metadata.get("cardData") or {}).get("license") or profile.get("license")
    if not terms:
        raise ValueError("source license is missing")
    files, missing = select_files(profile, metadata["siblings"], start, end, kinds, path_prefixes)
    for item in files:
        item["url"] = f"{HUB}/datasets/{repo}/resolve/{commit}/{urllib.parse.quote(item['path'])}"
    result = {"schema_version": 1, "repository": repo, "revision": commit,
            "license": terms, "attribution": profile.get("attribution", ""),
            "profile": profile, "start_date": start.isoformat(), "end_date": end.isoformat(),
            "total_bytes": sum(f["size"] for f in files), "missing": missing, "files": files}
    if path_prefixes:
        result["selected_path_prefixes"] = sorted(set(path_prefixes))
    return result


def local_file(root, path):
    result = root / relative_path(path)
    if result.is_symlink() or any(p.is_symlink() for p in result.parents):
        raise ValueError("download paths must not contain symlinks")
    return result


def acquire_file(root, item):
    target = local_file(root, item["path"])
    if target.exists():
        if not target.is_file() or target.stat().st_size != item["size"]:
            raise ValueError("existing source file has a different size")
        return
    target.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=target.parent, prefix=".download-", suffix=".partial") as temp:
        count = 0
        with urllib.request.urlopen(item["url"], timeout=120) as response:
            while chunk := response.read(1024 * 1024):
                count += len(chunk)
                temp.write(chunk)
        if count != item["size"]:
            raise ValueError(f"source download size mismatch: {item['path']} expected {item['size']}, received {count}")
        temp.flush()
        os.link(temp.name, target)
    print(f"downloaded {item['path']} ({count} bytes)", file=sys.stderr)


def selection_identity(value):
    result = {k: v for k, v in value.items() if k != "retrieved_at"}
    # Source-profile hints do not alter an already explicit file/stream selection.
    # Keep every profile field used by planning/decoding; the actual files, revision,
    # terms, dates and optional path selection remain exact top-level bindings.
    result["profile"] = {k: v for k, v in result["profile"].items()
                         if k in ("repository", "revision", "license", "attribution", "streams")}
    return result


def acquire(selection, output):
    root = output.absolute()
    root.mkdir(parents=True, exist_ok=True)
    manifest = local_file(root, "market-data.json")
    if manifest.exists():
        with manifest.open() as stream:
            previous = json.load(stream)
        if selection_identity(previous) != selection_identity(selection):
            raise ValueError("existing download belongs to a different selection")
    else:
        previous = None
    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(lambda item: acquire_file(root, item), selection["files"]))
    if previous is not None:
        return previous
    result = selection | {"retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
    with manifest.open("x") as stream:
        json.dump(result, stream, indent=2)
        stream.write("\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "download"))
    parser.add_argument("--profile", required=True, type=Path)
    parser.add_argument("--start-date", required=True, type=datetime.date.fromisoformat)
    parser.add_argument("--end-date", required=True, type=datetime.date.fromisoformat,
                        help="exclusive UTC date")
    parser.add_argument("--kind", action="append", help="profile record kind; omit for all streams")
    parser.add_argument("--path-prefix", action="append", help="explicit declared stream directory, ending in /; repeat to select multiple directories")
    parser.add_argument("--revision", help="override the profile's repository revision")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.command == "download" and args.output is None:
        parser.error("download requires --output")
    try:
        with args.profile.open() as stream:
            profile = json.load(stream)
        selection = plan(profile, args.start_date, args.end_date, args.kind, args.revision, args.path_prefix)
        result = acquire(selection, args.output) if args.command == "download" else selection
        print(json.dumps(result, indent=2))
    except urllib.error.HTTPError as error:
        print(f"market-data: source HTTP {error.code}", file=sys.stderr)
        return 1
    except (OSError, ValueError, KeyError, urllib.error.URLError) as error:
        message = str(error) if isinstance(error, (ValueError, KeyError)) else f"source or local I/O failed ({type(error).__name__})"
        print(f"market-data: {message}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
