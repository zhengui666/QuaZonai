#!/usr/bin/env python3
"""Plan or acquire explicitly selected, immutable public Hugging Face files."""

import argparse
from contextlib import contextmanager
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
import time
import urllib.error
import urllib.parse
import urllib.request


HUB = "https://huggingface.co"
DEFAULT_MAX_BYTES = None  # No implicit total byte budget.
CHUNK = 1024 * 1024


def checked_path(path):
    if (not isinstance(path, str) or not path or "\\" in path
            or any(ord(char) < 32 or ord(char) == 127 for char in path)
            or any(part in ("", ".", "..") for part in path.split("/"))):
        raise ValueError("unsafe repository path")
    return path


def fetch_json(url):
    body = fetch_bytes(url)
    value = json.loads(body)
    if not isinstance(value, dict):
        raise ValueError("repository metadata must be an object")
    return value


def fetch_bytes(url, limit=None):
    with urllib.request.urlopen(url, timeout=60) as response:
        body = response.read() if limit is None else response.read(limit + 1)
    if limit is not None and len(body) > limit:
        raise ValueError("repository metadata exceeds its byte limit")
    return body


def repository_metadata(dataset, revision=None):
    """Resolve ordinary Hub refs once and list that exact revision's public files."""
    if not re.fullmatch(r"[\w.-]+(?:/[\w.-]+)?", dataset, flags=re.ASCII):
        raise ValueError("dataset must be a Hugging Face repository ID")
    checked_path(dataset)
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
    return commit, available, metadata, metadata_url


def repository_file_url(dataset, revision, path):
    return f"{HUB}/datasets/{dataset}/resolve/{revision}/{urllib.parse.quote(checked_path(path))}"


def plan(dataset, includes, revision=None, max_bytes=DEFAULT_MAX_BYTES, license=None):
    if not includes:
        raise ValueError("explicit --include is required")
    if max_bytes is not None and (type(max_bytes) is not int or max_bytes <= 0):
        raise ValueError("explicit --max-bytes must be positive")
    for pattern in includes:
        checked_path(pattern)
    commit, available, metadata, metadata_url = repository_metadata(dataset, revision)
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
    if max_bytes is not None and total > max_bytes:
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
        body = stream.read()
    result = json.loads(body)
    if not isinstance(result, dict):
        raise ValueError("existing snapshot manifest must be an object")
    return result


def replace_json(path, value):
    """Atomically update our small cache metadata, never the source data file."""
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".hf-state-", delete=False) as stream:
        temporary = Path(stream.name)
        try:
            stream.write((json.dumps(value, indent=2, allow_nan=False) + "\n").encode())
            stream.flush()
            os.fsync(stream.fileno())
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)


@contextmanager
def cache_lock(path):
    """The OS releases this per-file lock after an interrupted or crashed process."""
    with path.open("a+b") as stream:
        try:
            import fcntl
        except ImportError:  # Standalone Python users on Windows.
            import msvcrt
            if stream.seek(0, os.SEEK_END) == 0:
                stream.write(b"\0")
                stream.flush()
            stream.seek(0)
            msvcrt.locking(stream.fileno(), msvcrt.LK_LOCK, 1)
            try:
                yield
            finally:
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            fcntl.flock(stream.fileno(), fcntl.LOCK_EX)
            try:
                yield
            finally:
                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)


def acquire_cached_file(root, item, inspect):
    """Shared public Hub transport: size/format checks, reusable cache and HTTP resume.

    The caller owns repository/revision selection. No checksums are calculated
    on this path. Failed bytes remain partial until a complete response validates.
    """
    target = safe_local(root, "files/" + item["path"])
    # The original path gets a directory of fixed control filenames. Adding a
    # suffix directly would collide for valid source paths like a / a.lock/b.
    partial = safe_local(root, "transfers/" + item["path"] + "/data.partial")
    state = safe_local(root, "transfers/" + item["path"] + "/state.json")
    lock = safe_local(root, "transfers/" + item["path"] + "/lock")
    for path in (target, partial, state, lock):
        path.parent.mkdir(parents=True, exist_ok=True)
        safe_local(root, path.relative_to(root).as_posix())
    with cache_lock(lock):
        identity = {key: item[key] for key in ("url", "size", "format")}
        if state.exists() and fetch_local_manifest(state) != identity:
            raise ValueError("partial download belongs to different file metadata")
        if target.exists():
            validation = inspect(target, item)
            return {"local_path": str(target), "cached": True, "resumed_bytes": 0,
                    "validation": validation}
        if partial.exists() and not partial.is_file():
            raise ValueError("partial download is not a regular file")
        if partial.exists() and not state.exists():
            raise ValueError("partial download metadata is missing")
        offset = partial.stat().st_size if partial.exists() else 0
        if offset > item["size"]:
            raise ValueError("partial download exceeds its declared byte size")
        # A crash might occur after the final write but before response EOF or
        # publication. Recheck the last byte through Range instead of assuming
        # a full-size partial was a completed transfer.
        if offset and offset == item["size"]:
            offset -= 1
        replace_json(state, identity)
        headers = {"Accept-Encoding": "identity", "User-Agent": "QuaZonai-public-data/1.0"}
        if offset:
            headers["Range"] = f"bytes={offset}-"
        request = urllib.request.Request(item["url"], headers=headers)
        with urllib.request.urlopen(request, timeout=60) as response:
            status = response.status
            if response.headers.get("Content-Encoding", "identity") != "identity":
                raise ValueError("Hub file response has unsupported content encoding")
            if offset and status == 206:
                expected = f"bytes {offset}-{item['size'] - 1}/{item['size']}"
                if response.headers.get("Content-Range") != expected:
                    raise ValueError("Hub file response has an unexpected byte range")
            elif status == 200:
                offset = 0  # The server ignored Range: restart, never append a full body.
            else:
                raise ValueError("Hub file response has an unexpected HTTP status")
            declared = response.headers.get("Content-Length")
            if declared is not None and (not re.fullmatch(r"[0-9]+", declared)
                                         or int(declared) != item["size"] - offset):
                raise ValueError("Hub file response has an unexpected byte size")
            mode = "r+b" if partial.exists() else "w+b"
            with partial.open(mode) as stream:
                stream.seek(offset)
                stream.truncate()
                size = offset
                try:
                    while chunk := response.read(min(CHUNK, item["size"] - size + 1)):
                        if size + len(chunk) > item["size"]:
                            raise ValueError("Hub file response exceeds its declared byte size")
                        stream.write(chunk)
                        size += len(chunk)
                finally:
                    stream.flush()
                    os.fsync(stream.fileno())
            if size != item["size"]:
                raise ValueError("Hub file response was truncated; partial bytes retained")
        try:
            validation = inspect(partial, item)
        except ValueError:
            # Keep the actual rejected source bytes for diagnosis; the next
            # request starts a fresh partial rather than retrying invalid bytes.
            partial.rename(partial.with_name(partial.name + f".rejected-{time.time_ns()}"))
            state.unlink()
            raise
        safe_local(root, target.relative_to(root).as_posix())
        os.link(partial, target)  # Publish complete data atomically without replacing existing files.
        partial.unlink()
        state.unlink()
        return {"local_path": str(target), "cached": False, "resumed_bytes": offset,
                "validation": validation}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "download"))
    parser.add_argument("--dataset", required=True, help="public Hugging Face dataset ID")
    parser.add_argument("--include", action="append", required=True, help="explicit repository glob (repeatable)")
    parser.add_argument("--revision", help="branch, tag or immutable commit; resolved before file selection")
    parser.add_argument("--max-bytes", type=int, default=DEFAULT_MAX_BYTES,
                        help="optional explicit total file byte budget; default unlimited")
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
