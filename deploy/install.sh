#!/usr/bin/env bash
# Published with the exact release tag substituted by the release producer.
set -euo pipefail
umask 077

version='@QUAZONAI_VERSION@'
directory="$HOME/.local/share/quazonai"
bin_directory="$HOME/.local/bin"
cli_only=false
deployment_args=()
fail() { printf 'Installation failed: %s\n' "$*" >&2; exit 1; }
while [ "$#" -gt 0 ]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: bash install.sh [--cli-only] [--version TAG] [--bin-dir PATH] [--directory PATH]
                       [--port PORT] [--database-port PORT] [--codex-home PATH]
Installs this release's prebuilt CLI into ~/.local/bin. Linux x86_64 also
installs or updates the Docker stack; macOS installs only the CLI.
--cli-only skips the Linux stack. Other options configure the Linux stack;
existing installations retain their settings. No local build is performed.
HELP
      exit 0 ;;
    --cli-only) cli_only=true; shift ;;
    --version|--directory|--bin-dir|--port|--database-port|--codex-home)
      [ "$#" -ge 2 ] && [ -n "$2" ] || fail "Missing value for $1"
      case "$1" in
        --version) version=$2 ;;
        --directory) directory=$2 ;;
        --bin-dir) bin_directory=$2 ;;
        *) deployment_args+=("$1" "$2") ;;
      esac
      shift 2 ;;
    *) fail "Unknown option: $1" ;;
  esac
done
[[ ${#version} -le 128 && "$version" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]] || fail 'Expected a published vMAJOR.MINOR.PATCH[-prerelease] tag.'
if [[ "$version" == *-* ]]; then
  IFS=. read -r -a identifiers <<< "${version#*-}"
  for identifier in "${identifiers[@]}"; do
    [[ ! "$identifier" =~ ^0[0-9]+$ ]] || fail 'Numeric prerelease identifiers cannot have leading zeroes.'
  done
fi
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) platform=linux-x86_64 ;;
  Darwin/x86_64) platform=macos-x86_64; cli_only=true ;;
  Darwin/arm64) platform=macos-aarch64; cli_only=true ;;
  *) fail 'Supported hosts: Linux x86_64, macOS x86_64 or Apple Silicon. Use install.ps1 on Windows x86_64.' ;;
esac
if command -v sha256sum >/dev/null; then
  checksum=(sha256sum)
elif command -v shasum >/dev/null; then
  checksum=(shasum -a 256)
else
  fail 'sha256sum or shasum is required.'
fi
command -v curl >/dev/null || fail 'curl is required.'
command -v tar >/dev/null || fail 'tar is required.'
if ! "$cli_only"; then
  command -v python3 >/dev/null || fail 'The Linux Docker stack requires Python 3.10+.'
fi
work=$(mktemp -d)
staged=''
trap 'rm -rf "$work"; if [ -n "$staged" ]; then rm -f "$staged"; fi' EXIT
base="https://github.com/zhengui666/QuaZonai/releases/download/$version"
download() {
  curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 \
    --retry 3 --connect-timeout 15 --max-time 600 --output "$work/$1" "$base/$1"
}
download SHA256SUMS
verified_download() {
  local expected actual
  expected=$(awk -v name="$1" '$2 == name {print $1}' "$work/SHA256SUMS")
  [[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail "Missing or duplicate checksum for $1"
  download "$1"
  actual=$("${checksum[@]}" "$work/$1")
  [ "${actual%% *}" = "$expected" ] || fail "Checksum mismatch for $1"
}
archive="quazonai-cli-$platform.tar.gz"
verified_download "$archive"
members=$(tar -tzf "$work/$archive" | LC_ALL=C sort)
[ "$members" = "$(printf '%s\n' LICENSE NOTICE THIRD_PARTY_NOTICES.md quazonai | LC_ALL=C sort)" ] || fail 'Unexpected CLI archive files.'
mkdir "$work/cli"
# Extract only named file contents, never archive paths or symlink targets.
for name in quazonai LICENSE NOTICE THIRD_PARTY_NOTICES.md; do
  tar -xOzf "$work/$archive" "$name" > "$work/cli/$name"
done
chmod 755 "$work/cli/quazonai"
"$work/cli/quazonai" --version
mkdir -p -- "$bin_directory"
bin_directory=$(cd -- "$bin_directory" && pwd)
[ ! -d "$bin_directory/quazonai" ] || fail 'The CLI destination is a directory.'
staged=$(mktemp "$bin_directory/.quazonai.XXXXXX")
cp "$work/cli/quazonai" "$staged"
chmod 755 "$staged"
if ! "$cli_only"; then
  verified_download quazonai-deploy.tar.gz
  python3 - "$work" "$version" "$directory" "${deployment_args[@]}" <<'PY'
import json
from pathlib import Path
import subprocess
import sys
import tarfile

work, version, directory = sys.argv[1:4]
bundle = Path(work) / 'deployment'
bundle.mkdir()
with tarfile.open(Path(work) / 'quazonai-deploy.tar.gz', 'r:gz') as archive:
    members = archive.getmembers()
    if not members or len({item.name for item in members}) != len(members):
        raise ValueError('Empty or duplicate deployment bundle files.')
    for item in members:
        if not item.isfile() or Path(item.name).name != item.name or item.size > 512_000:
            raise ValueError('Invalid deployment bundle member.')
        with archive.extractfile(item) as source:
            (bundle / item.name).write_bytes(source.read())
if json.loads((bundle / 'release.json').read_text())['version'] != version:
    raise ValueError('Downloaded deployment bundle does not match the requested tag.')
root = Path(directory).expanduser().resolve()
installed = (root / 'installation.json').exists()
pending_file = root / 'pending.json'
pending = json.loads(pending_file.read_text()) if pending_file.exists() else None
if pending is not None and (not installed or pending.get('operation') not in ('install', 'update')):
    raise ValueError('Preserve and reconcile the interrupted installation before retrying.')
# A crash after installation.json but before pending.json/current needs deploy.
initial = not installed or (pending is not None and pending['operation'] == 'install') or (
    pending is None and not (root / 'current').is_symlink())
command = 'deploy' if initial else 'apply-update'
subprocess.run([sys.executable, '-B', str(bundle / 'manage.py'), command,
                '--directory', str(root), *sys.argv[4:]], check=True)
PY
fi
licenses="$HOME/.local/share/quazonai-cli/licenses/$version"
mkdir -p "$licenses"
cp "$work/cli/LICENSE" "$work/cli/NOTICE" "$work/cli/THIRD_PARTY_NOTICES.md" "$licenses/"
# Both paths are on the same filesystem; a failed download/deploy leaves the old CLI.
mv -f "$staged" "$bin_directory/quazonai"
staged=''
printf 'Installed QuaZonai CLI from %s: %s/quazonai\n' "$version" "$bin_directory"
case ":$PATH:" in
  *":$bin_directory:"*) ;;
  *) printf 'Add it to your shell PATH: export PATH=%q:"$PATH"\n' "$bin_directory" ;;
esac
