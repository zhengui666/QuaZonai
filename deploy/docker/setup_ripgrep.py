"""Install the fixed official CI ripgrep, without system package operations."""
import hashlib
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile


VERSION = "14.1.1"
RELEASE = f"ripgrep-{VERSION}-x86_64-unknown-linux-musl"
URL = f"https://github.com/BurntSushi/ripgrep/releases/download/{VERSION}/{RELEASE}.tar.gz"
# Verified from the official release's URL + ".sha256" on 2026-10-01.
SHA256 = "4cf9f2741e6c465ffdb7c26f38056a59e2a2544b51f7cc128ef28337eeae4d8e"
MAX_ARCHIVE_BYTES = 4 * 1024 * 1024
MAX_BINARY_BYTES = 16 * 1024 * 1024
DOWNLOAD_TIMEOUT_SECONDS = 65
VERSION_TIMEOUT_SECONDS = 5


def download(archive: Path) -> None:
    # Older curl versions do not enforce --max-filesize on chunked responses.
    version = subprocess.run(["curl", "--disable", "--version"], check=True,
                             capture_output=True, text=True, timeout=VERSION_TIMEOUT_SECONDS)
    match = re.match(r"curl (\d+)\.(\d+)\.(\d+)\b", version.stdout)
    if not match or tuple(map(int, match.groups())) < (8, 4, 0):
        raise ValueError("CI ripgrep setup requires curl >= 8.4.0 for bounded downloads")
    subprocess.run([
        "curl", "--disable", "--fail", "--silent", "--show-error", "--location",
        "--proto", "=https", "--proto-redir", "=https", "--max-redirs", "3",
        "--connect-timeout", "10", "--max-time", "60", "--retry", "0",
        "--max-filesize", str(MAX_ARCHIVE_BYTES), "--output", str(archive), URL,
    ], check=True, timeout=DOWNLOAD_TIMEOUT_SECONDS)


def extract_binary(archive: Path, binary: Path) -> None:
    if not 0 < archive.stat().st_size <= MAX_ARCHIVE_BYTES:
        raise ValueError("ripgrep archive exceeds the download byte budget or is empty")
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise ValueError("official ripgrep archive SHA256 mismatch")
    with tarfile.open(archive, "r:gz") as source:
        members = [entry for entry in source.getmembers() if entry.name == f"{RELEASE}/rg"]
        if len(members) != 1:
            raise ValueError("ripgrep archive must contain exactly one expected rg member")
        member = members[0]
        if (member.type not in (tarfile.REGTYPE, tarfile.AREGTYPE)
                or member.sparse is not None or not 0 < member.size <= MAX_BINARY_BYTES):
            raise ValueError("ripgrep archive rg member must be a bounded regular file")
        # Copy only bytes, never archive paths, links, permissions or ownership.
        with source.extractfile(member) as contents, binary.open("xb") as target:
            shutil.copyfileobj(contents, target)
    binary.chmod(0o755)


def install(runner_temp: Path, github_path: Path) -> Path:
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise ValueError("CI ripgrep setup supports Linux x86_64 only")
    root = runner_temp.resolve(strict=True)
    if not root.is_dir() or "\n" in str(root) or "\r" in str(root):
        raise ValueError("RUNNER_TEMP must be a directory safe for GITHUB_PATH")
    # Always use a fresh owned directory. A pre-existing PATH rg has no verified
    # version/provenance and is neither executed, overwritten nor reused.
    owned = Path(tempfile.mkdtemp(prefix="quazonai-ripgrep-", dir=root))
    try:
        archive = owned / "release.tar.gz"
        download(archive)
        binary = owned / "rg"
        extract_binary(archive, binary)
        archive.unlink()
        result = subprocess.run([str(binary), "--version"], check=True, capture_output=True,
                                text=True, timeout=VERSION_TIMEOUT_SECONDS)
        first_line = result.stdout.splitlines()[0] if result.stdout else ""
        if not re.fullmatch(r"ripgrep " + re.escape(VERSION) + r"(?: \(rev [0-9a-f]+\))?", first_line):
            raise ValueError("verified ripgrep binary returned an unexpected version")
        # Publish only after download, digest, member and executable checks pass.
        with github_path.open("a", encoding="utf-8") as output:
            output.write(str(owned) + "\n")
        print(f"Installed {first_line} from {URL} (SHA256 {SHA256})")
        return owned
    except BaseException:
        shutil.rmtree(owned)
        raise


def main() -> None:
    install(Path(os.environ["RUNNER_TEMP"]), Path(os.environ["GITHUB_PATH"]))


if __name__ == "__main__":
    try:
        main()
    except (KeyError, OSError, ValueError, tarfile.TarError, subprocess.SubprocessError) as error:
        print(f"CI ripgrep setup failed: {error}", file=sys.stderr)
        sys.exit(1)
