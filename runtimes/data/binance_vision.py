#!/usr/bin/env python3
"""Offline inspection of one operator-supplied Binance Vision spot kline archive.

No acquisition, native conversion, source authentication or research admission.
Use zipfile/csv public APIs; see binance-vision.md for container validation limits.
"""

import argparse
from copy import deepcopy
import csv
from dataclasses import asdict, dataclass
import datetime as dt
from decimal import Decimal
import hashlib
import io
import os
from pathlib import Path
import re
import stat
import sys
import zipfile
import zlib

from acquire import file_record, json_bytes
from providers import read_json
from snapshot import publish_bytes, safe_local


SCHEMA = "qz.public_archive_acquisition/1"
PROVIDER = {"id": "binance-vision-spot-klines", "version": 1, "venue": "BINANCE",
            "market": "SPOT", "record_kind": "OHLCV_CANDLE"}
PARSER = "binance-vision-spot-csv/1"
SPEC_REVISION = "bd110bb04caad6ad964a0098809f18343b1e104b"
INTERVALS = {"1m": 60, "3m": 180, "5m": 300, "15m": 900, "30m": 1800,
             "1h": 3600, "2h": 7200, "4h": 14400, "6h": 21600,
             "8h": 28800, "12h": 43200, "1d": 86400}
LIMITS = {"archive_bytes": 1024 * 1024, "checksum_bytes": 4096,
          "csv_bytes": 4 * 1024 * 1024, "row_bytes": 4096, "field_bytes": 128,
          "decimal_digits": 100, "rows": 1440, "provenance_bytes": 4096,
          "evidence_bytes_each": 2 * 1024 * 1024, "manifest_bytes": 256 * 1024,
          "records_bytes": 4 * 1024 * 1024}
ADMISSION = {"coverage": "UNPROVEN", "historical_availability": "UNVERIFIED",
             "research_qualified": False, "registered_in_quazonai": False,
             "permission_status": "REQUIRES_INDEPENDENT_REVIEW"}
EVIDENCE_ROLES = ("vision_terms", "incorporated_terms", "license", "parser_spec")
MAX_NS = 2**64 - 1
EPOCH = dt.date(1970, 1, 1)
UTC = dt.timezone.utc


@dataclass(frozen=True)
class Selection:
    symbol: str
    base_asset: str
    quote_asset: str
    day: str
    interval: str

    def validate(self):
        for value in (self.symbol, self.base_asset, self.quote_asset):
            if not isinstance(value, str) or not re.fullmatch(r"[A-Z0-9]{1,32}", value):
                raise ValueError("selection requires explicit uppercase ASCII symbol/base/quote")
        if self.base_asset == self.quote_asset or self.symbol != self.base_asset + self.quote_asset:
            raise ValueError("symbol must match the explicitly declared distinct base and quote")
        if not isinstance(self.interval, str) or self.interval not in INTERVALS:
            raise ValueError("unsupported interval (one minute through one day only)")
        if not isinstance(self.day, str) or not re.fullmatch(r"[0-9]{4}-[0-9]{2}-[0-9]{2}", self.day):
            raise ValueError("day must be an ISO UTC date")
        day = dt.date.fromisoformat(self.day)
        start = (day - EPOCH).days * 86400
        if start < 0 or (start + 86400) * 1_000_000_000 > MAX_NS:
            raise ValueError("day cannot be represented as unsigned native nanoseconds")
        return start


def plan(selection):
    start = selection.validate()
    stem = f"{selection.symbol}-{selection.interval}-{selection.day}"
    archive = stem + ".zip"
    url = (f"https://data.binance.vision/data/spot/daily/klines/"
           f"{selection.symbol}/{selection.interval}/{archive}")
    return {"schema": SCHEMA, "provider": deepcopy(PROVIDER), "parser": PARSER,
            "selection": asdict(selection), "identity_status": "OPERATOR_DECLARED_UNVERIFIED",
            "start_ns": str(start * 1_000_000_000),
            "end_ns": str((start + 86400) * 1_000_000_000),
            "interval_seconds": INTERVALS[selection.interval],
            "source_timestamp_unit": "us" if selection.day >= "2025-01-01" else "ms",
            "archive_name": archive, "member_name": stem + ".csv",
            "source_references": [{"role": "CHECKSUM", "url": url + ".CHECKSUM"},
                                  {"role": "ARCHIVE", "url": url}],
            "source_reference_status": "DOCUMENTATION_DERIVED_NOT_REQUESTED",
            "artifact_scope": "OFFLINE_LOCAL_IMPORT",
            "parser_spec_reference": {"url": f"https://github.com/binance/binance-public-data/blob/{SPEC_REVISION}/README.md",
                                      "documentation_revision": SPEC_REVISION},
            "container_validation": "STDLIB_MEMBER_VALIDATION_NOT_CANONICAL_ZIP",
            "limits": dict(LIMITS), "admission": dict(ADMISSION)}


def local_bytes(path, limit):
    """Bounded regular-file read, refusing symlinks and nonblocking special files."""
    path = Path(os.path.abspath(path))
    safe_local(path.parent, path.name)
    flags = (os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0) | getattr(os, "O_NONBLOCK", 0)
             | getattr(os, "O_BINARY", 0))
    with os.fdopen(os.open(path, flags), "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_size > limit:
            raise ValueError("input must be a bounded regular file")
        body = stream.read(limit + 1)
        after = os.fstat(stream.fileno())
    safe_local(path.parent, path.name)
    current = path.stat()
    identity = lambda s: (s.st_dev, s.st_ino, s.st_size, s.st_mtime_ns, s.st_ctime_ns)
    if (len(body) > limit or len(body) != before.st_size
            or identity(before) != identity(after) or identity(after) != identity(current)):
        raise ValueError("input changed during read or exceeds byte limit")
    return body


def unsigned(value, label, maximum=MAX_NS):
    if not isinstance(value, str) or not re.fullmatch(r"(?:0|[1-9][0-9]{0,19})", value):
        raise ValueError(f"invalid {label}")
    number = int(value)
    if number > maximum:
        raise ValueError(f"{label} exceeds its bound")
    return number


def amount(value, positive=False):
    if (not re.fullmatch(r"(?:0|[1-9][0-9]*)(?:\.[0-9]+)?", value)
            or sum(c.isdigit() for c in value) > LIMITS["decimal_digits"]):
        raise ValueError("invalid decimal lexeme")
    result = Decimal(value)  # Construction and comparison do not round to context precision.
    if positive and result == 0:
        raise ValueError("OHLC must be positive")
    return result


def decode_csv(body, selection):
    spec = plan(selection)
    if len(body) > LIMITS["csv_bytes"]:
        raise ValueError("CSV exceeds byte limit")
    try:
        text = body.decode("ascii")
    except UnicodeDecodeError:
        raise ValueError("CSV must be ASCII without BOM") from None
    if any(ord(c) < 32 and c not in "\r\n" for c in text) or "\x7f" in text or '"' in text:
        raise ValueError("CSV controls and quoted fields are unsupported")
    if "\r" in text.replace("\r\n", ""):
        raise ValueError("CSV requires LF or CRLF line endings")
    # No quoted fields means physical lines are records, with one optional final terminator.
    lines = text.split("\n")
    if lines[-1] == "":
        lines.pop()
    expected = 86400 // spec["interval_seconds"]
    if len(lines) > expected or len(lines) > LIMITS["rows"]:
        raise ValueError("CSV row count exceeds selection")
    if any(not line.removesuffix("\r") or len(line) > LIMITS["row_bytes"] for line in lines):
        raise ValueError("CSV has a blank or oversized row")
    scale = 1000 if spec["source_timestamp_unit"] == "us" else 1_000_000
    width = spec["interval_seconds"] * 1_000_000_000
    start, end = int(spec["start_ns"]), int(spec["end_ns"])
    rows, present = [], set()
    for index, fields in enumerate(csv.reader(io.StringIO(text, newline=""), strict=True)):
        if len(fields) != 12 or any(len(field) > LIMITS["field_bytes"] for field in fields):
            raise ValueError("CSV requires twelve bounded fields")
        opened = unsigned(fields[0], "source open", MAX_NS // scale) * scale
        closed = unsigned(fields[6], "source close", MAX_NS // scale) * scale
        if (not start <= opened < end or (opened - start) % width
                or closed != opened + width - scale
                or (rows and opened <= int(rows[-1]["bucket_open_ns"]))):
            raise ValueError("CSV clock, ordering, duplicate or selection mismatch")
        open_, high, low, close = [amount(fields[i], positive=True) for i in range(1, 5)]
        base, quote, taker_base, taker_quote = [amount(fields[i]) for i in (5, 7, 9, 10)]
        if not low <= open_ <= high or not low <= close <= high:
            raise ValueError("invalid OHLC ordering")
        if taker_base > base or taker_quote > quote:
            raise ValueError("taker volume exceeds total volume")
        unsigned(fields[8], "trade count")
        if not fields[11] or any(ord(c) < 32 or ord(c) > 126 for c in fields[11]):
            raise ValueError("ignored source field must be bounded printable ASCII")
        rows.append({"kind": "OHLCV_CANDLE", "symbol": selection.symbol,
                     "bucket_open_ns": str(opened), "event_end_ns": str(opened + width),
                     "source_open": fields[0], "source_close_inclusive": fields[6],
                     "source_close_inclusive_ns": str(closed),
                     "source_timestamp_unit": spec["source_timestamp_unit"],
                     **dict(zip(("open", "high", "low", "close", "base_volume"), fields[1:6])),
                     "quote_volume": fields[7], "trade_count": fields[8],
                     "taker_base_volume": fields[9], "taker_quote_volume": fields[10],
                     "source_ignored": fields[11], "source_archive": "raw/archive.zip",
                     "source_member": spec["member_name"], "source_row_index": str(index),
                     "observed_at": None, "historical_available_at": None})
        present.add(opened)
    missing = [str(t) for t in range(start, end, width) if t not in present]
    return rows, {"rows": str(len(rows)), "possible_buckets": str(expected),
                  "missing_buckets": str(len(missing)), "missing_bucket_open_ns": missing}


def decode(archive, checksum, selection):
    """Validate bounded original bytes and reproduce source rows without extraction."""
    spec = plan(selection)
    if len(archive) > LIMITS["archive_bytes"] or len(checksum) > LIMITS["checksum_bytes"]:
        raise ValueError("archive or checksum exceeds byte limit")
    expected = re.escape(spec["archive_name"].encode("ascii"))
    match = re.fullmatch(rb"([0-9a-fA-F]{64}) [ *]" + expected + rb"(?:\r?\n)?", checksum)
    if not match or hashlib.sha256(archive).hexdigest() != match[1].decode().lower():
        raise ValueError("checksum requires one matching SHA-256 entry and exact archive basename")
    try:
        with zipfile.ZipFile(io.BytesIO(archive)) as container:
            members = container.infolist()
            if len(members) != 1:
                raise ValueError("archive requires exactly one member (no extras or duplicates)")
            member = members[0]
            mode = member.external_attr >> 16
            if (member.filename != spec["member_name"]
                    or member.is_dir() or member.external_attr & 0x10
                    or stat.S_IFMT(mode) not in (0, stat.S_IFREG)
                    or member.flag_bits & (1 | 0x40)
                    or member.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                    or member.header_offset != 0
                    or member.file_size > LIMITS["csv_bytes"]
                    or member.compress_size > LIMITS["archive_bytes"]):
                raise ValueError("unsafe or unsupported ZIP member, prefix, compression or size")
            # ZipInfo.filename can normalize a NUL suffix. Ask zipfile to check
            # the local name against a canonical, publicly constructed ZipInfo;
            # then use the original member for its normal structural/CRC checks.
            canonical = zipfile.ZipInfo(spec["member_name"])
            for attribute in ("header_offset", "compress_type", "compress_size", "file_size", "CRC", "flag_bits"):
                setattr(canonical, attribute, getattr(member, attribute))
            with container.open(canonical):
                pass
            with container.open(member) as stream:
                body = stream.read(LIMITS["csv_bytes"] + 1)
                if len(body) > LIMITS["csv_bytes"] or stream.read(1) or len(body) != member.file_size:
                    raise ValueError("decoded ZIP member exceeds limit or declared size")
            # Reading to EOF invokes zipfile's CRC/local-header/overlap checks.
            if zlib.crc32(body) & 0xFFFFFFFF != member.CRC:
                raise ValueError("ZIP CRC mismatch")
    except (zipfile.BadZipFile, RuntimeError, NotImplementedError, EOFError, zlib.error) as error:
        raise ValueError("invalid or unsupported ZIP archive") from error
    rows, counts = decode_csv(body, selection)
    return {"rows": rows, "counts": counts,
            "member": {"name": spec["member_name"], "size": len(body),
                       "sha256": hashlib.sha256(body).hexdigest()},
            "checksum_sha256": match[1].decode().lower()}


def utc_ns(value):
    if (not isinstance(value, str) or not re.fullmatch(
            r"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]{1,9})?Z", value)):
        raise ValueError("provenance clocks require UTC Z with at most nanosecond precision")
    clock = dt.datetime.strptime(value[:19], "%Y-%m-%dT%H:%M:%S").replace(tzinfo=UTC)
    seconds = (clock.date() - EPOCH).days * 86400 + clock.hour * 3600 + clock.minute * 60 + clock.second
    fraction = value[20:-1] if value[19] == "." else ""
    result = seconds * 1_000_000_000 + int(fraction.ljust(9, "0") or "0")
    if not 0 <= result <= MAX_NS:
        raise ValueError("provenance clock out of range")
    return result


def check_provenance(body, selection, imported_at):
    value = read_json(body) if body is not None else {"kind": "UNKNOWN", "retrieval": None}
    if not isinstance(value, dict) or set(value) != {"kind", "retrieval"}:
        raise ValueError("provenance requires kind and retrieval")
    if value["kind"] not in ("UNKNOWN", "OPERATOR_DECLARED", "SYNTHETIC"):
        raise ValueError("unsupported provenance kind; attested retrieval is not implemented")
    imported_ns = utc_ns(imported_at)
    end = int(plan(selection)["end_ns"])
    if imported_ns < end:
        raise ValueError("offline import must follow completion of the selected UTC day")
    retrieval = value["retrieval"]
    if value["kind"] == "UNKNOWN" and retrieval is not None:
        raise ValueError("unknown provenance cannot claim retrieval clocks")
    if retrieval is not None:
        if not isinstance(retrieval, dict) or set(retrieval) != {"checksum", "archive"}:
            raise ValueError("retrieval requires checksum and archive clocks")
        previous = end
        for role in ("checksum", "archive"):
            entry = retrieval[role]
            if not isinstance(entry, dict) or set(entry) != {"started_at", "completed_at"}:
                raise ValueError("each declared retrieval requires original start/completion clocks")
            start, finish = utc_ns(entry["started_at"]), utc_ns(entry["completed_at"])
            if not previous <= start <= finish <= imported_ns:
                raise ValueError("declared retrieval clocks violate day/order/import bounds")
            previous = finish
    return value


def _bundle(selection, archive, checksum, provenance, evidence, imported_at, published_at, implementation):
    spec = plan(selection)
    declaration = check_provenance(provenance, selection, imported_at)
    if not utc_ns(imported_at) <= utc_ns(published_at) <= utc_ns(now()):
        raise ValueError("publication must follow offline import and cannot be in the future")
    if not isinstance(implementation, str) or not re.fullmatch(r"[0-9a-f]{64}", implementation):
        raise ValueError("invalid recorded importer implementation digest")
    decoded = decode(archive, checksum, selection)
    for row in decoded["rows"]:
        row["provenance_kind"] = declaration["kind"]
        row["declared_observed_at"] = (declaration["retrieval"]["archive"]["completed_at"]
                                       if declaration["retrieval"] else None)
    records = b"".join(json_bytes(row) for row in decoded["rows"])
    if len(records) > LIMITS["records_bytes"]:
        raise ValueError("normalized records exceed byte limit")
    blobs = {"raw/archive.zip": archive, "raw/archive.CHECKSUM": checksum,
             "records.jsonl": records}
    if provenance is not None:
        blobs["provenance.json"] = provenance
    evidence_records = {}
    if not isinstance(evidence, dict) or set(evidence) - set(EVIDENCE_ROLES):
        raise ValueError("unknown evidence role")
    for role in EVIDENCE_ROLES:
        body = evidence.get(role)
        if body is not None:
            if len(body) > LIMITS["evidence_bytes_each"]:
                raise ValueError("evidence exceeds byte limit")
            blobs[f"evidence/{role}.bin"] = body
        evidence_records[role] = {"status": "OPERATOR_SUPPLIED_UNVERIFIED" if body is not None else "NOT_SUPPLIED",
                                  "source_url": None, "revision": None}
    manifest = {**spec, "files": {path: file_record(path, body) for path, body in blobs.items()},
                "decoded_member": decoded["member"], "checksum_sha256": decoded["checksum_sha256"],
                "archive_identity": "sha256:" + hashlib.sha256(archive).hexdigest(),
                "upstream_revision": None, "implementation_sha256": implementation,
                "implementation_revision": None, "implementation_status": "RECORDED_LOCAL_HASH_NOT_ATTESTED",
                "provenance": declaration,
                "provenance_kind": declaration["kind"],
                "provenance_status": {"UNKNOWN": "UNKNOWN", "SYNTHETIC": "SYNTHETIC",
                                      "OPERATOR_DECLARED": "DECLARED_UNVERIFIED"}[declaration["kind"]],
                "evidence": evidence_records, "imported_at": imported_at, "published_at": published_at,
                "counts": decoded["counts"],
                "status": "OBSERVATIONS" if decoded["rows"] else "NO_OBSERVATIONS"}
    if len(json_bytes(manifest)) > LIMITS["manifest_bytes"]:
        raise ValueError("manifest exceeds byte limit")
    return manifest, blobs


def now():
    return dt.datetime.now(UTC).isoformat().replace("+00:00", "Z")


def freeze(selection, archive_path, checksum_path, output, *, provenance_path=None, evidence_paths=None):
    """Freeze local original files into a new bundle; no inferred retrieval time."""
    plan(selection)
    imported_at = now()
    inputs = [(archive_path, LIMITS["archive_bytes"]), (checksum_path, LIMITS["checksum_bytes"])]
    if provenance_path is not None:
        inputs.append((provenance_path, LIMITS["provenance_bytes"]))
    evidence_paths = evidence_paths or {}
    if set(evidence_paths) - set(EVIDENCE_ROLES):
        raise ValueError("unknown evidence role")
    inputs.extend((path, LIMITS["evidence_bytes_each"]) for path in evidence_paths.values())
    originals = [(path, limit, local_bytes(path, limit)) for path, limit in inputs]
    archive, checksum = originals[0][2], originals[1][2]
    provenance = originals[2][2] if provenance_path is not None else None
    evidence = dict(zip(evidence_paths, [item[2] for item in originals[3 if provenance_path is not None else 2:]]))
    implementation = hashlib.sha256(local_bytes(__file__, 1024 * 1024)).hexdigest()
    manifest, blobs = _bundle(selection, archive, checksum, provenance, evidence,
                              imported_at, now(), implementation)
    root = Path(os.path.abspath(output))
    safe_local(root.parent, root.name)
    root.mkdir()  # Never resume, replace or clean up an existing user directory.
    for name, body in blobs.items():
        path = safe_local(root, name)
        path.parent.mkdir(exist_ok=True)
        publish_bytes(safe_local(root, name), body)
    for path, limit, body in originals:
        if local_bytes(path, limit) != body:
            raise ValueError("original input changed before publication")
    for name, body in blobs.items():
        if local_bytes(safe_local(root, name), len(body)) != body:
            raise ValueError("frozen output changed before publication")
    manifest["published_at"] = now()
    if utc_ns(manifest["published_at"]) < utc_ns(imported_at):
        raise ValueError("publication clock precedes import")
    publish_bytes(safe_local(root, "archive.json"), json_bytes(manifest))
    return manifest


def verify(output):
    """Reparse original bytes and compare the full v1 envelope and normalized rows."""
    root = Path(os.path.abspath(output))
    raw_manifest = local_bytes(safe_local(root, "archive.json"), LIMITS["manifest_bytes"])
    manifest = read_json(raw_manifest)
    try:
        selection = Selection(**manifest["selection"])
        expected_plan = plan(selection)
        for key, value in expected_plan.items():
            if json_bytes(manifest[key]) != json_bytes(value):
                raise ValueError("unsupported schema, provider, parser or selection plan")
        files = manifest["files"]
        expected_names = {"raw/archive.zip", "raw/archive.CHECKSUM", "records.jsonl"}
        optional = {"provenance.json", *(f"evidence/{role}.bin" for role in EVIDENCE_ROLES)}
        if not isinstance(files, dict) or not expected_names <= set(files) <= expected_names | optional:
            raise ValueError("unexpected or missing bundle file identity")
        limits = {"raw/archive.zip": LIMITS["archive_bytes"], "raw/archive.CHECKSUM": LIMITS["checksum_bytes"],
                  "records.jsonl": LIMITS["records_bytes"], "provenance.json": LIMITS["provenance_bytes"],
                  **{f"evidence/{role}.bin": LIMITS["evidence_bytes_each"] for role in EVIDENCE_ROLES}}
        blobs = {name: local_bytes(safe_local(root, name), limits[name]) for name in files}
        evidence = {role: blobs[f"evidence/{role}.bin"] for role in EVIDENCE_ROLES if f"evidence/{role}.bin" in blobs}
        rebuilt, expected_blobs = _bundle(selection, blobs["raw/archive.zip"], blobs["raw/archive.CHECKSUM"],
                                          blobs.get("provenance.json"), evidence,
                                          manifest["imported_at"], manifest["published_at"],
                                          manifest["implementation_sha256"])
        if json_bytes(rebuilt) != json_bytes(manifest) or blobs != expected_blobs:
            raise ValueError("archive envelope or normalized records do not reproduce")
        for name, body in blobs.items():
            if local_bytes(safe_local(root, name), limits[name]) != body:
                raise ValueError("bundle changed during verification")
        if local_bytes(safe_local(root, "archive.json"), LIMITS["manifest_bytes"]) != raw_manifest:
            raise ValueError("manifest changed during verification")
    except (KeyError, TypeError, csv.Error) as error:
        raise ValueError("malformed archive envelope") from error
    return {"integrity": "VERIFIED", "schema": SCHEMA, "provider": deepcopy(PROVIDER),
            "integrity_scope": "RETAINED_BYTES_AND_DECODED_MEMBER_ONLY",
            "status": manifest["status"], "counts": manifest["counts"],
            "provenance_status": manifest["provenance_status"], "admission": dict(ADMISSION)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    commands = parser.add_subparsers(dest="command", required=True)
    for command in ("plan", "inspect", "freeze", "verify"):
        child = commands.add_parser(command, allow_abbrev=False)
        if command != "verify":
            for name in ("symbol", "base-asset", "quote-asset", "day", "interval"):
                child.add_argument("--" + name, required=True)
        if command in ("inspect", "freeze"):
            child.add_argument("--archive", type=Path, required=True)
            child.add_argument("--checksum", type=Path, required=True)
        if command in ("freeze", "verify"):
            child.add_argument("--output", type=Path, required=True)
        if command == "freeze":
            child.add_argument("--provenance", type=Path)
            for role in EVIDENCE_ROLES:
                child.add_argument("--" + role.replace("_", "-") + "-file", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "verify":
            result = verify(args.output)
        else:
            selection = Selection(args.symbol, args.base_asset, args.quote_asset, args.day, args.interval)
            if args.command == "plan":
                result = plan(selection)
            elif args.command == "inspect":
                result = decode(local_bytes(args.archive, LIMITS["archive_bytes"]),
                                local_bytes(args.checksum, LIMITS["checksum_bytes"]), selection)
                del result["rows"]  # CLI summary does not print market observations.
                result["admission"] = dict(ADMISSION)
            else:
                result = freeze(selection, args.archive, args.checksum, args.output,
                                provenance_path=args.provenance,
                                evidence_paths={role: getattr(args, role + "_file") for role in EVIDENCE_ROLES
                                                if getattr(args, role + "_file") is not None})
        print(json_bytes(result).decode(), end="")
    except (ValueError, OSError, csv.Error) as error:
        print(f"binance-vision: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
