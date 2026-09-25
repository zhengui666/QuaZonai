#!/usr/bin/env python3
"""Select merged versions and publish their tested image/deployment bundle."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

from manage import BUNDLE, BUNDLE_FILES, DATABASE_IMAGE, REPOSITORY, run, validate_manifest, version

CI_PATHS = {
    ".github/workflows/ci.yml",
    ".github/workflows/web.yml",
    ".github/workflows/native-runtime.yml",
    ".github/workflows/polymarket-history.yml",
}
ASSETS = {"release.json", "quazonai-deploy.tar.gz"}


def api(endpoint: str, *, pages: bool = False):
    args = ["gh", "api", f"repos/{REPOSITORY}/{endpoint}"]
    if pages:
        args += ["--paginate", "--slurp"]
    return json.loads(run(args, capture=True))


def releases() -> dict:
    return {item["tag_name"]: item for page in api("releases?per_page=100", pages=True) for item in page}


def ci_ready(revision: str) -> bool:
    latest = {}
    for page in api(f"actions/runs?head_sha={revision}&per_page=100", pages=True):
        for item in page["workflow_runs"]:
            path = item["path"]
            if path in CI_PATHS and item["head_sha"] == revision:
                current = latest.get(path)
                if current is None or item["id"] > current["id"]:
                    latest[path] = item
    return all(path in latest and latest[path]["status"] == "completed"
               and latest[path]["conclusion"] == "success" for path in CI_PATHS)


def tag_revision(tag: str) -> str:
    return run(["git", "rev-parse", "--verify", f"refs/tags/{version(tag)}^{{commit}}"], capture=True)


def is_merged(revision: str) -> bool:
    result = subprocess.run(["git", "merge-base", "--is-ancestor", revision, "refs/remotes/origin/main"], check=False)
    if result.returncode not in (0, 1):
        raise ValueError("Could not establish the tag's ancestry.")
    return result.returncode == 0


def completed_release(release: dict, revision: str) -> bool:
    if release["target_commitish"] != revision:
        raise ValueError(f'Release {release["tag_name"]} does not identify the tagged source SHA; it was not overwritten.')
    if release["draft"]:
        return False
    if not ASSETS.issubset({x["name"] for x in release["assets"]}):
        raise ValueError(f'Published release {release["tag_name"]} has incomplete assets; it was not overwritten.')
    return True


def select(requested: str | None) -> list[dict]:
    existing = releases()
    # Scan on every event. A queued workflow superseded by another push cannot
    # lose a tag: the next scan still sees every reachable unpublished version.
    tags = [version(requested)] if requested else run(["git", "tag", "--list", "v*", "--sort=version:refname"], capture=True).splitlines()
    candidates = []
    for tag in tags:
        try:
            version(tag)
        except ValueError:
            continue
        revision = tag_revision(tag)
        if not is_merged(revision):
            print(f"Not merged into main yet: {tag}", file=sys.stderr)
            continue
        # Do not retroactively package historical versions predating this feature.
        supported = subprocess.run(["git", "cat-file", "-e", f"{revision}:deploy/docker/release.py"],
                                   check=False, stderr=subprocess.DEVNULL).returncode == 0
        if not supported:
            continue
        if tag in existing and completed_release(existing[tag], revision):
            continue
        if not ci_ready(revision):
            print(f"Waiting for successful exact-source CI: {tag} ({revision})", file=sys.stderr)
            continue
        candidates.append({"version": tag, "revision": revision})
    if len(candidates) > 256:
        raise ValueError("Too many unpublished versions for one Actions matrix; dispatch selected tags explicitly.")
    return candidates


def verify(tag: str, revision: str) -> None:
    if tag_revision(tag) != revision or not is_merged(revision):
        raise ValueError("Tag moved or source is not contained in main.")
    if run(["git", "rev-parse", "HEAD"], capture=True) != revision:
        raise ValueError("The build checkout does not match the tagged source.")
    if not ci_ready(revision):
        raise ValueError("The exact tagged source does not have successful applicable CI.")


def bundle(tag: str, revision: str, image: str, destination: Path) -> dict:
    release = validate_manifest({
        "schema_version": 1, "version": tag, "revision": revision,
        "image": image, "database_image": DATABASE_IMAGE,
    })
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "release.json").write_text(json.dumps(release, indent=2) + "\n")
    with tarfile.open(destination / "quazonai-deploy.tar.gz", "w:gz") as archive:
        for name in sorted(BUNDLE_FILES):
            source = destination / name if name == "release.json" else BUNDLE / name
            archive.add(source, arcname=name, recursive=False)
    return release


def publish(assets: Path) -> None:
    release = validate_manifest(json.loads((assets / "release.json").read_text()))
    tag, revision = release["version"], release["revision"]
    verify(tag, revision)
    existing = releases().get(tag)
    if existing and completed_release(existing, revision):
        print(f"Already published: {tag}")
        return
    notes = (
        f"Source revision: `{revision}`\n\n"
        f"Application: `{release['image']}`\n\n"
        f"Database: `{release['database_image']}`\n\n"
        "Linux x86_64. The application image includes the production frontend, Rust API, Caddy and native Codex. "
        "The deployment bundle starts PostgreSQL/PGMQ and the application on a Compose network, and installs "
        "the same-image Worker in the owner's systemd user manager. Scientific runtimes remain separately configured.\n\n"
        "Extract `quazonai-deploy.tar.gz` into an empty directory and read its README. Run `bash deploy.sh` "
        "for a new installation; run the installed `update.sh VERSION` for an existing installation. "
        "No floating latest image is published. Private GHCR packages require Docker login.\n"
    )
    with tempfile.TemporaryDirectory() as temporary:
        notes_file = Path(temporary) / "notes.md"
        notes_file.write_text(notes)
        if not existing:
            args = ["gh", "release", "create", tag, "--repo", REPOSITORY, "--verify-tag", "--target", revision,
                    "--title", f"QuaZonai {tag}", "--notes-file", str(notes_file), "--draft"]
            if "-" in tag:
                args.append("--prerelease")
            run(args)
        run(["gh", "release", "upload", tag, "--repo", REPOSITORY, "--clobber",
             str(assets / "release.json"), str(assets / "quazonai-deploy.tar.gz")])
        # Read back the actual uploaded manifest before advertising the release.
        uploaded = json.loads(run(["gh", "release", "download", tag, "--repo", REPOSITORY,
                                   "--pattern", "release.json", "--output", "-"], capture=True))
        if uploaded != release:
            raise ValueError("Uploaded release manifest does not match the tested image.")
        run(["gh", "release", "edit", tag, "--repo", REPOSITORY, "--draft=false", "--latest=false",
             f"--prerelease={'true' if '-' in tag else 'false'}", "--notes-file", str(notes_file)])
    actual = releases().get(tag)
    if not actual or not completed_release(actual, revision):
        raise ValueError("GitHub did not confirm a complete published release.")
    print(actual["html_url"])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["select", "verify", "bundle", "publish"])
    parser.add_argument("--version")
    parser.add_argument("--revision")
    parser.add_argument("--image")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.command == "select":
        selected = select(args.version or os.environ.get("REQUESTED_TAG") or None)
        values = {"matrix": json.dumps({"include": selected}, separators=(",", ":")), "any": str(bool(selected)).lower()}
        print(json.dumps(values))
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                for key, value in values.items():
                    stream.write(f"{key}={value}\n")
    elif args.command == "verify":
        verify(args.version, args.revision)
    elif args.command == "bundle":
        bundle(args.version, args.revision, args.image, args.output)
    else:
        publish(args.output)


if __name__ == "__main__":
    main()
