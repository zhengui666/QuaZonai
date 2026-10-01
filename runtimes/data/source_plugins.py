#!/usr/bin/env python3
"""Operator-only free-source lifecycle dispatch; native preparation is not research admission."""

import argparse
import csv
from dataclasses import dataclass, replace
from functools import partial
import hashlib
import http.client
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Callable
import urllib.error
import urllib.parse

import acquire
import binance_vision
import providers
import snapshot


MAX_REPORT_BYTES = 1024 * 1024
MAX_EVIDENCE_BYTES = 128 * 1024 * 1024
ARCHIVE_FORMATS = ("moose-fills", "time-seventeen-v2", "joseph-books")


@dataclass(frozen=True)
class Capability:
    configure: Callable
    run: Callable
    public_network: bool = False


@dataclass(frozen=True)
class SourcePlugin:
    descriptor: dict
    capabilities: dict[str, Capability]
    validate_native: Callable | None = None


def local_path(value):
    path = Path(os.path.abspath(value))
    snapshot.safe_local(path.parent, path.name)
    return path


def load_json(path, limit=MAX_REPORT_BYTES):
    value = providers.read_json(acquire.local_bytes(path, limit))
    if not isinstance(value, dict):
        raise ValueError("expected a JSON object")
    return value


def sha256(path):
    return snapshot.file_hash(local_path(path))


def source_selection(args):
    return providers.Selection(args.instrument, args.start_seconds,
                               args.end_seconds, args.interval_seconds)


def http_options(parser, download=False):
    parser.add_argument("--instrument", required=True)
    window_options(parser)
    parser.add_argument("--interval-seconds", type=int, required=True)
    parser.add_argument("--max-bytes", type=int, default=acquire.DEFAULT_MAX_BYTES)
    if download:
        parser.add_argument("--output", type=Path, required=True)
        parser.add_argument("--terms-file", type=Path, required=True)


def http_plan(provider_id, args):
    return acquire.plan(provider_id, source_selection(args), args.max_bytes)


def http_download(provider_id, args):
    return acquire.acquire(provider_id, source_selection(args), args.output,
                           args.terms_file, args.max_bytes)


def http_verify_options(parser):
    parser.add_argument("--acquisition", type=Path, required=True, help="original acquisition.json")


def http_verify(provider_id, args):
    path = local_path(args.acquisition)
    if path.name != "acquisition.json":
        raise ValueError("expected the original acquisition.json")
    result = acquire.verify(path.parent)
    if result["provider"] != provider_id:
        raise ValueError("acquisition does not belong to this source plugin")
    return result


def archive_options(parser, operation):
    for name in ("symbol", "base-asset", "quote-asset", "day", "interval"):
        parser.add_argument("--" + name, required=True)
    if operation in ("inspect", "freeze"):
        parser.add_argument("--archive", type=Path, required=True)
        parser.add_argument("--checksum", type=Path, required=True)
    if operation == "freeze":
        parser.add_argument("--output", type=Path, required=True)
        parser.add_argument("--provenance", type=Path)
        for role in binance_vision.EVIDENCE_ROLES:
            parser.add_argument("--" + role.replace("_", "-") + "-file", type=Path)


def archive_selection(args):
    return binance_vision.Selection(args.symbol, args.base_asset, args.quote_asset, args.day, args.interval)


def archive_operation(operation, args):
    selection = archive_selection(args)
    if operation == "plan":
        return binance_vision.plan(selection)
    if operation == "inspect":
        result = binance_vision.decode(
            binance_vision.local_bytes(args.archive, binance_vision.LIMITS["archive_bytes"]),
            binance_vision.local_bytes(args.checksum, binance_vision.LIMITS["checksum_bytes"]), selection)
        del result["rows"]  # Keep original observations out of the command summary.
        return {**result, "admission": dict(binance_vision.ADMISSION)}
    return binance_vision.freeze(selection, args.archive, args.checksum, args.output,
        provenance_path=args.provenance,
        evidence_paths={role: getattr(args, role + "_file") for role in binance_vision.EVIDENCE_ROLES
                        if getattr(args, role + "_file") is not None})


def archive_verify_options(parser):
    parser.add_argument("--acquisition", type=Path, required=True, help="original archive.json")


def archive_verify(args):
    path = local_path(args.acquisition)
    if path.name != "archive.json":
        raise ValueError("expected the original archive.json")
    return binance_vision.verify(path.parent)


def byte_budget_options(parser):
    parser.add_argument("--max-bytes", type=int, default=snapshot.DEFAULT_MAX_BYTES,
                        help="explicit total snapshot file byte budget (default 128 MiB)")


def snapshot_options(parser, download=False):
    parser.add_argument("--dataset", required=True, help="public, ungated Hugging Face dataset ID")
    parser.add_argument("--include", action="append", required=True, help="explicit file glob; repeatable")
    parser.add_argument("--revision", help="resolved to an immutable commit by snapshot.py")
    parser.add_argument("--license", help="verified source terms if the dataset card omits a license")
    byte_budget_options(parser)
    if download:
        parser.add_argument("--output", type=Path, required=True)


def snapshot_plan(args):
    return snapshot.plan(args.dataset, args.include, args.revision, args.max_bytes, args.license)


def snapshot_download(args):
    return snapshot.download(snapshot_plan(args), args.output)


def snapshot_verify_options(parser):
    parser.add_argument("--snapshot", type=Path, required=True, help="original snapshot.json")
    byte_budget_options(parser)


def verified_snapshot(path, max_bytes):
    """Offline file integrity only; format interpretation stays in the native adapter."""
    providers.integer(max_bytes, "max_bytes", minimum=1, maximum=2**63 - 1)
    path = local_path(path)
    manifest = load_json(path, 32 * snapshot.CHUNK)
    expected = {"schema_version", "repository", "revision", "license", "license_reference",
                "retrieved_at", "files"}
    if (set(manifest) != expected or type(manifest["schema_version"]) is not int
            or manifest["schema_version"] != 1
            or not isinstance(manifest["repository"], str)
            or not re.fullmatch(r"[\w.-]+(?:/[\w.-]+)?", manifest["repository"], re.ASCII)
            or not isinstance(manifest["revision"], str)
            or not re.fullmatch(r"[0-9a-f]{40}", manifest["revision"])
            or any(not isinstance(manifest[key], str) or not manifest[key].strip()
                   for key in ("license", "license_reference"))):
        raise ValueError("invalid immutable public snapshot identity")
    snapshot.checked_path(manifest["repository"])
    if acquire.utc_clock(manifest["retrieved_at"]) > acquire.utc_clock(acquire.now()):
        raise ValueError("snapshot observation is in the future")
    files = manifest["files"]
    if not isinstance(files, list) or not 1 <= len(files) <= 100_000:
        raise ValueError("invalid snapshot file count")
    seen, total = set(), 0
    for item in files:
        if not isinstance(item, dict) or set(item) != {"path", "size", "url", "sha256"}:
            raise ValueError("invalid snapshot file record")
        relative = snapshot.checked_path(item["path"])
        if relative in seen or relative.split("/")[0] == "snapshot.json":
            raise ValueError("duplicate or conflicting snapshot path")
        seen.add(relative)
        providers.integer(item["size"], "snapshot file size", maximum=max_bytes)
        total += item["size"]
        if total > max_bytes:
            raise ValueError("snapshot exceeds the explicit byte budget")
        expected_url = (f"{snapshot.HUB}/datasets/{manifest['repository']}/resolve/"
                        f"{manifest['revision']}/{urllib.parse.quote(relative)}")
        if (item["url"] != expected_url or not isinstance(item["sha256"], str)
                or not re.fullmatch(r"[0-9a-f]{64}", item["sha256"])):
            raise ValueError("snapshot file identity does not match its fixed public revision")
        target = snapshot.safe_local(path.parent, relative)
        if (not target.is_file() or target.stat().st_size != item["size"]
                or snapshot.file_hash(target) != item["sha256"]):
            raise ValueError("snapshot file size or checksum mismatch")
    return manifest


def snapshot_verify(args):
    manifest = verified_snapshot(args.snapshot, args.max_bytes)
    return {"integrity": "VERIFIED", "repository": manifest["repository"],
            "revision": manifest["revision"], "files": len(manifest["files"]),
            "admission": dict(acquire.ADMISSION)}


def window_options(parser):
    parser.add_argument("--start-seconds", type=int, required=True)
    parser.add_argument("--end-seconds", type=int, required=True)


def native_options(parser):
    parser.add_argument("--native-bin", type=Path,
                        help="explicit native executable; otherwise use the matching packaged bin/ executable")
    parser.add_argument("--output", type=Path, required=True, help="new native output directory")


def candle_options(parser, acquisition_options=http_verify_options):
    native_options(parser)
    acquisition_options(parser)
    parser.add_argument("--instruments", type=Path, required=True,
                        help="original native InstrumentAny JSON array, with original clocks")


def history_options(parser, capture=False):
    native_options(parser)
    snapshot_verify_options(parser)
    window_options(parser)
    if capture:
        parser.add_argument("--market-slug", required=True)
        parser.add_argument("--bar-seconds", type=int, default=1)
    else:
        parser.add_argument("--format", choices=ARCHIVE_FORMATS, required=True)
        parser.add_argument("--instruments", type=Path, required=True)
        parser.add_argument("--bar-seconds", type=int)
        parser.add_argument("--chain-evidence", type=Path,
                            help="existing original evm.py evidence; native v2 adapter verifies it")


def run_native(args, argv, binary_name="catalog-prepare"):
    if os.environ.get("QZ_OPERATOR_INSTALLED") == "1" and args.native_bin is not None:
        raise ValueError("installed source operations require their matching packaged native executable")
    output = local_path(args.output)
    if output.exists():
        raise ValueError("native output must be a new directory; failed artifacts are never removed")
    binary = local_path(args.native_bin or Path(__file__).parent / "bin" / binary_name)
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise ValueError("native-bin must be an existing executable file")
    # Diagnostics and original native stdout are retained on stderr, leaving one JSON result on stdout.
    result = subprocess.run([str(binary), *argv, "--output", str(output)],
                            stdin=subprocess.DEVNULL, stdout=sys.stderr, stderr=sys.stderr,
                            shell=False, check=False, timeout=3600)
    if result.returncode != 0:
        raise ValueError(f"native converter exited {result.returncode}; original diagnostics and artifacts retained")
    return output


def published_catalog(output):
    catalog = snapshot.safe_local(output, "catalog")
    if not catalog.is_dir():
        raise ValueError("published native catalog is missing")
    parquet_count = 0
    for root, directories, files in os.walk(catalog, followlinks=False):
        for name in directories + files:
            path = snapshot.safe_local(catalog, (Path(root) / name).relative_to(catalog).as_posix())
            if name in files and path.suffix == ".parquet":
                if not path.is_file() or path.stat().st_size < 12:
                    raise ValueError("incomplete native Parquet file")
                with path.open("rb") as stream:
                    if stream.read(4) != b"PAR1":
                        raise ValueError("invalid native Parquet header")
                    stream.seek(-4, os.SEEK_END)
                    if stream.read(4) != b"PAR1":
                        raise ValueError("incomplete native Parquet footer")
                parquet_count += 1
    if not parquet_count:
        raise ValueError("published native catalog contains no Parquet files")
    return catalog


def published_native(output):
    report = load_json(snapshot.safe_local(output, "import-report.json"))
    if (type(report.get("schema_version")) is not int or report["schema_version"] != 1
            or report.get("native_version") != "0.63.0"
            or report.get("catalog_relative_path") != "catalog"
            or report.get("coverage") != "UNPROVEN"
            or report.get("historical_availability") != "UNVERIFIED"
            or report.get("registered_in_quazonai") is not False
            or ("research_qualified" in report and report["research_qualified"] is not False)
            or not isinstance(report.get("limitations"), list) or not report["limitations"]
            or any(not isinstance(item, str) or not item.strip() for item in report["limitations"])):
        raise ValueError("invalid native publication report or admission boundary")
    for key in ("instruments", "instrument_versions"):
        providers.integer(report.get(key), f"native report {key}", minimum=1, maximum=1_000_000)
    if report["instruments"] > report["instrument_versions"]:
        raise ValueError("invalid native instrument counts")
    published_catalog(output)
    evidence = load_json(snapshot.safe_local(output, "source-evidence.json"), MAX_EVIDENCE_BYTES)
    return report, evidence


def converted(plugin_id, output, report):
    return {"schema": "qz.source_preparation/1", "plugin": plugin_id,
            "status": "NATIVE_ARTIFACTS_VALIDATED", "output": str(output),
            "native_report": report, "admission": dict(acquire.ADMISSION)}


def convert_candles(plugin_id, args, *, verify, native_mode, record_count):
    verified = verify(args)
    acquisition = local_path(args.acquisition)
    instruments = local_path(args.instruments)
    # The native parser owns original definition semantics and exact decimal conversion.
    original_definitions = providers.read_json(acquire.local_bytes(instruments, MAX_REPORT_BYTES))
    original_acquisition = load_json(acquisition)
    hashes = {"acquisition_sha256": sha256(acquisition),
              "instrument_definitions_sha256": sha256(instruments)}
    output = run_native(args, [native_mode, "--acquisition", str(acquisition),
                               "--instruments", str(instruments)])
    if (verify(args) != verified
            or sha256(acquisition) != hashes["acquisition_sha256"]
            or sha256(instruments) != hashes["instrument_definitions_sha256"]):
        raise ValueError("native conversion inputs changed during preparation")
    report, evidence = published_native(output)
    if (report.get("source_provider") != plugin_id
            or report.get("source_record_kind") != "OHLCV_CANDLE"
            or report.get("source_evidence_relative_path") != "source-evidence.json"
            or report.get("native_readback_verified") is not True
            or report.get("research_qualified") is not False
            or any(report.get(key) != value for key, value in hashes.items())
            or any(evidence.get(key) != value for key, value in hashes.items())
            or evidence.get("source_acquisition_path") != str(acquisition)
            or evidence.get("acquisition") != original_acquisition
            or evidence.get("instrument_definitions") != original_definitions
            or not isinstance(original_definitions, list)
            or report["instrument_versions"] != len(original_definitions)):
        raise ValueError("native candle report does not match its verified source inputs")
    providers.integer(report.get("bars"), "native bars", minimum=1, maximum=providers.MAX_RECORDS)
    if report["bars"] != record_count(verified):
        raise ValueError("native candle count differs from verified acquisition")
    validate_native(plugin_id, report, evidence)
    return converted(plugin_id, output, report)


def candle_convert(args):
    return convert_candles("coinbase-candles", args, verify=partial(http_verify, "coinbase-candles"),
                           native_mode="ingest-candles", record_count=lambda value: value["record_count"])


def archive_convert(args):
    archive_receipt(load_json(local_path(args.acquisition)))
    return convert_candles(binance_vision.PROVIDER["id"], args, verify=archive_verify,
        native_mode="ingest-archive-candles",
        record_count=lambda value: binance_vision.unsigned(value["counts"]["rows"], "rows",
                                                          binance_vision.LIMITS["rows"]))


def history_convert(args, capture=False):
    original = verified_snapshot(args.snapshot, args.max_bytes)
    inputs = [local_path(args.snapshot)]
    providers.integer(args.start_seconds, "start_seconds")
    providers.integer(args.end_seconds, "end_seconds")
    if args.start_seconds >= args.end_seconds:
        raise ValueError("selection requires start_seconds < end_seconds")
    if args.bar_seconds is not None:
        providers.integer(args.bar_seconds, "bar_seconds", minimum=1, maximum=86400)
    argv = ["capture" if capture else "archive", "--snapshot", str(inputs[0]),
            "--start-seconds", str(args.start_seconds), "--end-seconds", str(args.end_seconds)]
    if args.bar_seconds is not None:
        argv += ["--bar-seconds", str(args.bar_seconds)]
    if capture:
        if (not isinstance(args.market_slug, str)
                or not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,255}", args.market_slug)):
            raise ValueError("invalid explicit market slug")
        argv += ["--market-slug", args.market_slug]
        native_format = "lokima-dual-capture"
    else:
        if args.format not in ARCHIVE_FORMATS:
            raise ValueError("unsupported native archive format")
        inputs.append(local_path(args.instruments))
        acquire.local_bytes(inputs[-1], MAX_EVIDENCE_BYTES)
        argv += ["--format", args.format, "--instruments", str(inputs[-1])]
        native_format = args.format
        if args.chain_evidence is not None:
            inputs.append(local_path(args.chain_evidence))
            acquire.local_bytes(inputs[-1], MAX_EVIDENCE_BYTES)
            argv += ["--chain-evidence", str(inputs[-1])]
    hashes = [sha256(path) for path in inputs]
    output = run_native(args, argv, "polymarket-history")
    if ([sha256(path) for path in inputs] != hashes
            or verified_snapshot(args.snapshot, args.max_bytes) != original):
        raise ValueError("native conversion inputs changed during preparation")
    report, evidence = published_native(output)
    reference = f"{snapshot.HUB}/datasets/{original['repository']}/tree/{original['revision']}"
    metadata = evidence.get("source_metadata")
    selection = {"start_seconds": args.start_seconds, "end_seconds": args.end_seconds,
                 "bar_seconds": args.bar_seconds}
    if (report.get("source_reference") != reference or evidence.get("source_reference") != reference
            or not isinstance(metadata, dict) or metadata.get("snapshot") != original
            or metadata.get("format") != native_format or metadata.get("selection") != selection
            or evidence.get("source_observed_at") != report.get("source_observed_at")):
        raise ValueError("native history report does not match its original snapshot and selection")
    if capture and (not isinstance(metadata.get("market"), dict)
                    or metadata["market"].get("slug") != args.market_slug):
        raise ValueError("native capture evidence does not match its selected market")
    total = 0
    for key in ("trades", "quotes", "deltas", "bars", "closes"):
        providers.integer(report.get(key), f"native {key}", maximum=1_000_000)
        if not isinstance(evidence.get(key), list) or len(evidence[key]) != report[key]:
            raise ValueError("native history counts differ from preserved evidence")
        total += report[key]
    if (not total or not isinstance(evidence.get("instruments"), list)
            or len(evidence["instruments"]) != report["instrument_versions"]):
        raise ValueError("native history is empty or has inconsistent definitions")
    return converted("polymarket-capture" if capture else "polymarket-archive", output, report)


def prepare_options(parser):
    native_options(parser)
    parser.add_argument("--native-output", type=Path, required=True,
                        help="existing published native output with a nonempty BAR catalog")
    parser.add_argument("--declaration", type=Path, required=True,
                        help="explicit original catalog declaration; native measurement supplies quality/row_count")
    parser.add_argument("--selection", type=Path, required=True,
                        help="explicit original NativeDatasetSelectionV1; no inferred event or receipt bounds")


def file_record(path, limit):
    body = acquire.local_bytes(path, limit)
    return {"path": str(path), "bytes": len(body), "sha256": hashlib.sha256(body).hexdigest()}


def catalog_identity(value):
    # Only the two handoff hints are read here. Native code owns the complete metadata contract.
    if (not isinstance(value, dict) or type(value.get("schema_version")) is not int
            or value["schema_version"] != 1
            or any(not isinstance(value.get(key), str) or not value[key].strip()
                   or len(value[key]) > limit
                   for key, limit in (("registered_ref", 512), ("storage_version", 120)))):
        raise ValueError("catalog preparation requires explicit valid identity hints")
    return {"native_catalog_ref": value["registered_ref"],
            "native_storage_version": value["storage_version"]}


def candle_publication(plugin_id, report, evidence, *, declaration=None):
    if (report.get("source_provider") != plugin_id
            or report.get("source_record_kind") != "OHLCV_CANDLE"
            or report.get("source_evidence_relative_path") != "source-evidence.json"
            or report.get("native_readback_verified") is not True
            or report.get("research_qualified") is not False):
        raise ValueError("native publication does not belong to the candle prepare capability")


def archive_receipt(acquisition):
    try:
        provenance = acquisition["provenance"]
        kind = provenance["kind"]
        if kind not in ("OPERATOR_DECLARED", "SYNTHETIC") or provenance["retrieval"] is None:
            raise ValueError("native archive conversion requires explicit original receipt provenance")
        clock = provenance["retrieval"]["archive"]["completed_at"]
        timestamp = binance_vision.utc_ns(clock)
        if timestamp > 2**63 - 1:
            raise ValueError("declared receipt exceeds the native timestamp range")
        return {"kind": "OPERATOR_DECLARED_UNVERIFIED" if kind == "OPERATOR_DECLARED" else "SYNTHETIC",
                "source_clock": "provenance.retrieval.archive.completed_at",
                "declared_observed_at": clock, "ts_init_ns": str(timestamp)}
    except (KeyError, TypeError) as error:
        raise ValueError("native archive receipt provenance is missing or malformed") from error


def archive_publication(report, evidence, *, declaration=None):
    candle_publication(binance_vision.PROVIDER["id"], report, evidence)
    acquisition = evidence.get("acquisition")
    if (not isinstance(acquisition, dict) or acquisition.get("schema") != binance_vision.SCHEMA
            or acquisition.get("provider") != binance_vision.PROVIDER
            or report.get("source_schema") != binance_vision.SCHEMA):
        raise ValueError("native archive report does not match its original source format")
    receipt = archive_receipt(acquisition)
    profile = "CLASSIC_SINGLE_MEMBER_STORED_OR_DEFLATE_V1"
    if (report.get("receipt_basis") != receipt or evidence.get("receipt_basis") != receipt
            or report.get("source_provenance_kind") != acquisition["provenance"]["kind"]
            or report.get("supported_zip_profile") != profile
            or evidence.get("supported_zip_profile") != profile):
        raise ValueError("native archive receipt or supported format differs from preserved evidence")
    # Reject a known origin contradiction; PIT remains a separate native declaration contract.
    if (declaration is not None and acquisition["provenance"]["kind"] == "SYNTHETIC"
            and declaration.get("origin") == "REAL"):
        raise ValueError("catalog origin REAL contradicts preserved SYNTHETIC source provenance")


def history_publication(plugin_id, report, evidence, *, declaration=None):
    metadata = evidence.get("source_metadata")
    formats = ("lokima-dual-capture",) if plugin_id == "polymarket-capture" else (
        "moose-fills", "time-seventeen-v2")
    if (plugin_id not in ("polymarket-capture", "polymarket-archive")
            or not isinstance(metadata, dict) or metadata.get("format") not in formats
            or not isinstance(evidence.get("bars"), list) or len(evidence["bars"]) != report["bars"]
            or not isinstance(report.get("source_reference"), str) or not report["source_reference"].strip()
            or report["source_reference"] != evidence.get("source_reference")):
        raise ValueError("native publication is not a supported source BAR preparation")


def validate_native(plugin_id, report, evidence, *, declaration=None):
    plugin = PLUGINS.get(plugin_id)
    if plugin is None or plugin.validate_native is None:
        raise ValueError("source plugin has no native publication validator")
    plugin.validate_native(report, evidence, declaration=declaration)


def prepare_source(plugin_id, args):
    source = local_path(args.native_output)
    output = local_path(args.output)
    declaration = local_path(args.declaration)
    selection = local_path(args.selection)
    if output == source or source in output.parents or output in source.parents:
        raise ValueError("catalog preparation output must not overlap the original native output")
    paths = {"import_report": (snapshot.safe_local(source, "import-report.json"), MAX_REPORT_BYTES),
             "source_evidence": (snapshot.safe_local(source, "source-evidence.json"), MAX_EVIDENCE_BYTES),
             "declaration": (declaration, MAX_REPORT_BYTES), "selection": (selection, MAX_REPORT_BYTES)}
    originals = {name: file_record(path, limit) for name, (path, limit) in paths.items()}
    declared = load_json(declaration)
    identity = catalog_identity(declared)
    load_json(selection)  # Bounded original JSON only; the native parser owns selection semantics.
    report, evidence = published_native(source)
    providers.integer(report.get("bars"), "native preparation bars", minimum=1, maximum=1_000_000)
    validate_native(plugin_id, report, evidence, declaration=declared)
    if any(file_record(path, limit) != originals[name] for name, (path, limit) in paths.items()):
        raise ValueError("catalog preparation inputs changed during input checks")
    output = run_native(args, ["--catalog", str(source / "catalog"),
                               "--declaration", str(declaration), "--selection", str(selection)])
    if any(file_record(path, limit) != originals[name] for name, (path, limit) in paths.items()):
        raise ValueError("catalog preparation inputs changed during native execution; artifacts retained")
    catalog = published_catalog(output)
    metadata_file = snapshot.safe_local(output, "catalog-metadata.json")
    try:
        body = acquire.local_bytes(metadata_file, MAX_REPORT_BYTES)
        published_identity = catalog_identity(providers.read_json(body))
    except (OSError, ValueError):
        raise ValueError("catalog preparation final metadata is missing, invalid or too large; artifacts retained") from None
    if published_identity != identity:
        raise ValueError("catalog preparation published identity differs from the explicit declaration")
    result = {"schema": "qz.source_preparation/1", "plugin": plugin_id, "status": "CATALOG_PREPARED",
              "output": str(output), "catalog_root": str(catalog), "metadata_file": str(metadata_file),
              "metadata_bytes": len(body), "metadata_sha256": hashlib.sha256(body).hexdigest(),
              "catalog_registration": {"root": str(catalog), "metadata_file": str(metadata_file)},
              "identity_hints": identity,
              "source_artifacts": {name: originals[name] for name in ("import_report", "source_evidence")},
              "admission": dict(acquire.ADMISSION),
              "unperformed_steps": ["runtime_configuration", "source_registration", "source_grant_registration",
                                    "dataset_registration", "frozen_input_set", "fresh_DATA_VALIDATE"]}
    producer = {name: os.environ.get(f"QZ_OPERATOR_{name.upper()}") for name in ("version", "revision", "image")}
    if all(producer.values()):
        result["producer"] = producer  # Informational installed image identity, never source authority.
    return result


def source_plugin(plugin_id, source_format, capabilities, limitations, *, validate_native=None):
    return SourcePlugin({"id": plugin_id, "source_format": source_format,
                         "access": "PUBLIC_FREE", "authentication": "NONE", "operator_only": True,
                         "limitations": limitations}, capabilities, validate_native)


def http_plugin(provider_id):
    return source_plugin(provider_id, "qz.public_acquisition/1", {
        "plan": Capability(http_options, partial(http_plan, provider_id)),
        "download": Capability(partial(http_options, download=True), partial(http_download, provider_id),
                               public_network=True),
        "verify": Capability(http_verify_options, partial(http_verify, provider_id)),
    }, ["Frozen provider descriptors remain unchanged; lifecycle capabilities are listed here",
        "Observed public data and saved terms do not establish permission or historical availability"])


def snapshot_capabilities():
    return {"plan": Capability(snapshot_options, snapshot_plan, public_network=True),
            "download": Capability(partial(snapshot_options, download=True), snapshot_download, public_network=True),
            "verify": Capability(snapshot_verify_options, snapshot_verify)}


PLUGINS = {provider_id: http_plugin(provider_id) for provider_id in providers.PROVIDERS}
PLUGINS["coinbase-candles"].capabilities["convert"] = Capability(candle_options, candle_convert)
PLUGINS["coinbase-candles"].capabilities["prepare"] = Capability(
    prepare_options, partial(prepare_source, "coinbase-candles"))
PLUGINS["coinbase-candles"] = replace(PLUGINS["coinbase-candles"],
    validate_native=partial(candle_publication, "coinbase-candles"))
PLUGINS["polymarket-prices"].descriptor["limitations"].append(
    "PRICE_MARK cannot be converted into trades, OHLCV bars or qualified native research data")
PLUGINS["coinbase-candles"].descriptor["limitations"].append(
    "Native conversion preserves bucket-end events and actual retrieval clocks; consult report admission limits")
PLUGINS["hf-snapshot"] = source_plugin("hf-snapshot", "immutable public Hugging Face files",
    snapshot_capabilities(), ["Raw acquisition only; arbitrary Hugging Face Parquet has no native converter"])
PLUGINS["polymarket-capture"] = source_plugin("polymarket-capture", "lokima-dual-capture",
    snapshot_capabilities() | {"convert": Capability(partial(history_options, capture=True),
                                                       partial(history_convert, capture=True)),
                               "prepare": Capability(prepare_options, partial(prepare_source, "polymarket-capture"))},
    ["Requires original historical Gamma and both recorded CLOB feeds in the supported lokima schema",
     "Matching captured feeds do not prove all-exchange or all-market completeness"],
    validate_native=partial(history_publication, "polymarket-capture"))
PLUGINS["polymarket-archive"] = source_plugin("polymarket-archive", list(ARCHIVE_FORMATS),
    snapshot_capabilities() | {"convert": Capability(history_options, history_convert),
                               "prepare": Capability(prepare_options, partial(prepare_source, "polymarket-archive"))},
    ["Only the three listed existing native formats; original native definitions are required",
     "Archive conversion preserves each native adapter's existing semantics and limitations",
     "Preparation requires existing nonempty BAR output from moose-fills or time-seventeen-v2; joseph-books is unsupported"],
    validate_native=partial(history_publication, "polymarket-archive"))
PLUGINS[binance_vision.PROVIDER["id"]] = source_plugin(binance_vision.PROVIDER["id"], binance_vision.SCHEMA,
    {**{operation: Capability(partial(archive_options, operation=operation), partial(archive_operation, operation))
        for operation in ("plan", "inspect", "freeze")},
     "verify": Capability(archive_verify_options, archive_verify),
     "convert": Capability(partial(candle_options, acquisition_options=archive_verify_options), archive_convert),
     "prepare": Capability(prepare_options, partial(prepare_source, binance_vision.PROVIDER["id"]))},
    ["Offline operator-supplied original archive and checksum only; no download or financial-terms acceptance",
     "Native conversion supports classic single-member stored/deflate ZIP; ZIP64 and other unsupported layouts reject",
     "Original native spot definitions and explicit declared or synthetic receipt clocks are required",
     "SYNTHETIC stays synthetic; declared clocks are unverified and confer no PIT or data-use permission"],
    validate_native=archive_publication)


def plugin_descriptors():
    return [{**plugin.descriptor, "capabilities": sorted(plugin.capabilities),
             "public_network_operations": sorted(operation for operation, capability in plugin.capabilities.items()
                                                 if capability.public_network),
             "admission": dict(acquire.ADMISSION)} for plugin in PLUGINS.values()]


def parser():
    result = argparse.ArgumentParser(description=__doc__, allow_abbrev=False)
    commands = result.add_subparsers(dest="command", required=True)
    commands.add_parser("plugins", help="list actual per-source lifecycle capabilities", allow_abbrev=False).set_defaults(
        run=lambda _: plugin_descriptors())
    operations = sorted({operation for plugin in PLUGINS.values() for operation in plugin.capabilities})
    for operation in operations:
        command = commands.add_parser(operation, allow_abbrev=False)
        sources = command.add_subparsers(dest="plugin", required=True)
        for plugin_id, plugin in PLUGINS.items():
            capability = plugin.capabilities.get(operation)
            if capability is not None:
                sub = sources.add_parser(plugin_id, allow_abbrev=False)
                capability.configure(sub)
                sub.set_defaults(run=capability.run)
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        print(json.dumps(args.run(args), indent=2, allow_nan=False))
        return 0
    except urllib.error.HTTPError as error:
        error.close()
        print(f"source-plugins: public source HTTP error {error.code}; no authenticated or paid fallback", file=sys.stderr)
    except subprocess.TimeoutExpired:
        print("source-plugins: native converter timed out; original diagnostics and artifacts retained", file=sys.stderr)
    except KeyboardInterrupt:
        print("source-plugins: interrupted; original diagnostics and artifacts retained", file=sys.stderr)
        return 130
    except (OSError, ValueError, csv.Error, urllib.error.URLError, http.client.HTTPException) as error:
        message = str(error) if type(error) is ValueError else "source or local I/O failed"
        print(f"source-plugins: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
