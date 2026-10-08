"""CI-only release packaging contracts; never included in the user deployment bundle."""
from __future__ import annotations

from pathlib import Path
import re
import subprocess

REPOSITORY = "zhengui666/QuaZonai"
DATABASE_IMAGE = "ghcr.io/pgmq/pg18-pgmq@sha256:bfb3537068ce453609744518ece92b178ac89dff53747d47ca6fab91c2fc66a6"
BUNDLE = Path(__file__).resolve().parent
BUNDLE_FILES = {"manage.sh", "json.awk", "deploy.sh", "update.sh", "compose.yaml", "release.json", "README.md",
                "codex.sh", "codex-update.sh", "codex-login.sh", "runtime.sh", "codex.apparmor", ".env.example"}
SEMVER = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?")


def version(value: str) -> str:
    match = SEMVER.fullmatch(value)
    if not match or len(value) > 128:
        raise ValueError("Expected vMAJOR.MINOR.PATCH[-prerelease], without build metadata.")
    if match[4] and any(x.isdigit() and len(x) > 1 and x[0] == "0" for x in match[4].split(".")):
        raise ValueError("Numeric prerelease identifiers cannot have leading zeroes.")
    return value

def version_precedence(value: str) -> tuple:
    match = SEMVER.fullmatch(version(value))
    assert match is not None
    core = tuple(int(match[index]) for index in (1, 2, 3))
    prerelease = match[4]
    # SemVer 2.0: numeric identifiers sort below text; a release sorts above
    # its prereleases. The existing parser already rejects build metadata.
    identifiers = tuple((0, int(part)) if part.isdigit() else (1, part)
                        for part in prerelease.split('.')) if prerelease else ()
    return (*core, 1 if prerelease is None else 0, identifiers)

def validate_manifest(value: dict, *, published: bool = False) -> dict:
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 2:
        raise ValueError("This installer requires a version-2 deployment bundle with prebuilt images.")
    version(value["version"])
    if not re.fullmatch(r"[0-9a-f]{40}", value["revision"]):
        raise ValueError("A release must identify its exact source revision.")
    for field, repository in (("image", "quazonai"), ("runtime_image", "quazonai-runtime"),
                              ("codex_image", "quazonai-codex")):
        reference = value.get(field)
        # The pre-publication suite uses Docker's content-addressed IDs for its
        # already-built images. Public bundles contain only GHCR digests.
        pattern = rf"ghcr\.io/zhengui666/{repository}@sha256:[0-9a-f]{{64}}"
        if not isinstance(reference, str) or not (
            re.fullmatch(pattern, reference)
            or (not published and re.fullmatch(r"sha256:[0-9a-f]{64}", reference))
        ):
            raise ValueError(f"{field} must identify the prebuilt {repository} image by digest.")
    codex_version = value.get("codex_version")
    if not isinstance(codex_version, str) or codex_version.startswith("v"):
        raise ValueError("codex_version must be an exact published version.")
    version("v" + codex_version)
    if value["database_image"] != DATABASE_IMAGE:
        raise ValueError("This installer requires the release's supported PostgreSQL/PGMQ image.")
    return value

def run(args: list[str], *, capture: bool = False, env=None, output=None) -> str:
    result = subprocess.run(args, check=True, env=env,
                            stdout=subprocess.PIPE if capture else output, text=capture)
    return result.stdout.strip() if capture else ""
