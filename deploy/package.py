#!/usr/bin/env python3
"""Build and verify complete prebuilt release assets; Python standard library only."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
REPOSITORY = "zhengui666/QuaZonai"
CLI_ARCHIVES = {
    "linux-x86_64": "quazonai-cli-linux-x86_64.tar.gz",
    "macos-x86_64": "quazonai-cli-macos-x86_64.tar.gz",
    "macos-aarch64": "quazonai-cli-macos-aarch64.tar.gz",
    "windows-x86_64": "quazonai-cli-windows-x86_64.zip",
}
IMAGE_ARCHIVES = {name: f"quazonai-image-{name}.tar.gz"
                  for name in ("application", "runtime", "codex", "database")}
NOTICES = {"LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md"}
REQUIRED_ASSETS = frozenset({"release.json", "quazonai-deploy.tar.gz", "README.md",
                            "install.sh", "install.ps1", "SHA256SUMS"}
                           | set(CLI_ARCHIVES.values()) | set(IMAGE_ARCHIVES.values()))
MAX_ASSET = 2 * 1024**3 - 1  # GitHub requires each Release asset to be smaller than 2 GiB.
MAX_CLI = 512 * 1024**2
MAX_IMAGE = 64 * 1024**3
MARKER = "@QUAZONAI_VERSION@"


def identity(version: str, revision: str) -> None:
    match = re.fullmatch(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?", version)
    if not match or len(version) > 128 or (match[4] and any(
            part.isdigit() and len(part) > 1 and part.startswith("0") for part in match[4].split("."))):
        raise ValueError("Expected an exact vMAJOR.MINOR.PATCH[-prerelease] tag.")
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Expected the exact 40-character source revision.")


def regular(path: Path, maximum: int = MAX_ASSET) -> None:
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= maximum:
        raise ValueError(f"Missing, empty, linked or oversized asset: {path.name}")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def safe_name(name: str) -> bool:
    path = PurePosixPath(name)
    return bool(name) and not path.is_absolute() and ".." not in path.parts and "\\" not in name


def cli_members(platform: str) -> set[str]:
    return NOTICES | {"quazonai.exe" if platform.startswith("windows-") else "quazonai"}


def check_cli_archive(path: Path, platform: str) -> None:
    regular(path, MAX_CLI)
    expected = cli_members(platform)
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
            if (len(members) != len(expected) or {member.filename for member in members} != expected
                    or any(member.is_dir() or member.flag_bits & 1
                           or stat.S_ISLNK(member.external_attr >> 16)
                           or not 0 < member.file_size <= MAX_CLI for member in members)
                    or sum(member.file_size for member in members) > MAX_CLI):
                raise ValueError("CLI ZIP must contain only the binary and license notices.")
            if archive.testzip() is not None:
                raise ValueError("CLI ZIP checksum failed.")
    else:
        with tarfile.open(path, "r|gz") as archive:
            names = set()
            total = 0
            for member in archive:
                total += member.size
                if (member.name not in expected or member.name in names or not member.isfile()
                        or member.size <= 0 or total > MAX_CLI):
                    raise ValueError("CLI archive must contain only the binary and license notices.")
                names.add(member.name)
                if member.name == "quazonai" and not member.mode & 0o111:
                    raise ValueError("Packaged CLI must be executable.")
            if names != expected:
                raise ValueError("CLI archive must contain only the binary and license notices.")


def pack_cli(binary: Path, platform: str, destination: Path, version: str, revision: str) -> Path:
    identity(version, revision)
    regular(binary, MAX_CLI)
    destination.mkdir(parents=True, exist_ok=True)
    output = destination / CLI_ARCHIVES[platform]
    sources = {name: ROOT / name for name in NOTICES}
    sources["quazonai.exe" if platform.startswith("windows-") else "quazonai"] = binary
    for source in sources.values():
        regular(source, MAX_CLI)
    if output.suffix == ".zip":
        with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, source in sorted(sources.items()):
                archive.write(source, arcname=name)
    else:
        with tarfile.open(output, "w:gz") as archive:
            for name, source in sorted(sources.items()):
                info = archive.gettarinfo(str(source), arcname=name)
                info.uid = info.gid = info.mtime = 0
                info.uname = info.gname = ""
                info.mode = 0o755 if name == "quazonai" else 0o644
                with source.open("rb") as stream:
                    archive.addfile(info, stream)
    check_cli_archive(output, platform)
    return output


def smoke_cli(path: Path, platform: str, version: str) -> None:
    check_cli_archive(path, platform)
    # Extraction accepts only the exact regular files checked above, never links or paths.
    with tempfile.TemporaryDirectory(prefix="quazonai-cli-check-") as temporary:
        root = Path(temporary)
        if path.suffix == ".zip":
            with zipfile.ZipFile(path) as archive:
                archive.extractall(root)
        else:
            with tarfile.open(path, "r:gz") as archive:
                archive.extractall(root)
        binary = root / ("quazonai.exe" if platform.startswith("windows-") else "quazonai")
        def run(*args):
            return subprocess.run([str(binary), *args], check=True, capture_output=True, text=True).stdout.strip()
        if run("--version") != f"quazonai {version}":
            raise ValueError("Packaged CLI version differs from the requested release tag.")
        if "login" not in run("client", "--help"):
            raise ValueError("Packaged CLI has no native client commands.")
        if "ArtifactCreate" not in run("openapi", "--list-schemas"):
            raise ValueError("Packaged CLI has no offline contract catalog.")
        json.loads(run("openapi", "--schema", "ArtifactCreate"))


def check_tar(path: Path, *, manifest: dict | None = None) -> None:
    """Inspect headers and small metadata while streaming potentially large image layers."""
    regular(path)
    files = set()
    regular_files = set()
    total = 0
    metadata = None
    readme_text = None
    maximum = MAX_CLI if manifest is not None else MAX_IMAGE
    metadata_name = "release.json" if manifest is not None else "manifest.json"
    with tarfile.open(path, "r|gz") as archive:
        for member in archive:
            if (not safe_name(member.name) or member.name in files
                    or not (member.isfile() or member.isdir()) or member.size < 0):
                raise ValueError(f"Unsafe archive member in {path.name}: {member.name}")
            files.add(member.name)
            if member.isfile():
                regular_files.add(member.name)
            total += member.size
            if total > maximum or len(files) > 10000:
                raise ValueError(f"Archive expands beyond its release limit: {path.name}")
            if member.name == metadata_name:
                if not member.isfile() or member.size > 4 * 1024**2:
                    raise ValueError(f"Invalid archive manifest: {path.name}")
                metadata = json.load(archive.extractfile(member))
            if manifest is not None and member.name == "README.md":
                if not member.isfile() or member.size > 4 * 1024**2:
                    raise ValueError("Invalid deployment guide.")
                readme_text = archive.extractfile(member).read().decode("utf-8")
    if manifest is not None:
        if metadata != manifest:
            raise ValueError("Deployment archive must contain the exact release.json.")
        if not readme_text or MARKER in readme_text or manifest["version"] not in readme_text:
            raise ValueError("Deployment guide must be pinned to the exact release tag.")
    elif (not isinstance(metadata, list) or len(metadata) != 1
          or not isinstance(metadata[0], dict) or not metadata[0].get("Layers")
          or not isinstance(metadata[0]["Layers"], list)
          or any(not isinstance(name, str) or name not in regular_files
                 for name in [metadata[0].get("Config"), *metadata[0]["Layers"]])):
        raise ValueError(f"Image archive lacks its Docker config/layers: {path.name}")


def read_manifest(destination: Path) -> dict:
    regular(destination / "release.json", 4 * 1024**2)
    manifest = json.loads((destination / "release.json").read_text(encoding="utf-8"))
    identity(manifest["version"], manifest["revision"])
    return manifest


def readme(manifest: dict) -> str:
    version, revision = manifest["version"], manifest["revision"]
    base = f"https://github.com/{REPOSITORY}/releases/download/{version}"
    return f"""# QuaZonai {version}

Source revision: `{revision}`. Every command below installs or updates this exact
release using prebuilt binaries and images; no local compilation or image build.

## Install or update

Linux x86_64: install/update the Docker cluster and native CLI. Requires Bash,
curl, Python 3, rootful Docker with Compose v2, and a working systemd user manager.
Run as the installation owner; preserve the installation directory when updating.

```sh
curl -fsSL {base}/install.sh | bash
```

macOS Intel or Apple Silicon: install/update the native CLI for a remote QuaZonai
service (requires Bash, curl, Python 3; the Docker cluster runs on Linux x86_64).
The same CLI-only command also works on Linux x86_64:

```sh
curl -fsSL {base}/install.sh | bash -s -- --cli-only
```

Windows x86_64, PowerShell: install/update the native CLI for a remote service.

```powershell
& ([scriptblock]::Create((Invoke-WebRequest -UseBasicParsing '{base}/install.ps1').Content))
```

Check `quazonai --version` after installation; it must print `quazonai {version}`.
Use `quazonai client login --help` to connect to your service. First installation
still requires account setup and external scientific data configuration.

## Contents

The release includes four native CLI archives, application/scientific Runtime/
Codex/PostgreSQL images, the deployment bundle, manifest and installers. CLI
archives contain the binary and LICENSE, NOTICE and THIRD_PARTY_NOTICES.md.
`SHA256SUMS` verifies every downloadable asset except the checksum file itself.
Installers verify downloaded archives before installing or loading images.

Read the [deployment guide](https://github.com/{REPOSITORY}/blob/{version}/deploy/docker/README.md)
for prerequisites, custom directories, backup and recovery. For an existing
non-default installation, pass `--directory PATH` to the Linux installer.
"""


def prepare(destination: Path) -> None:
    manifest = read_manifest(destination)
    for name in set(CLI_ARCHIVES.values()) | set(IMAGE_ARCHIVES.values()) | {"quazonai-deploy.tar.gz"}:
        regular(destination / name)
    for name in ("install.sh", "install.ps1"):
        template = (ROOT / "deploy" / name).read_text(encoding="utf-8")
        if MARKER not in template:
            raise ValueError(f"Installer template has no release version marker: {name}")
        (destination / name).write_text(template.replace(MARKER, manifest["version"]), encoding="utf-8", newline="\n")
    (destination / "README.md").write_text(readme(manifest), encoding="utf-8", newline="\n")
    (destination / "SHA256SUMS").write_text("".join(
        f"{sha256(destination / name)}  {name}\n" for name in sorted(REQUIRED_ASSETS - {"SHA256SUMS"})), encoding="ascii")
    verify(destination)


def verify(destination: Path) -> dict:
    for name in REQUIRED_ASSETS:
        regular(destination / name)
    manifest = read_manifest(destination)
    sums = {}
    for line in (destination / "SHA256SUMS").read_text(encoding="ascii").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.-]+)", line)
        if not match or match[2] in sums:
            raise ValueError("Invalid or duplicated checksum entry.")
        sums[match[2]] = match[1]
    if set(sums) != REQUIRED_ASSETS - {"SHA256SUMS"}:
        raise ValueError("The checksum file must cover every required asset exactly once.")
    for name, digest in sums.items():
        if sha256(destination / name) != digest:
            raise ValueError(f"Release asset checksum failed: {name}")
    for platform, name in CLI_ARCHIVES.items():
        check_cli_archive(destination / name, platform)
    check_tar(destination / "quazonai-deploy.tar.gz", manifest=manifest)
    for name in IMAGE_ARCHIVES.values():
        check_tar(destination / name)
    for name in ("install.sh", "install.ps1", "README.md"):
        content = (destination / name).read_text(encoding="utf-8")
        if MARKER in content or manifest["version"] not in content:
            raise ValueError(f"Release instructions are not pinned to the tag: {name}")
    return manifest


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    pack = commands.add_parser("pack-cli")
    pack.add_argument("--binary", type=Path, required=True)
    pack.add_argument("--output", type=Path, required=True)
    pack.add_argument("--platform", choices=CLI_ARCHIVES, required=True)
    pack.add_argument("--version", required=True)
    pack.add_argument("--revision", required=True)
    smoke = commands.add_parser("smoke-cli")
    smoke.add_argument("--archive", type=Path, required=True)
    smoke.add_argument("--platform", choices=CLI_ARCHIVES, required=True)
    smoke.add_argument("--version", required=True)
    for name in ("prepare", "verify"):
        command = commands.add_parser(name)
        command.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "pack-cli":
        print(pack_cli(args.binary, args.platform, args.output, args.version, args.revision))
    elif args.command == "smoke-cli":
        smoke_cli(args.archive, args.platform, args.version)
    elif args.command == "prepare":
        prepare(args.output)
    else:
        verify(args.output)


if __name__ == "__main__":
    main()
