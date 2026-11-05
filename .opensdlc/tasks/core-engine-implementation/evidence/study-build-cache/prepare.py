#!/usr/bin/env python3
"""Freeze a tracked candidate patch; create a one-change, same-version baseline.

No compilation, measurement, or change to the original worktree is performed.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def sha(data):
    return hashlib.sha256(data).hexdigest()


def git(repo, *args, data=None):
    return subprocess.run(
        ["git", "-C", str(repo), *args], input=data, check=True,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    ).stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, required=True)
    parser.add_argument("--work-root", type=Path, required=True)
    parser.add_argument("--candidate-job", type=Path, required=True)
    args = parser.parse_args()
    repo, root = args.repo.resolve(), args.work_root.resolve()
    root.mkdir(exist_ok=False)
    head = git(repo, "rev-parse", "HEAD").decode().strip()
    patch = git(repo, "diff", "HEAD", "--binary")
    # A tracked patch cannot reproduce new untracked production modules.
    unexpected = git(
        repo, "ls-files", "--others", "--exclude-standard", "--",
        "apps/job/src", "crates", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml",
    ).decode().strip()
    if unexpected:
        raise RuntimeError(f"untracked production source is not frozen: {unexpected}")
    paths = git(repo, "ls-files", "-z").decode().split("\0")[:-1]
    hashes = {
        name: sha((repo / name).read_bytes()) if (repo / name).is_file() else None
        for name in paths
    }
    if git(repo, "diff", "HEAD", "--binary") != patch:
        raise RuntimeError("tracked tree changed while freezing patch")
    (root / "candidate.patch").write_bytes(patch)
    (root / "candidate-tracked-sha256.json").write_text(json.dumps(hashes, indent=2) + "\n")
    example = Path(__file__).with_name("qz_core_pair.rs")
    proof = Path(__file__).with_name("qz_cache_proof.rs")
    for arm in ("candidate", "baseline"):
        tree = root / arm
        git(repo, "worktree", "add", "--detach", str(tree), head)
        if patch:
            git(tree, "apply", "--binary", "-", data=patch)
        for name, expected in hashes.items():
            path = tree / name
            actual = sha(path.read_bytes()) if path.is_file() else None
            if actual != expected:
                raise RuntimeError(f"{arm} does not match frozen source: {name}")
        shutil.copyfile(example, tree / "apps/job/examples/qz_core_pair.rs")
        shutil.copyfile(proof, tree / "apps/job/examples/qz_cache_proof.rs")
    signal = root / "baseline/apps/job/src/signals.rs"
    contents = signal.read_text()
    for before, after in (
        ("            max_entries: 32,", "            max_entries: 0,"),
        ("            max_source_bytes: 16 * 1024 * 1024,", "            max_source_bytes: 0,"),
    ):
        if contents.count(before) != 1:
            raise RuntimeError(f"unexpected cache default: {before}")
        contents = contents.replace(before, after)
    signal.write_text(contents)
    diff = subprocess.run(
        ["diff", "-u", str(root / "candidate/apps/job/src/signals.rs"), str(signal)],
        stdout=subprocess.PIPE, check=False,
    )
    if diff.returncode != 1:
        raise RuntimeError("expected exactly the baseline cache-budget change")
    (root / "baseline-cache-only.patch").write_bytes(diff.stdout)
    bins = root / "bin"
    bins.mkdir()
    shutil.copyfile(args.candidate_job, bins / "candidate-job")
    (bins / "candidate-job").chmod(0o755)
    receipt = {
        "head": head,
        "head_tree": git(repo, "rev-parse", "HEAD^{tree}").decode().strip(),
        "candidate_patch_sha256": sha(patch),
        "candidate_tracked_hashes_sha256": sha((root / "candidate-tracked-sha256.json").read_bytes()),
        "baseline_delta_sha256": sha(diff.stdout),
        "example_sha256": sha(example.read_bytes()),
        "cache_proof_example_sha256": sha(proof.read_bytes()),
        "candidate_job_sha256": sha((bins / "candidate-job").read_bytes()),
        "candidate_job_from": str(args.candidate_job.resolve()),
        "binary_source_attestation": "coordinator built all-features candidate from this tracked production tree before freezing; verify source hashes against final commit",
        "candidate_signal_sha256": sha((root / "candidate/apps/job/src/signals.rs").read_bytes()),
        "baseline_signal_sha256": sha(signal.read_bytes()),
        "baseline_change": "SignalModuleCache::default max_entries and max_source_bytes are zero; compile/execute path, dependencies, fresh Store/fuel/trap behavior unchanged",
    }
    (root / "source-identity.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
