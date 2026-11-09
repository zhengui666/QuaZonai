#!/usr/bin/env python3
"""Single source-transfer tools; Pi durable owns scheduling, retries and joins."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tempfile
import time

import acquire
import hf_dataset
import hub
import providers
import snapshot


def encoded(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode()


def validate(kind, plan):
    if kind == "http":
        selection = providers.Selection(**plan["selection"])
        if selection.end_seconds > int(time.time()):
            raise ValueError("historical selection cannot include an unfinished future interval")
        expected = acquire.plan(plan["provider"]["id"], selection)
        if encoded(plan) != encoded(expected):
            raise ValueError("source plan differs from its official provider selection")
        return plan["requests"]
    if kind == "hf":
        hf_dataset.validate_plan(plan)
        return plan["files"]
    if kind == "snapshot":
        if not plan.get("license"):
            raise ValueError("dataset card has no license; supply verified source terms")
        repository, revision = plan.get("repository"), plan.get("revision")
        if (not isinstance(repository, str) or not re.fullmatch(r"[\w.-]+(?:/[\w.-]+)?", repository, re.ASCII)
                or not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision)
                or not isinstance(plan.get("files"), list) or not plan["files"]):
            raise ValueError("immutable snapshot plan required")
        seen = set()
        for item in plan["files"]:
            path = snapshot.checked_path(item["path"])
            if path in seen or item["url"] != snapshot.repository_file_url(repository, revision, path):
                raise ValueError("snapshot source conflicts with frozen revision")
            providers.integer(item["size"], "source file byte size")
            digest, blob = item.get("sha256"), item.get("git_blob")
            if not ((isinstance(digest, str) and re.fullmatch(r"[0-9a-f]{64}", digest))
                    or (isinstance(blob, str) and re.fullmatch(r"[0-9a-f]{40}", blob))):
                raise ValueError("snapshot source lacks an immutable byte identity")
            seen.add(path)
        if sum(item["size"] for item in plan["files"]) != plan["total_bytes"]:
            raise ValueError("snapshot total conflicts with frozen files")
        return plan["files"]
    if kind != "hub":
        raise ValueError("unsupported source unit kind")
    repository, revision = plan.get("repository"), plan.get("revision")
    if (plan.get("schema_version") != 1 or not isinstance(repository, str)
            or not re.fullmatch(r"[\w.-]+/[\w.-]+", repository, re.ASCII)
            or not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision)
            or not isinstance(plan.get("files"), list) or not plan["files"]):
        raise ValueError("immutable public Hub plan required")
    seen = set()
    for item in plan["files"]:
        path = hub.relative_path(item["path"])
        if path in seen or item["url"] != snapshot.repository_file_url(repository, revision, path):
            raise ValueError("Hub unit source differs from frozen revision")
        providers.integer(item["size"], "source file byte size")
        seen.add(path)
    if sum(item["size"] for item in plan["files"]) != plan["total_bytes"]:
        raise ValueError("Hub total differs from frozen files")
    return plan["files"]


def prepared(kind, plan):
    units = validate(kind, plan)
    identity = hashlib.sha256(encoded({"kind": kind, "plan": plan})).hexdigest()
    info = {"schemaVersion": 1, "kind": kind, "planId": identity,
            "concurrency": 1 if kind == "http" else 4,
            "minIntervalMs": 350 if kind == "http" else 0,
            "units": [{"id": f"{identity}-{index}", "index": index}
                      for index in range(len(units))]}
    return units, info


def descriptor(kind, plan):
    return prepared(kind, plan)[1]


def unit_root(workdir, index):
    workdir = Path(os.path.abspath(workdir))
    workdir.mkdir(parents=True, exist_ok=True)
    snapshot.safe_local(workdir, "units")
    (workdir / "units").mkdir(exist_ok=True)
    return snapshot.safe_local(workdir, f"units/{index:08d}")


def checked_receipt(kind, plan, workdir, index, units, info):
    if type(index) is not int or not 0 <= index < len(info["units"]):
        raise ValueError("source unit index outside frozen plan")
    directory = unit_root(workdir, index)
    value = json.loads((directory / "receipt.json").read_bytes())
    if (value.get("planId") != info["planId"] or value.get("kind") != kind
            or value.get("unit") != info["units"][index]):
        raise ValueError("source unit receipt belongs to another plan")
    item = units[index]
    if kind == "http":
        body = (directory / "response.bin").read_bytes()
        selection = providers.Selection(**plan["selection"])
        provider = providers.provider_by_id(plan["provider"]["id"])
        observation = value["result"]["observation"]
        acquire.check_observation(observation, selection)
        _, counts = acquire.decoded_page(provider, body, selection, item, observation,
                                        f"raw/{index:04d}.json")
        if counts != value["result"]["counts"] or len(body) != value["result"]["bytes"]:
            raise ValueError("source unit response differs from its receipt")
    elif kind == "hf":
        hf_dataset.inspect_file(Path(value["result"]["local_path"]), item)
    elif kind == "snapshot":
        path = Path(value["result"]["local_path"])
        if path.is_symlink() or not path.is_file() or path.stat().st_size != item["size"]:
            raise ValueError("snapshot source unit byte size mismatch")
        if snapshot.file_hash(path, item["git_blob"]) != value["result"]["sha256"]:
            raise ValueError("snapshot source unit SHA-256 mismatch")
        if item["sha256"] and item["sha256"] != value["result"]["sha256"]:
            raise ValueError("snapshot source unit SHA-256 mismatch")
    else:
        path = Path(value["result"]["local_path"])
        if path.is_symlink() or not path.is_file() or path.stat().st_size != item["size"]:
            raise ValueError("Hub source unit differs from frozen file")
    return value


def receipt(kind, plan, workdir, index):
    units, info = prepared(kind, plan)
    return checked_receipt(kind, plan, workdir, index, units, info)


def execute(kind, plan, workdir, index, cache_dir=None):
    units, info = prepared(kind, plan)
    if type(index) is not int or not 0 <= index < len(info["units"]):
        raise ValueError("source unit index outside frozen plan")
    target = unit_root(workdir, index)
    if target.exists():
        return checked_receipt(kind, plan, workdir, index, units, info)
    stage = Path(tempfile.mkdtemp(prefix=".unit-", dir=target.parent))
    try:
        item = units[index]
        if kind == "http":
            body, observed = acquire.fetch(item["url"])
            selection = providers.Selection(**plan["selection"])
            acquire.check_observation(observed, selection)
            provider = providers.provider_by_id(plan["provider"]["id"])
            _, counts = acquire.decoded_page(provider, body, selection, item, observed,
                                            f"raw/{index:04d}.json")
            snapshot.publish_bytes(stage / "response.bin", body)
            result = {"observation": observed, "counts": counts, "bytes": len(body)}
        elif kind == "hf":
            if cache_dir is None:
                raise ValueError("HF source unit requires cache directory")
            root = hf_dataset.cache_root(cache_dir, plan)
            result = snapshot.acquire_cached_file(root, item, hf_dataset.inspect_file)
        elif kind == "snapshot":
            if cache_dir is None:
                raise ValueError("snapshot source unit requires cache directory")
            root = Path(os.path.abspath(cache_dir)) / plan["repository"] / plan["revision"]
            root.mkdir(parents=True, exist_ok=True)
            digest = snapshot.acquire_file(root, item)
            result = {"local_path": str(snapshot.safe_local(root, item["path"])), "sha256": digest}
        else:
            if cache_dir is None:
                raise ValueError("Hub source unit requires cache directory")
            root = Path(os.path.abspath(cache_dir)) / plan["repository"] / plan["revision"]
            root.mkdir(parents=True, exist_ok=True)
            hub.acquire_file(root, item)
            result = {"local_path": str(hub.local_file(root, item["path"])), "bytes": item["size"]}
        value = {"schemaVersion": 1, "planId": info["planId"], "kind": kind,
                 "unit": info["units"][index], "result": result, "completedAt": acquire.now()}
        snapshot.publish_bytes(stage / "receipt.json", encoded(value))
        try:
            os.rename(stage, target)
        except OSError:
            if not target.exists():
                raise
        return checked_receipt(kind, plan, workdir, index, units, info)
    finally:
        if stage.exists():
            shutil.rmtree(stage)


def publish(kind, plan, workdir, output, terms_file=None, cache_dir=None):
    """Join verified completed artifacts; no transfers or retries happen here."""
    units, info = prepared(kind, plan)
    output = Path(os.path.abspath(output))
    snapshot.safe_local(output, "source-unit-publication.json")
    if output.exists():
        marker = snapshot.safe_local(output, "source-unit-publication.json")
        if not marker.is_file():
            raise ValueError("existing source output is not a completed publication")
        if json.loads(marker.read_bytes()).get("planId") != info["planId"]:
            raise ValueError("source publication belongs to another plan")
    parts = [checked_receipt(kind, plan, workdir, index, units, info)
             for index in range(len(units))]
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.exists():
        if json.loads((output / "source-unit-publication.json").read_bytes())["planId"] != info["planId"]:
            raise ValueError("source publication belongs to another plan")
        if kind == "http":
            if terms_file is None or acquire.local_bytes(terms_file) != (output / "source-terms.bin").read_bytes():
                raise ValueError("source terms differ from completed publication")
            acquire.verify(output)
            result = json.loads((output / "acquisition.json").read_bytes())
            for index, (response, part) in enumerate(zip(result["responses"], parts)):
                if (response["observation"] != part["result"]["observation"]
                        or response["counts"] != part["result"]["counts"]
                        or (output / response["file"]["path"]).read_bytes()
                        != (unit_root(workdir, index) / "response.bin").read_bytes()):
                    raise ValueError("HTTP publication differs from completed unit receipts")
            return result
        if kind == "hf":
            hf_dataset.verify(output / "selection.json")
            result = json.loads((output / "selection.json").read_bytes())
            if (result["plan"] != plan or result["files"] != [
                    {**item, **part["result"]} for item, part in zip(units, parts)]):
                raise ValueError("selection differs from frozen source plan or unit receipts")
            return result
        if kind == "snapshot":
            return verify_snapshot_publication(plan, output)
        result = json.loads((output / "market-data.json").read_bytes())
        root = Path(result["source_root"])
        if (any(result.get(key) != value for key, value in plan.items())
                or any(Path(part["result"]["local_path"]) != root / item["path"]
                       for item, part in zip(units, parts))):
            raise ValueError("Hub publication differs from frozen plan or completed units")
        return result
    stage = Path(tempfile.mkdtemp(prefix=".source-publication-", dir=output.parent))
    try:
        if kind == "http":
            if terms_file is None:
                raise ValueError("HTTP publication requires original source terms")
            terms = acquire.local_bytes(terms_file)
            if not terms.strip():
                raise ValueError("original source terms are empty")
            snapshot.publish_bytes(stage / "source-terms.bin", terms)
            (stage / "raw").mkdir()
            provider = providers.provider_by_id(plan["provider"]["id"])
            selection = providers.Selection(**plan["selection"])
            responses, accumulated, raw_bytes, previous = [], {}, 0, None
            for index, part in enumerate(parts):
                body = (unit_root(workdir, index) / "response.bin").read_bytes()
                observed = part["result"]["observation"]
                previous = acquire.check_observation(observed, selection, previous)
                path = f"raw/{index:04d}.json"
                rows, counts = acquire.decoded_page(provider, body, selection, plan["requests"][index], observed, path)
                acquire.collect_rows(accumulated, rows)
                snapshot.publish_bytes(stage / path, body)
                responses.append({"request": plan["requests"][index], "observation": observed,
                                  "file": {"path": path, "size": len(body)}, "counts": counts})
                raw_bytes += len(body)
            records = acquire.record_bytes(accumulated)
            snapshot.publish_bytes(stage / "records.jsonl", records)
            result = {**plan, "created_at": acquire.now(),
                      "source_terms": {"file": {"path": "source-terms.bin", "size": len(terms)},
                                       "reference": provider.descriptor["terms_reference"],
                                       "evidence_status": "OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED"},
                      "responses": responses, "records": {"path": "records.jsonl", "size": len(records)},
                      "record_count": len(accumulated), "raw_response_bytes": raw_bytes,
                      "observation_status": "OBSERVED" if accumulated else "NO_OBSERVATIONS"}
            snapshot.publish_bytes(stage / "acquisition.json", encoded(result))
            acquire.verify(stage)
        elif kind == "hf":
            if cache_dir is None:
                raise ValueError("HF publication requires cache directory")
            root = hf_dataset.cache_root(cache_dir, plan)
            files = [{**item, **part["result"]} for item, part in zip(plan["files"], parts)]
            result = hf_dataset.selection_result(plan, root, files, acquire.now())
            hf_dataset.validate_manifest(result, check_files=True)
            snapshot.publish_bytes(stage / "selection.json", encoded(result))
        elif kind == "snapshot":
            files = []
            for item, part in zip(units, parts):
                target = snapshot.safe_local(stage, item["path"])
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(part["result"]["local_path"], target)
                files.append({key: item[key] for key in ("path", "size", "url")}
                             | {"sha256": part["result"]["sha256"]})
            identity = {key: plan[key] for key in ("repository", "revision", "license", "license_reference")}
            result = {"schema_version": 1, **identity, "retrieved_at": acquire.now(), "files": files}
            snapshot.publish_bytes(stage / "snapshot.json", encoded(result))
            verify_snapshot_publication(plan, stage)
        else:
            root = Path(parts[0]["result"]["local_path"])
            for _ in Path(plan["files"][0]["path"]).parts:
                root = root.parent
            if any(Path(part["result"]["local_path"]) != root / item["path"]
                   for item, part in zip(plan["files"], parts)):
                raise ValueError("Hub units do not share their frozen source root")
            result = {**plan, "source_root": str(root), "retrieved_at": acquire.now()}
            snapshot.publish_bytes(stage / "market-data.json", encoded(result))
        snapshot.publish_bytes(stage / "source-unit-publication.json", encoded({"planId": info["planId"]}))
        try:
            os.rename(stage, output)
        except OSError:
            if not (output / "source-unit-publication.json").is_file():
                raise
            return publish(kind, plan, workdir, output, terms_file, cache_dir)
        return result
    finally:
        if stage.exists():
            shutil.rmtree(stage)


def verify_snapshot_publication(plan, output):
    value = snapshot.fetch_local_manifest(snapshot.safe_local(output, "snapshot.json"))
    keys = ("repository", "revision", "license", "license_reference")
    if (value.get("schema_version") != 1 or any(value.get(key) != plan[key] for key in keys)
            or len(value.get("files", [])) != len(plan["files"])):
        raise ValueError("snapshot manifest conflicts with frozen selection")
    for recorded, item in zip(value["files"], plan["files"]):
        if (any(recorded.get(key) != item[key] for key in ("path", "size", "url"))
                or (item["sha256"] and item["sha256"] != recorded.get("sha256"))):
            raise ValueError("snapshot manifest conflicts with frozen file")
        path = snapshot.safe_local(output, item["path"])
        if (not path.is_file() or path.stat().st_size != item["size"]
                or snapshot.file_hash(path, item["git_blob"]) != recorded.get("sha256")):
            raise ValueError("snapshot file size or SHA-256 mismatch")
    return value


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("plan", "execute", "publish"))
    parser.add_argument("--kind", choices=("http", "hf", "hub", "snapshot"), required=True)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--workdir", type=Path)
    parser.add_argument("--unit-index", type=int)
    parser.add_argument("--cache-dir", type=Path)
    parser.add_argument("--terms-file", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    plan = json.loads(args.plan.read_bytes())
    if args.command == "plan":
        result = descriptor(args.kind, plan)
    elif args.command == "execute":
        if args.workdir is None or args.unit_index is None:
            parser.error("execute requires --workdir and --unit-index")
        result = execute(args.kind, plan, args.workdir, args.unit_index, args.cache_dir)
    else:
        if args.workdir is None or args.output is None:
            parser.error("publish requires --workdir and --output")
        result = publish(args.kind, plan, args.workdir, args.output, args.terms_file, args.cache_dir)
    print(json.dumps(result, allow_nan=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
