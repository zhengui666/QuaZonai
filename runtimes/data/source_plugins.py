#!/usr/bin/env python3
"""Read local Hyperliquid archives and dispatch their native conversion."""

import argparse
from dataclasses import dataclass
from functools import partial
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Callable

import acquire
import cryptohftdata
import providers
import snapshot


@dataclass(frozen=True)
class Capability:
    configure: Callable
    run: Callable
    public_network: bool = False


@dataclass(frozen=True)
class SourcePlugin:
    descriptor: dict
    capabilities: dict[str, Capability]


def local_path(value):
    path = Path(os.path.abspath(value))
    snapshot.safe_local(path.parent, path.name)
    return path


def load_json(path):
    value = providers.read_json(acquire.local_bytes(path))
    if not isinstance(value, dict):
        raise ValueError("expected a JSON object")
    return value


def native_options(parser):
    parser.add_argument("--native-bin", type=Path,
                        help="explicit native executable; otherwise use the matching packaged bin/ executable")
    parser.add_argument("--output", type=Path, required=True, help="new native output directory")


def run_native(args, argv, binary_name):
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
                            shell=False, check=False)
    if result.returncode != 0:
        raise ValueError(f"native converter exited {result.returncode}; original diagnostics and artifacts retained")
    return output


def source_plugin(plugin_id, source_format, capabilities, limitations):
    return SourcePlugin({"id": plugin_id, "source_format": source_format,
                         "access": "PUBLIC_FREE", "authentication": "NONE", "operator_only": True,
                         "limitations": limitations}, capabilities)


def cryptohftdata_options(parser, convert=False, plan=False):
    parser.add_argument("--archive", type=Path, required=True, help="existing CryptoHFTData root containing dataset.json")
    if convert or plan:
        if convert:
            native_options(parser)
        else:
            parser.add_argument("--output", type=Path, required=True, help="new source manifest file")
        parser.add_argument("--market", choices=cryptohftdata.MARKETS, required=True)
        parser.add_argument("--symbol", action="append", required=True, help="exact source filename symbol; repeatable")
        parser.add_argument("--start-date", required=True, help="inclusive UTC YYYY-MM-DD")
        parser.add_argument("--end-date", required=True, help="exclusive UTC YYYY-MM-DD")
        parser.add_argument("--hour", type=int, action="append", help="UTC hour 0-23; omit for every hour")
        parser.add_argument("--stream", choices=tuple(cryptohftdata.STREAMS), action="append",
                            help="repeatable; defaults to books and perpetual context")
        if convert:
            mode = parser.add_mutually_exclusive_group()
            mode.add_argument("--trade-ticks", action="store_true", help="native trade-only catalog")
            mode.add_argument("--cohort-cache", action="store_true", help="derived trade cohort cache")
            mode.add_argument("--cohort-flow-cache", action="store_true", help="derived trade cohort cache with aggressive flow")
            parser.add_argument("--received-time", action="store_true", help="select and group original collector receive times")
            parser.add_argument("--maximum-receive-delay-ns", type=int,
                                help="explicit maximum receive minus exchange-event time for received-time input")
        parser.add_argument("--instruments", type=Path, required=True, help="original native definitions and explicit symbol aliases")


def cryptohftdata_manifest(args):
    manifest = cryptohftdata.selection(args.archive, args.market, args.symbol, args.start_date,
                                      args.end_date, args.stream, args.hour)
    instruments = providers.read_json(acquire.local_bytes(local_path(args.instruments)))
    if not isinstance(instruments, (dict, list)):
        raise ValueError("expected original native instrument definitions")
    return manifest | {"instrument_source": instruments}


def cryptohftdata_plan(args):
    manifest = cryptohftdata_manifest(args)
    output = local_path(args.output)
    cryptohftdata.publish(output, manifest)
    return {"plugin": cryptohftdata.PLUGIN, "status": "PLANNED", "source_manifest": str(output),
            "files": len(manifest["files"]), "missing": len(manifest["missing"]),
            "missing_hours": manifest["missing_hour_count"], "research_qualified": False}


def cryptohftdata_convert(args):
    output = local_path(args.output)
    if output.exists():
        raise ValueError("native output must be a new directory; failed artifacts are never removed")
    manifest = cryptohftdata_manifest(args)
    mode = next((name for name in ("trade_ticks", "cohort_cache", "cohort_flow_cache") if getattr(args, name)), None)
    if mode and manifest["selection"]["streams"] != ["trades"]:
        raise ValueError("trade/catalog cache mode requires --stream trades only")
    if (args.received_time and mode is None or args.received_time != (args.maximum_receive_delay_ns is not None)
            or args.maximum_receive_delay_ns is not None and args.maximum_receive_delay_ns < 0):
        raise ValueError("received-time mode requires a trade/catalog cache mode and a nonnegative maximum receive delay")
    source_file = output.with_name(output.name + ".source.json")
    source_file.parent.mkdir(parents=True, exist_ok=True)
    cryptohftdata.publish(source_file, manifest)
    native = ["import", "--manifest", str(source_file), "--instruments", str(local_path(args.instruments)), "--native-precision"]
    if mode:
        native.append("--" + mode.replace("_", "-"))
    if args.received_time:
        native += ["--received-time", "--maximum-receive-delay-ns", str(args.maximum_receive_delay_ns)]
    run_native(args, native, "market-history")
    report = load_json(output / "import-report.json")
    if (report.get("status") != "IMPORTED" or type(report.get("readback_records")) is not int
            or report["readback_records"] <= 0 or report.get("missing_symbols") or report.get("rejected_rows")):
        raise ValueError("CryptoHFTData native import is incomplete; artifacts retained")
    return {"plugin": cryptohftdata.PLUGIN, "status": "IMPORTED", "output": str(output),
            "catalog_root": str(output / "catalog"), "source_manifest": str(source_file),
            "native_report": report, "research_qualified": False}


PLUGINS = {cryptohftdata.PLUGIN: source_plugin(cryptohftdata.PLUGIN, "CryptoHFTData Hyperliquid Parquet / Parquet.zst",
    {"inspect": Capability(cryptohftdata_options, lambda args: cryptohftdata.inspect(args.archive)),
     "plan": Capability(partial(cryptohftdata_options, plan=True), cryptohftdata_plan),
     "convert": Capability(partial(cryptohftdata_options, convert=True), cryptohftdata_convert)},
    ["Reads an existing downloaded archive; no credentials, network access or raw-file copies",
     "Native conversion preserves published trades, book snapshots, collector clocks and perpetual context; it creates no BARs",
     "Explicit market, symbols, UTC dates and optional hours select existing files; missing hourly paths remain reported",
     "Source download completeness does not establish exchange coverage, historical instrument definitions or research qualification"])}


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
    except subprocess.TimeoutExpired:
        print("source-plugins: native converter timed out; original diagnostics and artifacts retained", file=sys.stderr)
    except KeyboardInterrupt:
        print("source-plugins: interrupted; original diagnostics and artifacts retained", file=sys.stderr)
        return 130
    except (OSError, ValueError) as error:
        message = str(error) if type(error) is ValueError else "source or local I/O failed"
        print(f"source-plugins: {message}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
