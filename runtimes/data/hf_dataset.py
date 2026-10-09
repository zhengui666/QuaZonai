"""On-demand Hub dataset selections using the shared snapshot metadata/transport.

File partitions come from an actual repository index, never guessed filenames.
This downloads source files; it does not decode native research observations.
"""

import csv
from contextlib import ExitStack
import datetime
import fnmatch
import json
import os
from pathlib import Path
import re
import stat
import sys
from threading import Lock

import acquire
import providers
import snapshot


INDEX_SCHEMA = "qz.hf_partitions/1"
PLAN_SCHEMA = "qz.hf_dataset_plan/1"
SELECTION_SCHEMA = "qz.hf_selection/1"
MAX_INDEX_BYTES = None
MAX_SELECTION_BYTES = None
FORMATS = ("parquet", "json", "jsonl", "csv", "zip", "gzip", "opaque")
CSV_FIELD_LOCK = Lock()


def date(value):
    if not isinstance(value, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}", value):
        raise ValueError("partition dates must be YYYY-MM-DD")
    try:
        return datetime.date.fromisoformat(value)
    except ValueError:
        raise ValueError("invalid partition date") from None


def inferred_format(path):
    suffix = Path(path).suffix.lower().removeprefix(".")
    return {"ndjson": "jsonl", "gz": "gzip"}.get(suffix, suffix if suffix in FORMATS else "opaque")


def partition_markets(item):
    """An explicit unknown mapping is never an empty or all-market universe."""
    values = item.get("markets")
    unknown = "market_mapping" in item
    if (not isinstance(values, list)
            or any(not isinstance(m, str) or not m.strip() for m in values)
            or len(set(values)) != len(values)
            or (unknown and (item["market_mapping"] != "UNKNOWN" or values))
            or (not unknown and not values)):
        raise ValueError("invalid partition market mapping")
    return values, unknown


def partition_files(index, available, markets, start_date, end_date, includes=None):
    if not isinstance(index, dict) or index.get("schema") != INDEX_SCHEMA:
        raise ValueError("unsupported Hugging Face partition index")
    entries = index.get("files")
    if not isinstance(entries, list) or not entries:
        raise ValueError("invalid partition index file list")
    selected, seen, indexed_markets = {}, set(), set()
    for item in entries:
        if not isinstance(item, dict):
            raise ValueError("invalid partition index entry")
        path = snapshot.checked_path(item.get("path"))
        source_markets, unknown = partition_markets(item)
        if path in seen or path not in available or item.get("format") not in FORMATS:
            raise ValueError("partition index does not describe an available file")
        seen.add(path)
        indexed_markets.update(source_markets)
        start, end = date(item.get("start_date")), date(item.get("end_date"))
        if start >= end:
            raise ValueError("partition index requires start_date < end_date")
        if start_date is not None and (end <= start_date or start >= end_date):
            continue
        if includes and not any(fnmatch.fnmatchcase(path, pattern) for pattern in includes):
            continue
        if markets:
            if unknown:
                raise ValueError("market selection requires known membership for every candidate partition")
            if not set(markets).intersection(source_markets):
                continue
        selected[path] = {key: item[key] for key in ("markets", "start_date", "end_date", "format")}
        if unknown:
            selected[path]["market_mapping"] = "UNKNOWN"
    if set(markets) - indexed_markets:
        raise ValueError("a requested market is absent from the partition index")
    return selected


def plan(dataset, includes=None, revision=None, max_bytes=snapshot.DEFAULT_MAX_BYTES,
         manifest=None, markets=None, start_date=None, end_date=None):
    includes, markets = list(includes or []), list(markets or [])
    if max_bytes is not None:
        providers.integer(max_bytes, "max_bytes", minimum=1, maximum=None)
    if any(not isinstance(m, str) or not m.strip() for m in markets):
        raise ValueError("market selectors must be nonempty strings")
    if bool(start_date) != bool(end_date):
        raise ValueError("both --start-date and --end-date are required")
    start, end = (date(start_date), date(end_date)) if start_date else (None, None)
    if start is not None and start >= end:
        raise ValueError("selection requires start_date < end_date")
    if not manifest and (markets or start is not None):
        raise ValueError("market/date selection requires an actual --manifest partition index")
    if not manifest and not includes:
        raise ValueError("explicit --include files or a filtered --manifest are required")
    if manifest and not (markets or start is not None or includes):
        raise ValueError("partition download requires an explicit market, date or file selector")
    for pattern in includes:
        snapshot.checked_path(pattern)
    if manifest:
        snapshot.checked_path(manifest)
    commit, available, metadata, _ = snapshot.repository_metadata(dataset, revision)
    partitions, index_record = {}, None
    if manifest:
        info = available.get(manifest)
        if info is None:
            raise ValueError("partition index is missing from the requested repository revision")
        providers.integer(info.get("size"), "partition index byte size", maximum=MAX_INDEX_BYTES)
        url = snapshot.repository_file_url(dataset, commit, manifest)
        body = snapshot.fetch_bytes(url, MAX_INDEX_BYTES)
        if len(body) != info["size"]:
            raise ValueError("partition index byte size differs from repository metadata")
        partitions = partition_files(providers.read_json(body), available, markets, start, end, includes)
        selected = set(partitions)
        index_record = {"path": manifest, "size": len(body), "url": url}
    else:
        selected = set(available)
    if includes:
        matched = set()
        for pattern in includes:
            paths = {p for p in selected if fnmatch.fnmatchcase(p, pattern)}
            if not paths:
                raise ValueError("an --include pattern matched no requested files")
            matched.update(paths)
        selected = matched
    if not selected:
        raise ValueError("no indexed files overlap the requested selection")
    files = []
    for path in sorted(selected):
        size = providers.integer(available[path].get("size"), "source file byte size", maximum=None)
        item = {"path": path, "size": size,
                "url": snapshot.repository_file_url(dataset, commit, path),
                "format": partitions[path]["format"] if path in partitions else inferred_format(path)}
        if path in partitions:
            item["partition"] = partitions[path]
        files.append(item)
    total = sum(f["size"] for f in files)
    if max_bytes is not None and total > max_bytes:
        raise ValueError(f"requested files require {total} bytes, exceeding --max-bytes={max_bytes}")
    card = metadata.get("cardData") or {}
    license = card.get("license") if isinstance(card, dict) else None
    return {"schema": PLAN_SCHEMA, "repository": dataset, "requested_revision": revision or "main",
            "revision": commit, "license": license, "partition_index": index_record,
            "request": {"includes": includes, "markets": markets,
                        "start_date": start_date, "end_date": end_date},
            "selection_bounds": "[start_date,end_date)", "download_granularity": "FILE_PARTITION",
            "coverage": "NOT_ASSERTED",
            "max_bytes": max_bytes, "total_bytes": total, "files": files}


def inspect_file(path, item):
    """Ordinary byte/format checks only, with the validation scope reported explicitly."""
    if not path.is_file() or path.stat().st_size != item["size"]:
        raise ValueError("cached file is missing, not regular, or has an unexpected byte size")
    format = item["format"]
    with path.open("rb") as stream:
        prefix = stream.read(4096)
        if format == "parquet":
            if item["size"] < 12 or prefix[:4] != b"PAR1":
                raise ValueError("source file has an invalid Parquet header")
            stream.seek(-4, os.SEEK_END)
            if stream.read(4) != b"PAR1":
                raise ValueError("source file has an incomplete Parquet footer")
            return "PARQUET_ENVELOPE"
        if format in ("gzip", "zip"):
            signatures = {"gzip": (b"\x1f\x8b",), "zip": (b"PK\x03\x04", b"PK\x05\x06", b"PK\x07\x08")}
            if not prefix.startswith(signatures[format]):
                raise ValueError("source file has an invalid archive header")
            return "ARCHIVE_HEADER"
        if format == "opaque":
            return "BYTE_SIZE_ONLY"
        if not prefix.strip() or prefix.lstrip().lower().startswith((b"<!doctype html", b"<html")):
            raise ValueError("source file does not contain the declared text format")
        stream.seek(0)
        if format == "json":
            providers.read_json(stream.read())
            return "JSON_DOCUMENT"
        if format == "jsonl":
            first = stream.readline()
            providers.read_json(first)
            return "JSONL_FIRST_ROW"
        def csv_lines():
            first = True
            for line in stream:
                yield line.decode("utf-8-sig" if first else "utf-8")
                first = False
        try:
            # Remove csv's 128 KiB application default. sys.maxsize is the
            # parser's platform length range, not a data budget. Restore the
            # process-global setting across concurrent inspections.
            with CSV_FIELD_LOCK:
                previous_limit = csv.field_size_limit(sys.maxsize)
                try:
                    next(csv.reader(csv_lines(), strict=True))
                finally:
                    csv.field_size_limit(previous_limit)
        except (UnicodeError, csv.Error, StopIteration):
            raise ValueError("source file has an invalid CSV first row") from None
        return "CSV_FIRST_ROW"


def validate_plan(selection):
    if not isinstance(selection, dict) or selection.get("schema") != PLAN_SCHEMA:
        raise ValueError("unsupported on-demand dataset plan")
    repository, revision = selection.get("repository"), selection.get("revision")
    if (not isinstance(repository, str) or not re.fullmatch(r"[\w.-]+(?:/[\w.-]+)?", repository, re.ASCII)
            or not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision)):
        raise ValueError("invalid on-demand dataset revision")
    snapshot.checked_path(repository)
    if (not isinstance(selection.get("requested_revision"), str) or not selection["requested_revision"].strip()
            or selection.get("coverage") != "NOT_ASSERTED"
            or selection.get("download_granularity") != "FILE_PARTITION"
            or selection.get("selection_bounds") != "[start_date,end_date)"):
        raise ValueError("on-demand selection scope differs from its original file-partition contract")
    request = selection.get("request")
    if not isinstance(request, dict):
        raise ValueError("on-demand selection has no original request")
    includes, markets = request.get("includes"), request.get("markets")
    if (not isinstance(includes, list) or not isinstance(markets, list)
            or any(not isinstance(m, str) or not m.strip() for m in markets)):
        raise ValueError("invalid on-demand file or market request")
    for pattern in includes:
        snapshot.checked_path(pattern)
    lower, upper = request.get("start_date"), request.get("end_date")
    if lower is None and upper is None:
        lower_date = upper_date = None
    else:
        lower_date, upper_date = date(lower), date(upper)
        if lower_date >= upper_date:
            raise ValueError("invalid on-demand request date range")
    if not (includes or markets or lower_date is not None):
        raise ValueError("on-demand selection has no explicit file, market or date request")
    index = selection.get("partition_index")
    if index is None:
        if markets or lower_date is not None:
            raise ValueError("on-demand market/date request has no actual partition index")
    elif not isinstance(index, dict):
        raise ValueError("invalid on-demand partition index record")
    else:
        index_path = snapshot.checked_path(index.get("path"))
        providers.integer(index.get("size"), "partition index byte size", maximum=MAX_INDEX_BYTES)
        if index.get("url") != snapshot.repository_file_url(repository, revision, index_path):
            raise ValueError("partition index source differs from its selected repository revision")
    budget = selection.get("max_bytes")
    if budget is not None:
        providers.integer(budget, "max_bytes", minimum=1, maximum=None)
    files = selection.get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("invalid on-demand file list")
    seen, total = set(), 0
    for item in files:
        if not isinstance(item, dict):
            raise ValueError("invalid on-demand file record")
        path = snapshot.checked_path(item.get("path"))
        if (path in seen or item.get("format") not in FORMATS
                or item.get("url") != snapshot.repository_file_url(repository, revision, path)):
            raise ValueError("on-demand file identity differs from the repository revision")
        seen.add(path)
        if includes and not any(fnmatch.fnmatchcase(path, pattern) for pattern in includes):
            raise ValueError("selected file does not match the original explicit file request")
        partition = item.get("partition")
        if index is not None:
            if not isinstance(partition, dict):
                raise ValueError("selected file has no original partition mapping")
            source_markets, unknown = partition_markets(partition)
            if partition.get("format") != item["format"] or (unknown and markets):
                raise ValueError("invalid original partition market/format mapping")
            a, b = date(partition.get("start_date")), date(partition.get("end_date"))
            if (a >= b or (markets and not set(markets).intersection(source_markets))
                    or (lower_date is not None and (b <= lower_date or a >= upper_date))):
                raise ValueError("selected file does not overlap its original partition request")
        elif partition is not None:
            raise ValueError("selected file claims a partition mapping without its source index")
        total += providers.integer(item.get("size"), "source file byte size", maximum=budget)
    declared_total = providers.integer(selection.get("total_bytes"), "total_bytes", maximum=budget)
    if ((budget is not None and total > budget) or total != declared_total
            or any(not any(fnmatch.fnmatchcase(path, pattern) for path in seen) for pattern in includes)):
        raise ValueError("on-demand selection exceeds or differs from its byte budget")


def cache_root(cache_dir, selection):
    root = Path(os.path.abspath(cache_dir))
    return snapshot.safe_local(root, f"datasets/{selection['repository']}/{selection['revision']}")


def selection_result(selection, root, files, retrieved_at):
    return {"schema": SELECTION_SCHEMA, "plan": selection, "cache_root": str(root), "files": files,
            "retrieved_at": retrieved_at,
            "downloaded_bytes": sum(f["size"] - f["resumed_bytes"] for f in files if not f["cached"]),
            "cached_files": sum(f["cached"] for f in files)}


def manifest_bytes(value):
    return (json.dumps(value, indent=2, allow_nan=False) + "\n").encode()


def same_request(left, right):
    # main/tag/returned-commit aliases can identify the exact same selected
    # revision. Preserve the original spelling without blocking that retry.
    return (isinstance(left, dict) and isinstance(right, dict)
            and {k: v for k, v in left.items() if k != "requested_revision"}
            == {k: v for k, v in right.items() if k != "requested_revision"})


def freeze_observation(path):
    """Ordinary identity/stat observation, without hashing or following symlinks."""
    snapshot.safe_local(path.parent, path.name)
    try:
        observed = path.lstat()
    except FileNotFoundError:
        raise ValueError(f"offline freeze input is missing: {path}") from None
    if not stat.S_ISREG(observed.st_mode):
        raise ValueError("offline freeze requires an existing regular file")
    return (observed.st_dev, observed.st_ino, observed.st_size,
            observed.st_mtime_ns, observed.st_ctime_ns)


def freeze(plan_path, cache_dir, output):
    """Publish an existing fixed plan/cache handoff; never acquire missing bytes."""
    plan_path = Path(os.path.abspath(plan_path))
    plan_state = freeze_observation(plan_path)
    selection = snapshot.fetch_local_manifest(plan_path)
    validate_plan(selection)
    if plan_state != freeze_observation(plan_path):
        raise ValueError("offline freeze input changed")
    root = cache_root(cache_dir, selection)
    output = Path(os.path.abspath(output))
    manifest_path = snapshot.safe_local(output, "selection.json")
    request_path = snapshot.safe_local(output, "request.json")
    lock = snapshot.safe_local(output, ".hf-request.lock")
    if output.exists():
        raise ValueError("offline freeze output must be a new directory")
    output.mkdir(parents=True, exist_ok=False)
    with snapshot.cache_lock(lock), ExitStack() as locks:
        # Keep every selected cache lock until final publication. A writer
        # honoring the shared locks must not change an earlier file after its
        # final stat check while a later file is still being checked.
        # All freeze requests acquire these locks in the same stable order;
        # download retains its original one-file-at-a-time locking behavior.
        for item in sorted(selection["files"], key=lambda item: item["path"]):
            target = snapshot.safe_local(root, "files/" + item["path"])
            # Missing completed sources fail before control-directory creation;
            # neither partial adoption nor source acquisition is attempted.
            freeze_observation(target)
            file_lock = snapshot.safe_local(root, "transfers/" + item["path"] + "/lock")
            file_lock.parent.mkdir(parents=True, exist_ok=True)
            snapshot.safe_local(root, file_lock.relative_to(root).as_posix())
            if file_lock.exists():
                freeze_observation(file_lock)
            locks.enter_context(snapshot.cache_lock(file_lock))
        observations = {plan_path: plan_state}
        files = []
        for item in selection["files"]:
            target = snapshot.safe_local(root, "files/" + item["path"])
            before = freeze_observation(target)
            state = snapshot.safe_local(root, "transfers/" + item["path"] + "/state.json")
            if state.exists():
                observations[state] = freeze_observation(state)
                identity = {key: item[key] for key in ("url", "size", "format")}
                if snapshot.fetch_local_manifest(state) != identity:
                    raise ValueError("cached source belongs to different file metadata")
            else:
                observations[state] = None
            validation = inspect_file(target, item)
            if before != freeze_observation(target):
                raise ValueError("offline freeze input changed")
            observations[target] = before
            files.append({**item, "local_path": str(target), "cached": True,
                          "resumed_bytes": 0, "validation": validation})
        result = selection_result(selection, root, files,
            datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"))
        validate_manifest(result)

        def unchanged():
            # Check the entire set again: an earlier file or the original plan
            # may change while a later file is being inspected.
            for path, before in observations.items():
                snapshot.safe_local(path.parent, path.name)
                after = freeze_observation(path) if path.exists() else None
                if before != after:
                    raise ValueError("offline freeze input changed")

        unchanged()
        snapshot.publish_bytes(request_path, manifest_bytes(selection))
        unchanged()
        snapshot.publish_bytes(manifest_path, manifest_bytes(result))
        return result


def download(selection, cache_dir, output):
    validate_plan(selection)
    output = Path(os.path.abspath(output))
    manifest_path = snapshot.safe_local(output, "selection.json")
    request_path = snapshot.safe_local(output, "request.json")
    lock = snapshot.safe_local(output, ".hf-request.lock")
    if output.exists() and (not output.is_dir() or any(
            p.name not in ("request.json", "selection.json", ".hf-request.lock")
            and not p.name.startswith(".snapshot-") for p in output.iterdir())):
        raise ValueError("existing selection output contains unrelated files")
    output.mkdir(parents=True, exist_ok=True)
    with snapshot.cache_lock(lock):
        return download_locked(selection, cache_dir, output, manifest_path, request_path)


def download_locked(selection, cache_dir, output, manifest_path, request_path):
    if manifest_path.exists():
        existing = snapshot.fetch_local_manifest(manifest_path)
        if not same_request(existing.get("plan"), selection):
            raise ValueError("existing selection output belongs to a different request")
        verify(manifest_path)
        return existing
    if request_path.exists():
        original = snapshot.fetch_local_manifest(request_path) if request_path.is_file() else None
        if not same_request(original, selection):
            raise ValueError("existing selection output belongs to a different request")
        validate_plan(original)
        selection = original
    else:
        snapshot.publish_bytes(request_path, manifest_bytes(selection))
    root = cache_root(cache_dir, selection)
    files = [{**item, **snapshot.acquire_cached_file(root, item, inspect_file)} for item in selection["files"]]
    result = selection_result(selection, root, files,
        datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"))
    snapshot.publish_bytes(manifest_path, manifest_bytes(result))
    return result


def verify(selection_path):
    path = Path(os.path.abspath(selection_path))
    snapshot.safe_local(path.parent, path.name)
    result = snapshot.fetch_local_manifest(path)
    return validate_manifest(result, check_files=True)


def validate_manifest(result, check_files=False):
    """Validate preserved request/file identities; optional original-cache byte/format readback."""
    if not isinstance(result, dict):
        raise ValueError("on-demand selection manifest must be an object")
    if result.get("schema") != SELECTION_SCHEMA:
        raise ValueError("unsupported on-demand selection manifest")
    plan = result.get("plan")
    validate_plan(plan)
    retrieved_at = acquire.utc_clock(result.get("retrieved_at"))
    if retrieved_at > acquire.utc_clock(acquire.now()):
        raise ValueError("on-demand selection observation is in the future")
    if not isinstance(result.get("cache_root"), str):
        raise ValueError("invalid on-demand cache root")
    root = Path(result["cache_root"])
    if (not root.is_absolute() or ".." in root.parts or not isinstance(result.get("files"), list)
            or len(result["files"]) != len(plan["files"])):
        raise ValueError("invalid on-demand cache file list")
    downloaded, cached = 0, 0
    for item, recorded in zip(plan["files"], result["files"]):
        target = (snapshot.safe_local(root, "files/" + item["path"]) if check_files
                  else root / "files" / item["path"])
        if (not isinstance(recorded, dict) or set(recorded) != set(item) | {"local_path", "cached", "resumed_bytes", "validation"}
                or any(recorded.get(k) != v for k, v in item.items())
                or recorded.get("local_path") != str(target)):
            raise ValueError("cached file record differs from the original request")
        resumed = providers.integer(recorded.get("resumed_bytes"), "resumed_bytes", maximum=item["size"])
        scopes = {"parquet": ("PARQUET_ENVELOPE",), "zip": ("ARCHIVE_HEADER",), "gzip": ("ARCHIVE_HEADER",),
                  "json": ("JSON_DOCUMENT", "JSON_PREFIX_ONLY"), "jsonl": ("JSONL_FIRST_ROW",),
                  "csv": ("CSV_FIRST_ROW",), "opaque": ("BYTE_SIZE_ONLY",)}
        if type(recorded.get("cached")) is not bool or recorded.get("validation") not in scopes[item["format"]]:
            raise ValueError("cached file record has an invalid byte/format validation scope")
        cached += int(recorded["cached"])
        downloaded += 0 if recorded["cached"] else item["size"] - resumed
        if check_files:
            inspect_file(target, item)
    if (providers.integer(result.get("downloaded_bytes"), "downloaded_bytes", maximum=plan["total_bytes"]) != downloaded
            or providers.integer(result.get("cached_files"), "cached_files", maximum=len(plan["files"])) != cached):
        raise ValueError("cached file totals differ from the original request")
    return {"repository": plan["repository"], "revision": plan["revision"],
            "files": len(plan["files"]), "total_bytes": plan["total_bytes"],
            "validation": "BYTE_SIZE_AND_DECLARED_FORMAT" if check_files else "REQUEST_FILE_IDENTITIES_ONLY",
            "download_granularity": "FILE_PARTITION"}


def native_window(selection, start_seconds, end_seconds):
    """A native UTC window may narrow, never widen a requested partition date range."""
    providers.integer(start_seconds, "start_seconds")
    providers.integer(end_seconds, "end_seconds")
    if start_seconds >= end_seconds:
        raise ValueError("selection requires start_seconds < end_seconds")
    request = selection["plan"].get("request")
    if not isinstance(request, dict):
        raise ValueError("on-demand selection has no original request")
    start, end = request.get("start_date"), request.get("end_date")
    if bool(start) != bool(end):
        raise ValueError("on-demand selection has an incomplete date request")
    if start:
        lower = datetime.datetime.combine(date(start), datetime.time.min, datetime.timezone.utc)
        upper = datetime.datetime.combine(date(end), datetime.time.min, datetime.timezone.utc)
        if lower >= upper or start_seconds < int(lower.timestamp()) or end_seconds > int(upper.timestamp()):
            raise ValueError("native conversion window exceeds the requested partition dates")


def file_observations(selection):
    """Ordinary local file observations catch mutation during native conversion without hashes."""
    result = []
    for item in selection["files"]:
        path = snapshot.safe_local(Path(selection["cache_root"]), "files/" + item["path"])
        observed = path.stat()
        result.append((str(path), observed.st_dev, observed.st_ino, observed.st_size,
                       observed.st_mtime_ns, observed.st_ctime_ns))
    return result
