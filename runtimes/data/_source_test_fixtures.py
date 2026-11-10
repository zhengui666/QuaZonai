"""Test-only synchronous driver for the production single-unit tool protocol."""
from pathlib import Path

import acquire
import source_units


def run_units(kind, plan, output, *, terms_file=None, cache_dir=None):
    info = source_units.descriptor(kind, plan)
    output = Path(output)
    workdir = output.parent / ("." + output.name + "-units") / info["planId"]
    source_units.snapshot.safe_local(output, "source-unit-publication.json")
    if terms_file is not None and not acquire.local_bytes(terms_file).strip():
        raise ValueError("original source terms are empty")
    if output.exists():
        return source_units.publish(kind, plan, workdir, output, terms_file, cache_dir)
    # Only tests own this synchronous loop. Production units are official Pi Tasks.
    for unit in info["units"]:
        source_units.execute(kind, plan, workdir, unit["index"], cache_dir)
    return source_units.publish(kind, plan, workdir, output, terms_file, cache_dir)


def http_acquisition(provider_id, selection, output, terms_file, **options):
    return run_units("http", acquire.plan(provider_id, selection, **options), output,
                     terms_file=terms_file)


def hf_acquisition(plan, cache_dir, output):
    return run_units("hf", plan, output, cache_dir=cache_dir)


def hub_acquisition(plan, cache_dir, output):
    return run_units("hub", plan, output, cache_dir=cache_dir)


def snapshot_acquisition(plan, output):
    output = Path(output)
    return run_units("snapshot", plan, output, cache_dir=output.parent / "snapshot-cache")
