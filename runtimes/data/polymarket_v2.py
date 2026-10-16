"""TimeSeventeen v2 selection intent; transport and native semantics remain shared."""

import datetime
from decimal import Decimal
import json
from pathlib import Path
import re

import acquire
import hf_dataset
import providers
import snapshot


PLUGIN = "polymarket-v2-hf"
REPOSITORY = "TimeSeventeen/Polymarket-v2"
FORMAT = "time-seventeen-v2"
REQUEST_KEY = "polymarket_v2"
REQUEST_SCHEMA = "qz.polymarket_v2_request/1"


def source_request(start_date, end_date, tokens):
    start, end = hf_dataset.date(start_date), hf_dataset.date(end_date)
    if start >= end:
        raise ValueError("Polymarket v2 requires start_date < end_date")
    if (not isinstance(tokens, list) or not tokens
            or any(not isinstance(token, str) or len(token) > 78
                   or not re.fullmatch(r"[1-9][0-9]*", token, re.ASCII)
                   or int(token) >= 1 << 256 for token in tokens)
            or len(set(tokens)) != len(tokens)):
        raise ValueError("Polymarket v2 requires unique positive uint256 token strings")
    return {"schema": REQUEST_SCHEMA, "start_date": start_date, "end_date": end_date,
            "tokens": tokens}


def files(request):
    start, end = hf_dataset.date(request["start_date"]), hf_dataset.date(request["end_date"])
    return ["OrderFilled/" + (start + datetime.timedelta(days=offset)).strftime("%Y_%m_%d") + ".parquet"
            for offset in range((end - start).days)]


def plan(start_date, end_date, tokens, revision=None, max_bytes=None):
    request = source_request(start_date, end_date, tokens)
    result = hf_dataset.plan(REPOSITORY, files(request), revision, max_bytes)
    result[REQUEST_KEY] = request
    validate_plan(result)
    return result


def validate_plan(value):
    hf_dataset.validate_plan(value)
    request = value.get(REQUEST_KEY)
    if not isinstance(request, dict):
        raise ValueError("HF plan has no original Polymarket v2 request")
    expected = source_request(request.get("start_date"), request.get("end_date"), request.get("tokens"))
    paths = files(expected)
    if (request != expected or value["repository"] != REPOSITORY
            or value["partition_index"] is not None
            or value["request"] != {"includes": paths, "markets": [], "start_date": None, "end_date": None}
            or [item["path"] for item in value["files"]] != paths
            or any(item["format"] != "parquet" for item in value["files"])):
        raise ValueError("HF plan differs from its exact Polymarket v2 daily file request")
    return request


def read_plan(path):
    value = providers.read_json(acquire.local_bytes(Path(path).absolute(), None))
    validate_plan(value)
    return value


def validate_selection(value):
    hf_dataset.validate_manifest(value)
    return validate_plan(value["plan"])


def read_selection(path):
    value = providers.read_json(acquire.local_bytes(Path(path).absolute(), None))
    validate_selection(value)
    return value


def window(request, start_seconds=None, end_seconds=None):
    lower, upper = [int(datetime.datetime.combine(hf_dataset.date(request[key]), datetime.time.min,
                                                 datetime.timezone.utc).timestamp())
                    for key in ("start_date", "end_date")]
    if (start_seconds is None) != (end_seconds is None):
        raise ValueError("both native UTC window boundaries are required")
    start, end = (lower, upper) if start_seconds is None else (start_seconds, end_seconds)
    providers.integer(start, "start_seconds", maximum=None)
    providers.integer(end, "end_seconds", maximum=None)
    if not lower <= start < end <= upper:
        raise ValueError("native conversion window exceeds the original Polymarket v2 dates")
    return start, end


def definitions(request, value):
    if not isinstance(value, list) or not value:
        raise ValueError("original native InstrumentAny definitions are required")
    symbols = []
    for definition in value:
        if not isinstance(definition, dict) or set(definition) != {"BinaryOption"}:
            raise ValueError("Polymarket v2 requires original BinaryOption definitions")
        payload = definition["BinaryOption"]
        if (not isinstance(payload, dict) or payload.get("currency") != "pUSD"
                or not isinstance(payload.get("raw_symbol"), str)):
            raise ValueError("Polymarket v2 requires original pUSD token definitions")
        symbols.append(payload["raw_symbol"])
    if len(symbols) != len(set(symbols)) or set(symbols) != set(request["tokens"]):
        raise ValueError("original instrument tokens differ from the Polymarket v2 request")


def publication(metadata, declaration=None):
    if (not isinstance(metadata, dict) or metadata.get("format") != FORMAT
            or not isinstance(metadata.get("selection_manifest"), dict)):
        raise ValueError("native publication is not the requested Polymarket v2 source")
    request = validate_selection(metadata["selection_manifest"])
    selection = metadata.get("selection")
    if not isinstance(selection, dict):
        raise ValueError("native Polymarket v2 publication has no UTC window")
    window(request, selection.get("start_seconds"), selection.get("end_seconds"))
    definitions(request, metadata.get("instruments_definitions"))
    if declaration is not None and declaration.get("pit_status") != "UNVERIFIED":
        raise ValueError("Polymarket v2 archive preparation must retain UNVERIFIED historical availability")
    return request


def definition_json(value):
    """Canonical private lineage without float/u64 coercion of native JSON numbers."""
    if isinstance(value, Decimal):
        if not value.is_finite():
            raise ValueError("native definition numbers must be finite")
        return str(value)
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(key) + ":" + definition_json(value[key]) for key in sorted(value)) + "}"
    if isinstance(value, list):
        return "[" + ",".join(definition_json(item) for item in value) + "]"
    return json.dumps(value, allow_nan=False)


def publish(path, value):
    path = Path(path).absolute()
    snapshot.safe_local(path.parent, path.name)
    if path.exists():
        raise ValueError("Polymarket v2 output must be a new file")
    path.parent.mkdir(parents=True, exist_ok=True)
    snapshot.safe_local(path.parent, path.name)
    snapshot.publish_bytes(path, hf_dataset.manifest_bytes(value))
    return value
