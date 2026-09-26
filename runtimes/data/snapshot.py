#!/usr/bin/env python3
"""Plan or acquire explicitly selected, immutable public Hugging Face files."""

import argparse
import datetime
import fnmatch
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request


HUB = "https://huggingface.co"
DEFAULT_MAX_BYTES = 128 * 1024 * 1024
CHUNK = 1024 * 1024


def checked_path(path):
    if (not isinstance(path, str) or not path or "\\" in path
            or any(ord(char) < 32 or ord(char) == 127 for char in path)
            or any(part in ("", ".", "..") for part in path.split("/"))):
        raise ValueError("unsafe repository path")
    return path


def fetch_json(url):
    with urllib.request.urlopen(url, timeout=60) as response:
        body = response.read(32 * CHUNK + 1)
    if len(body) > 32 * CHUNK:
        raise ValueError("repository metadata exceeds 32 MiB")
    value = json.loads(body)
    if not isinstance(value, dict):
        raise ValueError("repository metadata must be an object")
    return value


def plan(dataset, includes, revision=None, max_bytes=DEFAULT_MAX_BYTES, license=None):
    if not re.fullmatch(r"[\w.-]+(?:/[\w.-]+)?", dataset, flags=re.ASCII):
        raise ValueError("dataset must be a Hugging Face repository ID")
    checked_path(dataset)
    if not includes or max_bytes <= 0:
        raise ValueError("explicit --include and positive --max-bytes are required")
    for pattern in includes:
        checked_path(pattern)
    api = f"{HUB}/api/datasets/{dataset}"
    requested = f"/revision/{urllib.parse.quote(revision, safe='')}" if revision else ""
    head = fetch_json(api + requested)
    commit = head.get("sha", "")
    if not isinstance(commit, str) or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError("repository did not resolve to an immutable commit")
    if revision and re.fullmatch(r"[0-9a-f]{40}", revision) and revision != commit:
        raise ValueError("repository resolved to a different requested commit")
    metadata_url = f"{api}/revision/{commit}?blobs=true"
    metadata = fetch_json(metadata_url)
    if metadata.get("sha") != commit or metadata.get("private") or metadata.get("gated"):
        raise ValueError("fixed revision must resolve to the same public, ungated repository")
    siblings = metadata.get("siblings")
    if not isinstance(siblings, list):
        raise ValueError("repository file metadata is missing")
    available = {}
    for item in siblings:
        if not isinstance(item, dict):
            raise ValueError("invalid repository file metadata")
        path = checked_path(item.get("rfilename"))
        if path in available:
            raise ValueError("duplicate repository file metadata")
        available[path] = item
    selected = set()
    for pattern in includes:
        matches = {path for path in available if fnmatch.fnmatchcase(path, pattern)}
        if not matches:
            raise ValueError("an --include pattern matched no files")
        selected.update(matches)
    selected.update(path for path in ("README.md", "SNAPSHOT.json") if path in available)
    if any(path.split("/")[0] == "snapshot.json" for path in selected):
        raise ValueError("repository path conflicts with the local snapshot manifest")
    files = []
    for path in sorted(selected):
        item = available[path]
        size = item.get("size")
        lfs = item.get("lfs")
        if type(size) is not int or not 0 <= size < 2**64:
            raise ValueError("selected file has no valid byte size")
        sha256 = lfs.get("sha256") if isinstance(lfs, dict) else None
        blob_id = item.get("blobId")
        if lfs is not None:
            if (not isinstance(sha256, str) or not re.fullmatch(r"[0-9a-f]{64}", sha256)
                    or lfs.get("size") != size):
                raise ValueError("selected LFS file has invalid integrity metadata")
        elif not isinstance(blob_id, str) or not re.fullmatch(r"[0-9a-f]{40}", blob_id):
            raise ValueError("selected Git file has no valid blob identity")
        files.append({"path": path, "size": size, "sha256": sha256,
                      "git_blob": blob_id if lfs is None else None,
                      "url": f"{HUB}/datasets/{dataset}/resolve/{commit}/{urllib.parse.quote(path)}"})
    total = sum(item["size"] for item in files)
    if total > max_bytes:
        raise ValueError(f"selection requires {total} bytes, exceeding --max-bytes={max_bytes}")
    card = metadata.get("cardData") or {}
    if not isinstance(card, dict):
        raise ValueError("invalid dataset card metadata")
    declared = card.get("license") or license
    if declared is not None and (not isinstance(declared, str) or not declared.strip()):
        raise ValueError("license must be a nonempty string")
    reference = next((item["url"] for item in files if item["path"] == "README.md"), metadata_url)
    return {"repository": dataset, "revision": commit, "license": declared,
            "license_reference": reference, "card": card, "total_bytes": total, "files": files}


def safe_local(root, relative):
    target = root / checked_path(relative)
    for path in (*target.parents, target):
        if path.is_symlink():
            raise ValueError("symlinks are not allowed in snapshot paths")
    return target


def file_hash(path, git_blob=None):
    sha256 = hashlib.sha256()
    blob = hashlib.sha1(f"blob {path.stat().st_size}\0".encode(), usedforsecurity=False)
    with path.open("rb") as stream:
        while chunk := stream.read(CHUNK):
            sha256.update(chunk)
            if git_blob:
                blob.update(chunk)
    if git_blob and blob.hexdigest() != git_blob:
        raise ValueError("existing file Git blob mismatch")
    return sha256.hexdigest()


def publish_bytes(path, content):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".snapshot-", suffix=".partial") as temp:
        temp.write(content)
        temp.flush()
        os.fsync(temp.fileno())
        os.link(temp.name, path)  # Atomic publication; never replace an existing file.


def acquire_file(root, item):
    target = safe_local(root, item["path"])
    if target.exists():
        if not target.is_file() or target.stat().st_size != item["size"]:
            raise ValueError("existing file conflicts with the selected snapshot")
        existing_hash = file_hash(target)
        if item["sha256"]:
            if existing_hash != item["sha256"]:
                raise ValueError("existing file SHA-256 mismatch")
            return existing_hash
    else:
        existing_hash = None
    target.parent.mkdir(parents=True, exist_ok=True)
    safe_local(root, item["path"])
    # Tiny regular Git files need a fresh comparison when no completed manifest exists.
    with tempfile.NamedTemporaryFile(dir=target.parent, prefix=".snapshot-", suffix=".partial") as temp:
        sha256 = hashlib.sha256()
        git_blob = hashlib.sha1(f"blob {item['size']}\0".encode(), usedforsecurity=False)
        size = 0
        with urllib.request.urlopen(item["url"], timeout=60) as response:
            while chunk := response.read(min(CHUNK, item["size"] - size + 1)):
                size += len(chunk)
                if size > item["size"]:
                    raise ValueError("download exceeds its declared byte size")
                temp.write(chunk)
                sha256.update(chunk)
                git_blob.update(chunk)
        digest = sha256.hexdigest()
        if size != item["size"] or (item["sha256"] and digest != item["sha256"]):
            raise ValueError("download size or SHA-256 mismatch")
        if item["git_blob"] and git_blob.hexdigest() != item["git_blob"]:
            raise ValueError("download Git blob mismatch")
        if existing_hash is not None:
            if digest != existing_hash:
                raise ValueError("existing file SHA-256 mismatch")
        else:
            temp.flush()
            os.fsync(temp.fileno())
            safe_local(root, item["path"])
            os.link(temp.name, target)
        return digest


def download(selection, output):
    if not selection["license"]:
        raise ValueError("dataset card has no license; supply the verified source terms with --license")
    root = Path(os.path.abspath(output))
    manifest_path = safe_local(root, "snapshot.json")
    for item in selection["files"]:
        safe_local(root, item["path"])
    identity = {key: selection[key] for key in ("repository", "revision", "license", "license_reference")}
    if manifest_path.exists():
        existing = fetch_local_manifest(manifest_path)
        if (existing.get("schema_version") != 1
                or any(existing.get(key) != value for key, value in identity.items())
                or not isinstance(existing.get("files"), list)
                or len(existing["files"]) != len(selection["files"])):
            raise ValueError("existing snapshot manifest conflicts with selection")
        for recorded, item in zip(existing["files"], selection["files"]):
            if (not isinstance(recorded, dict)
                    or any(recorded.get(key) != item[key] for key in ("path", "size", "url"))
                    or not isinstance(recorded.get("sha256"), str)
                    or not re.fullmatch(r"[0-9a-f]{64}", recorded["sha256"])
                    or (item["sha256"] and item["sha256"] != recorded["sha256"])):
                raise ValueError("existing snapshot manifest conflicts with file metadata")
            target = safe_local(root, item["path"])
            if (not target.is_file() or target.stat().st_size != item["size"]
                    or file_hash(target, item["git_blob"]) != recorded["sha256"]):
                raise ValueError("existing snapshot file size or SHA-256 mismatch")
        return existing
    root.mkdir(parents=True, exist_ok=True)
    files = []
    for item in selection["files"]:
        digest = acquire_file(root, item)
        files.append({key: item[key] for key in ("path", "size", "url")} | {"sha256": digest})
    manifest = {"schema_version": 1, **identity,
                "retrieved_at": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
                "files": files}
    safe_local(root, "snapshot.json")
    publish_bytes(manifest_path, (json.dumps(manifest, indent=2) + "\n").encode())
    return manifest


def fetch_local_manifest(path):
    with path.open("rb") as stream:
        body = stream.read(32 * CHUNK + 1)
    if len(body) > 32 * CHUNK:
        raise ValueError("existing snapshot manifest exceeds 32 MiB")
    result = json.loads(body)
    if not isinstance(result, dict):
        raise ValueError("existing snapshot manifest must be an object")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "download"))
    parser.add_argument("--dataset", required=True, help="public Hugging Face dataset ID")
    parser.add_argument("--include", action="append", required=True, help="explicit repository glob (repeatable)")
    parser.add_argument("--revision", help="branch, tag or immutable commit; resolved before file selection")
    parser.add_argument("--max-bytes", type=int, default=DEFAULT_MAX_BYTES)
    parser.add_argument("--license", help="verified source terms, only when dataset card omits a license")
    parser.add_argument("--output", type=Path, help="snapshot directory (required for download)")
    args = parser.parse_args(argv)
    if args.command == "download" and args.output is None:
        parser.error("download requires --output")
    try:
        selection = plan(args.dataset, args.include, args.revision, args.max_bytes, args.license)
        result = download(selection, args.output) if args.command == "download" else selection
        print(json.dumps(result, indent=2))
        return 0
    except urllib.error.HTTPError as error:
        print(f"snapshot: public source HTTP error {error.code}", file=sys.stderr)
    except (OSError, ValueError, urllib.error.URLError, http.client.HTTPException) as error:
        # Avoid echoing signed redirect URLs, server bodies or local file contents.
        message = str(error) if type(error) is ValueError else "source or local I/O failed"
        print(f"snapshot: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
