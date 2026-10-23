#!/usr/bin/env python3
"""Select merged versions and publish their tested image/deployment bundle."""
from __future__ import annotations

import argparse
from datetime import datetime
import gzip
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time


from release_support import BUNDLE, BUNDLE_FILES, DATABASE_IMAGE, REPOSITORY, run, validate_manifest, version, version_precedence

# Exact-push core CI owns the tested native release artifacts. Publication does
# not rerun Rust regression for dev or rebuild those binaries.
CI_PATHS = {".github/workflows/ci.yml"}
DEV_TAG = re.compile(r"v[0-9]+\.[0-9]+\.[0-9]+-dev\.[0-9]{14}\.[1-9][0-9]*")


def api(endpoint: str, *, pages: bool = False):
    args = ["gh", "api", f"repos/{REPOSITORY}/{endpoint}"]
    if pages:
        args += ["--paginate", "--slurp"]
    return json.loads(run(args, capture=True))


def releases() -> dict:
    return {item["tag_name"]: item for page in api("releases?per_page=100", pages=True) for item in page}


def asset_package():
    # smoke.py reuses bundle() without shipping this CI-only producer to users.
    sys.path.insert(0, str(BUNDLE.parent))
    import package
    return package


def ci_runs(revision: str) -> dict:
    latest = {}
    for page in api(f"actions/runs?head_sha={revision}&per_page=100", pages=True):
        for item in page["workflow_runs"]:
            path = item["path"]
            if path in CI_PATHS and item["head_sha"] == revision and item.get("event") == "push":
                current = latest.get(path)
                if current is None or item["id"] > current["id"]:
                    latest[path] = item
    return latest


def ci_ready(revision: str) -> bool:
    latest = ci_runs(revision)
    return all(path in latest and latest[path]["status"] == "completed"
               and latest[path]["conclusion"] == "success" for path in CI_PATHS)


def ci_run_id(revision: str) -> int:
    runs = ci_runs(revision)
    if not all(path in runs and runs[path]["status"] == "completed" and runs[path]["conclusion"] == "success" for path in CI_PATHS):
        raise ValueError("Successful exact-push core CI is required; rerun CI if its artifacts expired.")
    return runs[".github/workflows/ci.yml"]["id"]


def tag_revision(tag: str) -> str:
    return run(["git", "rev-parse", "--verify", f"refs/tags/{version(tag)}^{{commit}}"], capture=True)


def is_merged(revision: str) -> bool:
    branch = os.environ.get("RELEASE_BRANCH", "main")
    if branch not in {"main", "dev"}:
        raise ValueError("Release sources must be merged into main or dev.")
    result = subprocess.run(["git", "merge-base", "--is-ancestor", revision, f"refs/remotes/origin/{branch}"], check=False)
    if result.returncode not in (0, 1):
        raise ValueError("Could not establish the tag's ancestry.")
    return result.returncode == 0


def dev_tag(revision: str, run_id: str) -> str:
    if not re.fullmatch(r"[0-9a-f]{40}", revision) or not re.fullmatch(r"[1-9][0-9]*", run_id):
        raise ValueError("Expected the exact push SHA and Actions run ID.")
    if os.environ.get("RELEASE_BRANCH") != "dev" or not is_merged(revision):
        raise ValueError("Automatic development releases must belong to dev.")
    execution = api(f"actions/runs/{run_id}")
    if execution["head_sha"] != revision or execution["head_branch"] != "dev" or execution["event"] != "push":
        raise ValueError("The dev tag must identify the triggering dev push.")
    # Run creation time is stable across retries, unlike the runner's clock.
    timestamp = datetime.fromisoformat(execution["created_at"].replace("Z", "+00:00")).strftime("%Y%m%d%H%M%S")
    cargo = run(["git", "show", f"{revision}:Cargo.toml"], capture=True)
    core = re.search(r'^version = "([0-9]+\.[0-9]+\.[0-9]+)(?:-[^"]+)?"$', cargo, re.MULTILINE)
    if core is None:
        raise ValueError("Missing workspace version.")
    tag = version(f"v{core[1]}-dev.{timestamp}.{run_id}")
    exists = subprocess.run(["git", "show-ref", "--verify", "--quiet", f"refs/tags/{tag}"], check=False)
    if exists.returncode == 0:
        if tag_revision(tag) != revision:
            raise ValueError("The development tag already identifies another source; it was not moved.")
    elif exists.returncode == 1:
        created = json.loads(run(["gh", "api", "--method", "POST", f"repos/{REPOSITORY}/git/refs",
                                  "-f", f"ref=refs/tags/{tag}", "-f", f"sha={revision}"], capture=True))
        if created["object"]["sha"] != revision:
            raise ValueError("GitHub did not confirm the development tag source.")
    else:
        raise ValueError("Could not inspect the development tag.")
    return tag


def wait_ci(revision: str) -> int:
    deadline = time.monotonic() + 115 * 60
    while time.monotonic() < deadline:
        latest = ci_runs(revision)
        failed = [path for path, item in latest.items()
                  if item["status"] == "completed" and item["conclusion"] != "success"]
        if failed:
            raise ValueError(f"Exact-source checks failed; fix/rerun those checks, then retry this release: {failed}")
        if all(path in latest and latest[path]["status"] == "completed" for path in CI_PATHS):
            # Preserve the run from this verified snapshot. A second list read
            # can be older; the build still independently revalidates ci-run.
            return latest[".github/workflows/ci.yml"]["id"]
        print(f"Waiting for exact-source CI: {revision}", flush=True)
        time.sleep(30)
    raise TimeoutError("Exact-source CI did not complete; retry the release after the checks pass.")


def completed_release(release: dict, revision: str) -> bool:
    if release["target_commitish"] != revision:
        raise ValueError(f'Release {release["tag_name"]} does not identify the tagged source SHA; it was not overwritten.')
    if release["draft"]:
        return False
    uploaded = {item["name"] for item in release["assets"]
                if item.get("state") == "uploaded" and item.get("size", 0) > 0}
    if not asset_package().REQUIRED_ASSETS.issubset(uploaded):
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
        if DEV_TAG.fullmatch(tag) and os.environ.get("RELEASE_BRANCH", "main") != "dev":
            if requested:
                raise ValueError("Retry timestamped dev releases through their original Dev release run.")
            continue
        revision = tag_revision(tag)
        if not is_merged(revision):
            print(f"Not merged into main yet: {tag}", file=sys.stderr)
            continue
        # Do not retroactively package historical versions predating this feature.
        supported = subprocess.run(["git", "cat-file", "-e", f"{revision}:deploy/install.sh"],
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
    if (os.environ.get("RELEASE_BRANCH", "main") == "dev") != bool(DEV_TAG.fullmatch(tag)):
        raise ValueError("Timestamped development tags must use the dev publisher; other tags must use main.")
    if tag_revision(tag) != revision or not is_merged(revision):
        raise ValueError("Tag moved or source is not contained in the release branch.")
    if run(["git", "rev-parse", "HEAD"], capture=True) != revision:
        raise ValueError("The build checkout does not match the tagged source.")
    if not ci_ready(revision):
        raise ValueError("The exact tagged source does not have successful applicable CI.")


def bundle(tag: str, revision: str, image: str, destination: Path, *,
           runtime_image: str, published: bool = False) -> dict:
    release = validate_manifest({
        "schema_version": 4, "version": tag, "revision": revision,
        "image": image, "database_image": DATABASE_IMAGE,
        "runtime_image": runtime_image,
    }, published=published)
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "release.json").write_text(json.dumps(release, indent=2) + "\n")
    with tarfile.open(destination / "quazonai-deploy.tar.gz", "w:gz") as archive:
        for name in sorted(BUNDLE_FILES):
            source = destination / name if name == "release.json" else BUNDLE / name
            if name == "README.md":
                content = source.read_text().replace("@QUAZONAI_VERSION@", tag).encode()
                member = tarfile.TarInfo(name)
                member.size = len(content)
                archive.addfile(member, io.BytesIO(content))
            else:
                archive.add(source, arcname=name, recursive=False)
    return release


def publish(assets: Path) -> None:
    package = asset_package()
    package.verify(assets)
    release = validate_manifest(json.loads((assets / "release.json").read_text()), published=True)
    tag, revision = release["version"], release["revision"]
    verify(tag, revision)
    existing = releases().get(tag)
    if existing and completed_release(existing, revision):
        print(f"Already published: {tag}")
        return
    notes = (assets / "README.md").read_text()
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
             *(str(assets / name) for name in sorted(package.REQUIRED_ASSETS))])
        # Read back the actual uploaded manifest before advertising the release.
        uploaded = json.loads(run(["gh", "release", "download", tag, "--repo", REPOSITORY,
                                   "--pattern", "release.json", "--output", "-"], capture=True))
        if uploaded != release:
            raise ValueError("Uploaded release manifest does not match the tested image.")
        uploaded_assets = {item["name"]: item for item in releases()[tag]["assets"]}
        for name in package.REQUIRED_ASSETS:
            item = uploaded_assets.get(name)
            if (item is None or item.get("state") != "uploaded"
                    or item["size"] != (assets / name).stat().st_size):
                raise ValueError(f"GitHub did not confirm the complete uploaded asset: {name}")
        run(["gh", "release", "edit", tag, "--repo", REPOSITORY, "--draft=false", "--latest=false",
             f"--prerelease={'true' if '-' in tag else 'false'}", "--notes-file", str(notes_file)])
    actual = releases().get(tag)
    if not actual or not completed_release(actual, revision):
        raise ValueError("GitHub did not confirm a complete published release.")
    print(actual["html_url"])


def archive_images(assets: Path) -> None:
    release = validate_manifest(json.loads((assets / "release.json").read_text()), published=True)
    for component, field in (("application", "image"), ("runtime", "runtime_image"), ("database", "database_image")):
        reference = release[field]
        run(["docker", "pull", "--platform", "linux/amd64", reference])
        identity = run(["docker", "image", "inspect", "--format", "{{.Id}}", reference], capture=True)
        local = f"quazonai-bundle/{component}:{release['version']}"
        run(["docker", "tag", reference, local])
        output = assets / f"quazonai-image-{component}.tar.gz"
        with output.open("wb") as destination:
            with subprocess.Popen(["docker", "image", "save", local], stdout=subprocess.PIPE) as process:
                assert process.stdout is not None
                with gzip.GzipFile(fileobj=destination, mode="wb", compresslevel=1, mtime=0) as compressed:
                    shutil.copyfileobj(process.stdout, compressed)
                if process.wait() != 0:
                    raise ValueError(f"Could not archive the {component} image.")
        if output.stat().st_size >= 2_000_000_000:
            raise ValueError(f"The {component} archive exceeds GitHub's per-asset limit.")
        # This job owns a fresh disposable Docker daemon. Roundtrip the actual
        # shipped archive; a successful registry pull alone does not test it.
        run(["docker", "image", "rm", local, reference])
        run(["docker", "image", "load", "--input", str(output)])
        if run(["docker", "image", "inspect", "--format", "{{.Id}}", local], capture=True) != identity:
            raise ValueError(f"The {component} archive changed the image identity.")
        run(["docker", "image", "rm", local])


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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["select", "verify", "bundle", "publish", "push-images", "dev-tag", "wait-ci", "ci-run", "archive-images"])
    parser.add_argument("--version")
    parser.add_argument("--revision")
    parser.add_argument("--image")
    parser.add_argument("--runtime-image")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--run-id")
    args = parser.parse_args()
    if args.command == "dev-tag":
        tag = dev_tag(args.revision, args.run_id)
        print(tag)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                stream.write(f"version={tag}\n")
    elif args.command == "wait-ci":
        run_id = wait_ci(args.revision)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                stream.write(f"ci_run_id={run_id}\n")
    elif args.command == "ci-run":
        run_id = ci_run_id(args.revision)
        print(run_id)
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                stream.write(f"ci_run_id={run_id}\n")
    elif args.command == "archive-images":
        archive_images(args.output)
    elif args.command == "select":
        selected = select(args.version or os.environ.get("REQUESTED_TAG") or None)
        values = {"matrix": json.dumps({"include": selected}, separators=(",", ":")), "any": str(bool(selected)).lower()}
        print(json.dumps(values))
        if os.environ.get("GITHUB_OUTPUT"):
            with open(os.environ["GITHUB_OUTPUT"], "a") as stream:
                for key, value in values.items():
                    stream.write(f"{key}={value}\n")
    elif args.command == "verify":
        verify(args.version, args.revision)
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
        references = {field: push_image(image, "ghcr.io/zhengui666/" + repository, args.version)
                      for field, image, repository in images}
        bundle(args.version, args.revision, references["image"], args.output,
               runtime_image=references["runtime_image"], published=True)
    elif args.command == "bundle":
        bundle(args.version, args.revision, args.image, args.output,
               runtime_image=args.runtime_image, published=True)
    else:
        publish(args.output)


if __name__ == "__main__":
    main()

