#!/usr/bin/env python3
"""Freeze bounded public EVM logs and headers; never qualifies historical availability.

No resume or overwrite: choose a new output after a failed or completed acquisition.
The archive records observations and source URLs, not a data license or PIT claim.
Connection failures before a response opens allow three attempts with 1s/2s backoff.
HTTP errors and response-body failures are never retried. Each open has a 30s timeout;
the 60s body-reading deadline starts after open succeeds, not before all attempts.
"""

import argparse
import datetime
import hashlib
import http.client
import ipaddress
import json
import os
from pathlib import Path
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

from snapshot import DEFAULT_MAX_BYTES, publish_bytes, safe_local


MAX_BLOCKS = 4096
MAX_PROVIDERS = 4
MAX_ADDRESSES = 256
MAX_RESPONSE_BYTES = 32 * 1024 * 1024
CHUNK = 64 * 1024
ZERO_HASH = "0x" + "00" * 32


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


OPENER = urllib.request.build_opener(NoRedirect)


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")


def u64(value):
    if type(value) is not int or not 0 <= value < 2**64:
        raise ValueError("unsigned 64-bit integer required")
    return value


def cli_u64(value):
    try:
        return u64(int(value, 16 if value.startswith("0x") else 10))
    except ValueError:
        raise argparse.ArgumentTypeError("unsigned 64-bit integer required") from None


def select_blocks(blocks=None, first=None, last=None):
    if blocks is not None:
        if first is not None or last is not None:
            raise ValueError("choose explicit blocks or an inclusive block range")
        selected = sorted({u64(block) for block in blocks})
    else:
        u64(first)
        u64(last)
        if first > last or last - first >= MAX_BLOCKS:
            raise ValueError("invalid or oversized block range")
        selected = list(range(first, last + 1))
    if not 1 <= len(selected) <= MAX_BLOCKS:
        raise ValueError("select between 1 and 4096 blocks")
    return selected


def endpoint_host(endpoint):
    if (not isinstance(endpoint, str) or any(c.isspace() or ord(c) < 32 for c in endpoint)
            or "?" in endpoint or "#" in endpoint):
        raise ValueError("anonymous public HTTPS RPC URL required")
    parsed = urllib.parse.urlsplit(endpoint)
    host = (parsed.hostname or "").lower().rstrip(".")
    if (parsed.scheme != "https" or not host or parsed.username is not None
            or parsed.password is not None or parsed.port == 0):
        raise ValueError("anonymous public HTTPS RPC URL required")
    try:
        address = ipaddress.ip_address(host)
    except ValueError:
        if "." not in host or host.endswith(".localhost"):
            raise ValueError("public RPC host required") from None
    else:
        if not address.is_global or address.is_multicast:
            raise ValueError("public RPC host required")
    return host


def hex_bytes(value, length=None):
    if (not isinstance(value, str) or not re.fullmatch(r"0x(?:[0-9a-fA-F]{2})*", value)
            or (length is not None and len(value) != 2 + 2 * length)):
        raise ValueError("invalid RPC byte field")
    return value.lower()


def quantity(value):
    if not isinstance(value, str) or not re.fullmatch(r"0x(?:0|[1-9a-fA-F][0-9a-fA-F]*)", value):
        raise ValueError("invalid RPC quantity")
    return u64(int(value, 16))


def unique_object(pairs):
    result = dict(pairs)
    if len(result) != len(pairs):
        raise ValueError("duplicate JSON object member")
    return result


def invalid_constant(_):
    raise ValueError("invalid JSON numeric constant")


def rpc_call(endpoint, method, params, remaining):
    limit = min(MAX_RESPONSE_BYTES, remaining)
    if limit <= 0:
        raise ValueError("download byte budget exhausted")
    request = urllib.request.Request(
        endpoint,
        data=json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode(),
        headers={"Content-Type": "application/json", "Accept-Encoding": "identity",
                 "User-Agent": "QuaZonai-public-data/2.0"},
    )
    for attempt in range(3):
        try:
            response = OPENER.open(request, timeout=30)
            break
        except urllib.error.HTTPError as error:
            # HTTPError is a URLError; payment, access and server errors stay final.
            error.close()
            raise
        except (urllib.error.URLError, TimeoutError, ConnectionError):
            if attempt == 2:
                raise
            time.sleep(attempt + 1)
    deadline = time.monotonic() + 60
    body = bytearray()
    with response:
        if response.status != 200 or response.headers.get("Content-Encoding", "identity") != "identity":
            raise ValueError("unexpected RPC HTTP response")
        declared = response.headers.get("Content-Length")
        if declared is not None:
            if not re.fullmatch(r"[0-9]+", declared) or int(declared) > limit:
                raise ValueError("RPC response exceeds byte budget")
            declared = int(declared)
        while len(body) < limit:
            if time.monotonic() > deadline:
                raise ValueError("RPC response deadline exceeded")
            chunk = response.read1(min(CHUNK, limit - len(body)))
            if time.monotonic() > deadline:
                raise ValueError("RPC response deadline exceeded")
            if not chunk:
                break
            body.extend(chunk)
        # Without a length, reaching the limit cannot prove EOF without reading
        # beyond the budget. Fail closed instead of consuming an extra byte.
        if ((declared is not None and len(body) != declared)
                or (declared is None and len(body) == limit)):
            raise ValueError("RPC response truncated or byte budget exhausted")
    value = json.loads(body, object_pairs_hook=unique_object, parse_constant=invalid_constant)
    if (not isinstance(value, dict) or value.get("jsonrpc") != "2.0"
            or type(value.get("id")) is not int or value["id"] != 1
            or "error" in value or "result" not in value):
        raise ValueError("invalid or failed JSON-RPC response")
    return value, len(body)


def canonical_header(response, number):
    header = response["result"]
    if not isinstance(header, dict) or quantity(header.get("number")) != number:
        raise ValueError("missing or mismatched block header")
    result = {"number": number, "timestamp": quantity(header.get("timestamp"))}
    for key in ("hash", "parentHash", "receiptsRoot", "transactionsRoot", "stateRoot"):
        result[key] = hex_bytes(header.get(key), 32)
    if result["hash"] == ZERO_HASH:
        raise ValueError("zero block hash")
    transactions = header.get("transactions")
    if not isinstance(transactions, list):
        raise ValueError("block transaction hashes required")
    result["transactions"] = [hex_bytes(value, 32) for value in transactions]
    if len(set(result["transactions"])) != len(transactions):
        raise ValueError("duplicate block transaction hash")
    return result


def canonical_logs(response, header, addresses, topic0):
    logs = response["result"]
    if not isinstance(logs, list):
        raise ValueError("RPC log array required")
    result = []
    seen = set()
    for log in logs:
        if not isinstance(log, dict) or log.get("removed") is not False:
            raise ValueError("removed or invalid RPC log")
        block_hash = hex_bytes(log.get("blockHash"), 32)
        number = quantity(log.get("blockNumber"))
        transaction = hex_bytes(log.get("transactionHash"), 32)
        transaction_index = quantity(log.get("transactionIndex"))
        index = quantity(log.get("logIndex"))
        address = hex_bytes(log.get("address"), 20)
        topics = log.get("topics")
        if not isinstance(topics, list) or not 1 <= len(topics) <= 4:
            raise ValueError("invalid RPC log topics")
        topics = tuple(hex_bytes(topic, 32) for topic in topics)
        data = hex_bytes(log.get("data"))
        if (block_hash != header["hash"] or number != header["number"] or transaction == ZERO_HASH
                or transaction_index >= len(header["transactions"])
                or header["transactions"][transaction_index] != transaction
                or address not in addresses or topics[0] != topic0 or index in seen):
            raise ValueError("log identity, query or transaction mismatch")
        seen.add(index)
        result.append((index, transaction_index, transaction, address, topics, data, block_hash, number))
    result.sort()
    if any(a[1] > b[1] for a, b in zip(result, result[1:])):
        raise ValueError("log indices contradict transaction order")
    return result


def download(chain_id, endpoints, blocks, addresses, topic0, output, max_bytes=DEFAULT_MAX_BYTES):
    u64(chain_id)
    blocks = select_blocks(blocks)
    if type(max_bytes) is not int or max_bytes <= 0:
        raise ValueError("positive byte budget required")
    if not 2 <= len(endpoints) <= MAX_PROVIDERS:
        raise ValueError("select between two and four public RPC endpoints")
    hosts = [endpoint_host(endpoint) for endpoint in endpoints]
    if len(set(hosts)) != len(hosts):
        raise ValueError("every RPC endpoint must use a different public host")
    if not 1 <= len(addresses) <= MAX_ADDRESSES:
        raise ValueError("select between one and 256 contract addresses")
    addresses = sorted({hex_bytes(address, 20) for address in addresses})
    topic0 = hex_bytes(topic0, 32)
    output = Path(os.path.abspath(output))
    safe_local(output.parent, output.name)
    if output.exists():
        raise ValueError("output already exists; choose a new output file")
    remaining = max_bytes

    def request(endpoint, method, params):
        nonlocal remaining
        response, consumed = rpc_call(endpoint, method, params, remaining)
        remaining -= consumed
        return response

    observations = []
    canonical = {}
    rows = 0
    latest_received = None
    for provider_index, endpoint in enumerate(endpoints):
        chain = request(endpoint, "eth_chainId", [])
        if quantity(chain["result"]) != chain_id:
            raise ValueError("RPC chain ID mismatch")
        observation = {"endpoint": endpoint, "chain_id": chain, "blocks": []}
        previous = None
        for number in blocks:
            header = request(endpoint, "eth_getBlockByNumber", [hex(number), False])
            identity = canonical_header(header, number)
            logs = request(endpoint, "eth_getLogs", [{
                "blockHash": identity["hash"], "address": addresses, "topics": [topic0],
            }])
            received_at = now()
            received_time = datetime.datetime.fromisoformat(received_at.replace("Z", "+00:00"))
            if identity["timestamp"] > int(received_time.timestamp()):
                raise ValueError("block timestamp is later than RPC observation")
            latest_received = max(latest_received, received_time) if latest_received else received_time
            events = canonical_logs(logs, identity, addresses, topic0)
            if provider_index == 0:
                if previous is not None:
                    if identity["timestamp"] < previous["timestamp"]:
                        raise ValueError("block timestamps contradict block order")
                    if number == previous["number"] + 1 and identity["parentHash"] != previous["hash"]:
                        raise ValueError("adjacent blocks do not share a parent link")
                canonical[number] = (identity, events)
                rows += len(events)
            elif canonical[number] != (identity, events):
                raise ValueError("RPC providers disagree on canonical block or logs")
            observation["blocks"].append({"number": number, "header": header,
                                           "logs": logs, "received_at": received_at})
            previous = identity
        observations.append(observation)
    retrieved_at = now()
    if datetime.datetime.fromisoformat(retrieved_at.replace("Z", "+00:00")) < latest_received:
        raise ValueError("local observation clock moved backwards")
    archive = {"schema_version": 1, "chain_id": chain_id, "retrieved_at": retrieved_at,
               "query": {"blocks": blocks, "addresses": addresses, "topic0": topic0},
               "observations": observations}
    content = (json.dumps(archive, ensure_ascii=False, separators=(",", ":")) + "\n").encode()
    if len(content) > max_bytes:
        raise ValueError("archive exceeds byte budget")
    output.parent.mkdir(parents=True, exist_ok=True)
    safe_local(output.parent, output.name)
    publish_bytes(output, content)
    return {"file": str(output), "bytes": len(content), "sha256": hashlib.sha256(content).hexdigest(),
            "rows": rows, "blocks": len(blocks), "providers": len(endpoints),
            "downloaded_bytes": max_bytes - remaining}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chain-id", required=True, type=cli_u64)
    parser.add_argument("--rpc", required=True, action="append", help="anonymous public HTTPS endpoint; two to four unique hosts")
    selection = parser.add_mutually_exclusive_group(required=True)
    selection.add_argument("--block", action="append", type=cli_u64)
    selection.add_argument("--from-block", type=cli_u64)
    parser.add_argument("--to-block", type=cli_u64, help="inclusive range end")
    parser.add_argument("--address", required=True, action="append", help="contract address; at most 256")
    parser.add_argument("--topic0", required=True)
    parser.add_argument("--max-bytes", type=cli_u64, default=DEFAULT_MAX_BYTES,
                        help="total response-body and output byte limit")
    parser.add_argument("--output", required=True, type=Path, help="new JSON file; no resume or overwrite")
    args = parser.parse_args(argv)
    try:
        blocks = select_blocks(args.block, args.from_block, args.to_block)
        summary = download(args.chain_id, args.rpc, blocks, args.address, args.topic0,
                           args.output, args.max_bytes)
        print(json.dumps(summary))
        return 0
    except (OSError, ValueError, http.client.HTTPException, urllib.error.URLError) as error:
        # RPC error bodies, redirects and exception URLs can contain credentials.
        if isinstance(error, urllib.error.HTTPError):
            error.close()
        print("evm: acquisition failed; no archive published", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
