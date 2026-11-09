#!/usr/bin/env python3
"""Plan, freeze or verify selected free public-source observations (never research admission)."""

import argparse
from copy import deepcopy
from dataclasses import asdict
import datetime
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import stat
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

from providers import MAX_RECORDS, MAX_REQUESTS, Selection, integer, provider_by_id, PROVIDERS, read_json
from snapshot import publish_bytes, safe_local


# Retain manifest field names; null means no application-imposed cap.
MAX_RESPONSE_BYTES = None
DEFAULT_MAX_BYTES = None
MAX_OUTPUT_BYTES = None
MAX_TERMS_BYTES = None
MAX_MANIFEST_BYTES = None
SCHEMA = "qz.public_acquisition/1"
ADMISSION = {"coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
             "research_qualified": False, "registered_in_quazonai": False,
             "permission_status": "REQUIRES_INDEPENDENT_REVIEW"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = urllib.request.build_opener(NoRedirect)


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def json_bytes(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode()


def file_record(path, body):
    # Retained for the separate immutable-archive adapter.
    return {"path": path, "size": len(body), "sha256": hashlib.sha256(body).hexdigest()}


def plan(provider_id, selection, max_bytes=DEFAULT_MAX_BYTES):
    provider = provider_by_id(provider_id)
    requests = provider.plan(selection)
    if not requests:
        raise ValueError("provider request count is invalid")
    cursor = selection.start_seconds
    for request in requests:
        url = urllib.parse.urlsplit(request.url)
        if (request.method != "GET" or url.scheme != "https" or url.port is not None
                or url.hostname not in provider.descriptor["hosts"] or url.username is not None
                or url.password is not None or url.fragment
                or request.start_seconds != cursor or request.end_seconds <= cursor
                or request.end_seconds > selection.end_seconds):
            raise ValueError("provider requires fixed official HTTPS GETs and disjoint bounded windows")
        cursor = request.end_seconds
    if cursor != selection.end_seconds:
        raise ValueError("provider request windows do not cover the selection")
    return {"schema": SCHEMA, "provider": deepcopy(provider.descriptor), "selection": asdict(selection),
            "selection_bounds": "[start_seconds,end_seconds)",
            "limits": {"max_response_bytes": MAX_RESPONSE_BYTES, "max_response_total_bytes": None,
                       "max_requests": MAX_REQUESTS, "max_records": MAX_RECORDS,
                       "max_output_bytes": MAX_OUTPUT_BYTES},
            "requests": [asdict(request) for request in requests], "admission": dict(ADMISSION)}


def fetch(url, limit=None):
    """One request, no redirects, retries, credentials, cookies or paid fallback."""
    request = urllib.request.Request(url, headers={"Accept": "application/json",
                                     "Accept-Encoding": "identity",
                                     "User-Agent": "QuaZonai-public-data/1.0"})
    started = now()
    with OPENER.open(request, timeout=30) as response:
        media_type = response.headers.get("Content-Type", "").split(";", 1)[0].strip().lower()
        if (response.status != 200 or response.headers.get("Content-Encoding", "identity") != "identity"
                or media_type != "application/json"):
            raise ValueError("unexpected public-source HTTP response")
        declared = response.headers.get("Content-Length")
        if declared is not None:
            if not re.fullmatch(r"[0-9]+", declared):
                raise ValueError("invalid response Content-Length")
            declared = int(declared)
        body = bytearray()
        while True:
            chunk = response.read1(64 * 1024)
            if not chunk:
                break
            body.extend(chunk)
        if declared is not None and len(body) != declared:
            raise ValueError("response truncated or Content-Length differs")
        headers = {name: response.headers[name] for name in ("Content-Type", "Date", "ETag", "Last-Modified")
                   if name in response.headers}
    return bytes(body), {"status": 200, "headers": headers,
                         "request_started_at": started, "retrieved_at": now()}


def local_bytes(path, limit=None):
    path = Path(os.path.abspath(path))
    safe_local(path.parent, path.name)
    if not path.is_file():
        raise ValueError("local input missing or not a regular file")
    flags = (os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
             | getattr(os, "O_BINARY", 0))
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode):
            raise ValueError("local input is not a regular file")
        body = stream.read()
        after = os.fstat(stream.fileno())
    safe_local(path.parent, path.name)
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if (len(body) != before.st_size
            or identity(before) != identity(after) or identity(after) != identity(path.stat())):
        raise ValueError("local input changed during read")
    return body


def utc_clock(value):
    if (not isinstance(value, str)
            or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,6})?(?:Z|\+00:00)", value)):
        raise ValueError("invalid UTC provenance clock")
    result = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if result.utcoffset() != datetime.timedelta(0):
        raise ValueError("invalid UTC provenance clock")
    return result


def check_observation(observation, selection, previous_retrieved=None, publication=None):
    if observation["status"] != 200 or not isinstance(observation["headers"], dict):
        raise ValueError("invalid source response metadata")
    started = utc_clock(observation["request_started_at"])
    retrieved = utc_clock(observation["retrieved_at"])
    if started > retrieved or selection.end_seconds > int(started.timestamp()):
        raise ValueError("invalid source observation clock")
    if previous_retrieved is not None and started < previous_retrieved:
        raise ValueError("sequential response clocks overlap or move backwards")
    if publication is not None and retrieved > publication:
        raise ValueError("publication clock precedes source retrieval")
    return retrieved


def decoded_page(provider, body, selection, request, observation, path):
    source_rows = provider.decode(body, selection)
    rows = []
    for index, source in enumerate(source_rows):
        if request["start_seconds"] <= source["selection_time_seconds"] < request["end_seconds"]:
            rows.append({**source, "instrument": selection.instrument,
                         "kind": provider.descriptor["record_kind"],
                         "interval_seconds": selection.interval_seconds,
                         "observed_at": observation["retrieved_at"],
                         "historical_available_at": None,
                         "source_response": path, "source_row_index": index})
    return rows, {"source_rows": len(source_rows), "selected_rows": len(rows),
                  "outside_request_window": len(source_rows) - len(rows)}


def collect_rows(accumulated, rows):
    for row in rows:
        timestamp = row["selection_time_seconds"]
        if timestamp in accumulated:
            raise ValueError("overlapping or duplicate selected observations")
        accumulated[timestamp] = row


def record_bytes(accumulated):
    return b"".join(json_bytes(accumulated[key]) for key in sorted(accumulated))


def checked_file(root, item, expected_path, limit):
    # Older acquisitions may carry a checksum; it is retained as source metadata,
    # never calculated or treated as evidence of source authenticity.
    if (not isinstance(item, dict) or item.get("path") != expected_path
            or set(item) - {"path", "size", "sha256"}):
        raise ValueError("unexpected acquisition file path")
    body = local_bytes(safe_local(root, expected_path), limit)
    if type(item.get("size")) is not int or item["size"] != len(body):
        raise ValueError("acquisition file size mismatch")
    return body


def verify(output):
    """Reconstruct source records and clocks; never certify checksums or permission."""
    root = Path(os.path.abspath(output))
    manifest = read_json(local_bytes(safe_local(root, "acquisition.json"), MAX_MANIFEST_BYTES))
    if not isinstance(manifest, dict) or manifest.get("schema") != SCHEMA:
        raise ValueError("unsupported acquisition manifest")
    try:
        selection = Selection(**manifest["selection"])
        provider = provider_by_id(manifest["provider"]["id"])
        original_plan = plan(provider.descriptor["id"], selection,
                             manifest["limits"]["max_response_total_bytes"])
        # Recognize the exact historical envelope without imposing its old
        # read/output caps or rewriting frozen provenance. Other mutations fail.
        recorded_limits = manifest["limits"]
        original_plan["limits"]["max_response_total_bytes"] = recorded_limits["max_response_total_bytes"]
        legacy_limits = {"max_response_bytes": 4 * 1024 * 1024,
                         "max_response_total_bytes": recorded_limits["max_response_total_bytes"],
                         "max_requests": 128, "max_records": 100_000,
                         "max_output_bytes": 128 * 1024 * 1024}
        if json_bytes(recorded_limits) not in (json_bytes(original_plan["limits"]), json_bytes(legacy_limits)):
            raise ValueError("invalid acquisition limit metadata")
        original_plan["limits"] = recorded_limits
        expected_keys = set(original_plan) | {"created_at", "source_terms", "responses", "records",
                                              "record_count", "raw_response_bytes", "observation_status"}
        if (set(manifest) != expected_keys
                or json_bytes({key: manifest.get(key) for key in original_plan}) != json_bytes(original_plan)):
            raise ValueError("manifest no longer matches the provider plan or admission boundary")
        terms = manifest["source_terms"]
        if (terms["reference"] != provider.descriptor["terms_reference"]
                or terms["evidence_status"] != "OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED"):
            raise ValueError("invalid source permission evidence")
        if not checked_file(root, terms["file"], "source-terms.bin", MAX_TERMS_BYTES).strip():
            raise ValueError("missing source terms")
        responses = manifest["responses"]
        if not isinstance(responses, list) or len(responses) != len(original_plan["requests"]):
            raise ValueError("incomplete acquisition responses")
        publication = utc_clock(manifest["created_at"])
        previous_retrieved = None
        accumulated, raw_size = {}, 0
        for index, (response, request) in enumerate(zip(responses, original_plan["requests"])):
            if response["request"] != request or response["observation"]["status"] != 200:
                raise ValueError("source request or response mismatch")
            observation = response["observation"]
            previous_retrieved = check_observation(observation, selection, previous_retrieved, publication)
            path = f"raw/{index:04d}.json"
            body = checked_file(root, response["file"], path, MAX_RESPONSE_BYTES)
            raw_size += len(body)
            rows, counts = decoded_page(provider, body, selection, request, observation, path)
            if counts != response["counts"]:
                raise ValueError("source counts mismatch")
            collect_rows(accumulated, rows)
        records = checked_file(root, manifest["records"], "records.jsonl", MAX_OUTPUT_BYTES)
        if (records != record_bytes(accumulated) or manifest["record_count"] != len(accumulated)
                or manifest["raw_response_bytes"] != raw_size
                or manifest["observation_status"] != ("OBSERVED" if accumulated else "NO_OBSERVATIONS")):
            raise ValueError("derived records differ from original source responses")
    except (KeyError, TypeError, AttributeError, OverflowError):
        raise ValueError("malformed acquisition manifest") from None
    return {"schema": SCHEMA, "integrity": "SOURCE_RECORDS_VALIDATED", "provider": provider.descriptor["id"],
            "record_count": len(accumulated), "admission": dict(ADMISSION)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("providers", help="list capabilities and unsupported native handoffs")
    checking = commands.add_parser("verify", help="offline source-record and clock reconstruction; not research admission")
    checking.add_argument("--output", type=Path, required=True)
    for command in ("plan",):
        sub = commands.add_parser(command)
        sub.add_argument("--provider", choices=sorted(PROVIDERS), required=True)
        sub.add_argument("--instrument", required=True, help="explicit provider token/product ID")
        sub.add_argument("--start-seconds", type=int, required=True)
        sub.add_argument("--end-seconds", type=int, required=True)
        sub.add_argument("--interval-seconds", type=int, required=True)
        sub.add_argument("--max-bytes", type=int, default=DEFAULT_MAX_BYTES,
                         help="legacy option accepted for compatibility; downloads are not byte-budgeted")
    args = parser.parse_args(argv)
    try:
        if args.command == "providers":
            result = [provider.descriptor for provider in PROVIDERS.values()]
        elif args.command == "verify":
            result = verify(args.output)
        else:
            selection = Selection(args.instrument, args.start_seconds, args.end_seconds, args.interval_seconds)
            result = plan(args.provider, selection, args.max_bytes)
        print(json.dumps(result, indent=2))
        return 0
    except urllib.error.HTTPError as error:
        error.close()
        print(f"acquire: public source HTTP error {error.code}; no authenticated or paid fallback", file=sys.stderr)
    except (OSError, ValueError, urllib.error.URLError, http.client.HTTPException) as error:
        # Never echo server bodies, local source contents or unexpected request details.
        message = str(error) if type(error) is ValueError else "source or local I/O failed"
        print(f"acquire: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
