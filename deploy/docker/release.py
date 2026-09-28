#!/usr/bin/env python3
"""Select merged versions and publish their tested image/deployment bundle."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tarfile
import tempfile

import codex

from manage import BUNDLE, BUNDLE_FILES, DATABASE_IMAGE, REPOSITORY, run, validate_manifest, version, version_precedence

CI_PATHS = {
    ".github/workflows/ci.yml",
    ".github/workflows/web.yml",
    ".github/workflows/native-runtime.yml",
    ".github/workflows/polymarket-history.yml",
    ".github/workflows/container.yml",
}
ASSETS = {"release.json", "quazonai-deploy.tar.gz"}
CODEX_REPOSITORY = "ghcr.io/zhengui666/quazonai-codex"


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
        supported = subprocess.run(["git", "cat-file", "-e", f"{revision}:deploy/docker/runtime.sh"],
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


def bundle(tag: str, revision: str, image: str, destination: Path, *,
           runtime_image: str, codex_version: str, codex_image: str, published: bool = False) -> dict:
    release = validate_manifest({
        "schema_version": 2, "version": tag, "revision": revision,
        "image": image, "database_image": DATABASE_IMAGE,
        "runtime_image": runtime_image, "codex_version": codex_version, "codex_image": codex_image,
    }, published=published)
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "release.json").write_text(json.dumps(release, indent=2) + "\n")
    with tarfile.open(destination / "quazonai-deploy.tar.gz", "w:gz") as archive:
        for name in sorted(BUNDLE_FILES):
            source = destination / name if name == "release.json" else BUNDLE / name
            archive.add(source, arcname=name, recursive=False)
    return release


def publish(assets: Path) -> None:
    release = validate_manifest(json.loads((assets / "release.json").read_text()), published=True)
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
        f"Scientific Runtime: `{release['runtime_image']}`\n\n"
        f"Codex {release['codex_version']}: `{release['codex_image']}`\n\n"
        "Linux x86_64. The application image includes the production frontend, Rust API and Caddy. "
        "The deployment bundle starts PostgreSQL/PGMQ and the application on a Compose network, and installs "
        "the same-image Worker in the owner's systemd user manager. The image also supplies the matching Runtime gateway. "
        "Codex and scientific jobs use the published GHCR images above. Configure native catalogs and account access after installation.\n\n"
        "Extract `quazonai-deploy.tar.gz` into an empty directory and read its README. Run `bash deploy.sh` "
        "for a new installation; run the installed `update.sh VERSION` for an existing installation. "
        "For the first migration from a version-1 deployment bundle, use the new extracted bundle's "
        "`python3 manage.py apply-update --directory EXISTING_INSTALLATION` instead; its old updater cannot unpack this bundle. "
        "Application and scientific Runtime versions use explicit release tags. Private GHCR packages require Docker login.\n"
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


def docker_configuration() -> dict:
    endpoint = os.environ.get("DOCKER_HOST") or run(
        ["docker", "context", "inspect", "--format", "{{.Endpoints.docker.Host}}"], capture=True)
    if not endpoint.startswith("unix://"):
        raise ValueError("Image verification requires the local Docker daemon.")
    return {"docker_socket": endpoint.removeprefix("unix://"), "uid": os.getuid(), "gid": os.getgid()}


def build_codex(target: str, image: str) -> str:
    # CI-only producer. Neither this file nor the Dockerfile enters the deploy bundle.
    target = codex.exact_version(target)
    with tempfile.TemporaryDirectory(prefix="quazonai-codex-context-") as temporary:
        run(["docker", "build", "--platform", "linux/amd64", "--build-arg", "CODEX_VERSION=" + target,
             "--file", str(BUNDLE / "Codex.Dockerfile"), "--tag", image, temporary])
    candidate = run(["docker", "image", "inspect", "--format", "{{.Id}}", image], capture=True)
    codex.verify_candidate(docker_configuration(), target, candidate)
    return candidate


def push_image(image: str, repository: str, tag: str) -> str:
    identity = run(["docker", "image", "inspect", "--format", "{{.Id}}", image], capture=True)
    reference = repository + ":" + tag
    run(["docker", "tag", image, reference])
    run(["docker", "push", reference])
    digests = json.loads(run(["docker", "image", "inspect", "--format", "{{json .RepoDigests}}", reference], capture=True))
    matches = [value for value in digests if value.startswith(repository + "@sha256:")]
    if len(matches) != 1:
        raise ValueError("Registry push did not identify one immutable image digest.")
    run(["docker", "pull", matches[0]])
    if run(["docker", "image", "inspect", "--format", "{{.Id}}", matches[0]], capture=True) != identity:
        raise ValueError("Registry roundtrip changed the tested image identity.")
    return matches[0]


def published_codex_image(target: str) -> str | None:
    # The app and standalone publishers share the workflow concurrency group.
    # Reuse an existing exact version even when a local rebuild has a new ID.
    reference = CODEX_REPOSITORY + ":" + codex.exact_version(target)
    result = subprocess.run(["docker", "pull", reference], capture_output=True, text=True, check=False)
    if result.returncode != 0:
        if any(message in result.stderr.lower() for message in ("manifest unknown", "no such manifest", "name unknown")):
            return None
        raise ValueError("Could not inspect the published Codex version; no image was pushed.")
    metadata = json.loads(run(["docker", "image", "inspect", reference], capture=True))[0]
    digests = metadata.get("RepoDigests") or []
    matches = [value for value in digests if isinstance(value, str)
               and re.fullmatch(re.escape(CODEX_REPOSITORY) + r"@sha256:[0-9a-f]{64}", value)]
    if (len(matches) != 1 or metadata.get("Os") != "linux" or metadata.get("Architecture") != "amd64"
            or metadata.get("Config", {}).get("Labels", {}).get("org.opencontainers.image.version") != target):
        raise ValueError("Published Codex version has incompatible metadata; it was not overwritten.")
    codex.verify_candidate(docker_configuration(), target, matches[0])
    return matches[0]


def advance_codex_latest(image: str, target: str) -> None:
    target = codex.exact_version(target)
    if "-" in target:
        return
    reference = CODEX_REPOSITORY + ":latest"
    result = subprocess.run(["docker", "pull", reference], capture_output=True, text=True, check=False)
    if result.returncode == 0:
        previous = run(["docker", "image", "inspect", "--format",
                        '{{index .Config.Labels "org.opencontainers.image.version"}}', reference], capture=True)
        if version_precedence("v" + codex.exact_version(previous)) >= version_precedence("v" + target):
            return
    elif not any(message in result.stderr.lower() for message in ("manifest unknown", "no such manifest", "name unknown")):
        raise ValueError("Could not inspect the published Codex latest version.")
    run(["docker", "pull", image])
    push_image(image, CODEX_REPOSITORY, "latest")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["select", "verify", "bundle", "publish", "build-codex", "push-images", "publish-codex", "codex-latest"])
    parser.add_argument("--version")
    parser.add_argument("--revision")
    parser.add_argument("--image")
    parser.add_argument("--runtime-image")
    parser.add_argument("--codex-image")
    parser.add_argument("--codex-version")
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
    elif args.command == "build-codex":
        print(build_codex(args.codex_version, args.image))
    elif args.command == "codex-latest":
        selected = validate_manifest(json.loads((args.output / "release.json").read_text()), published=True)
        advance_codex_latest(selected["codex_image"], selected["codex_version"])
    elif args.command == "publish-codex":
        target = codex.exact_version(args.codex_version)
        codex.verify_candidate(docker_configuration(), target, args.image)
        digest = published_codex_image(target)
        if digest is None:
            digest = push_image(args.image, CODEX_REPOSITORY, target)
        advance_codex_latest(digest, target)
        print(digest)
    elif args.command == "push-images":
        verify(args.version, args.revision)
        images = (("image", args.image, "quazonai"),
                  ("runtime_image", args.runtime_image, "quazonai-runtime"))
        # Complete all candidate checks before mutating any registry reference.
        for _, image, _ in images:
            labels = json.loads(run(["docker", "image", "inspect", "--format",
                                     "{{json .Config.Labels}}", image], capture=True))
            if (labels.get("org.opencontainers.image.revision") != args.revision
                    or labels.get("org.opencontainers.image.version") != args.version):
                raise ValueError("The tested image does not match the release version and source.")
        target = codex.exact_version(args.codex_version)
        codex.verify_candidate(docker_configuration(), target, args.codex_image)
        codex_digest = published_codex_image(target)
        references = {field: push_image(image, "ghcr.io/zhengui666/" + repository, args.version)
                      for field, image, repository in images}
        if codex_digest is None:
            codex_digest = push_image(args.codex_image, CODEX_REPOSITORY, target)
        bundle(args.version, args.revision, references["image"], args.output,
               runtime_image=references["runtime_image"], codex_version=target,
               codex_image=codex_digest, published=True)
    elif args.command == "bundle":
        bundle(args.version, args.revision, args.image, args.output,
               runtime_image=args.runtime_image, codex_version=args.codex_version,
               codex_image=args.codex_image, published=True)
    else:
        publish(args.output)


if __name__ == "__main__":
    main()
