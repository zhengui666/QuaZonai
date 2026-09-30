"""Bounded, dependency-only cache for the opt-in native Job CI build."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import tomllib

RUST = "1.98.1"
PREFIX = "quazonai-native-job-default-release-v1"
MAX_BYTES = 3 * 1024**3
HEADROOM_BYTES = 1024**3
LAYOUT_ENV = ("CARGO_TARGET_DIR", "CARGO_BUILD_TARGET_DIR", "CARGO_BUILD_TARGET", "CARGO_BUILD_BUILD_DIR")
BUILD_ENV = ("CARGO_BUILD_", "CARGO_PROFILE_", "CARGO_TARGET_", "CARGO_ENCODED_RUSTFLAGS",
             "CARGO_INCREMENTAL", "RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC", "CC", "CXX", "AR",
             "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "PKG_CONFIG", "BINDGEN_EXTRA_CLANG_ARGS")


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def policy(env):
    dev_push = env.get("GITHUB_EVENT_NAME") == "push" and env.get("GITHUB_REF") == "refs/heads/dev"
    dev_pr = env.get("GITHUB_EVENT_NAME") == "pull_request" and env.get("GITHUB_BASE_REF") == "dev"
    same_repo = bool(env.get("GITHUB_REPOSITORY")) and env.get("CACHE_HEAD_REPOSITORY") == env["GITHUB_REPOSITORY"]
    return dev_push or dev_pr, dev_push or (dev_pr and same_repo)


def source_root(env):
    root = Path.cwd().resolve()
    workspace = Path(env["GITHUB_WORKSPACE"]).resolve()
    root.relative_to(workspace)
    # actions/cache interprets paths as patterns and accepts newline-separated paths.
    if any(char in str(root) for char in "\r\n!*?[]"):
        raise ValueError("source path cannot be represented as one literal cache path")
    if (root / "target").is_symlink() or (root / "target/release").is_symlink():
        raise ValueError("cache requires this checkout's ordinary target/release directory")
    return root


def config_files(root, env):
    directories = [parent / ".cargo" for parent in (root, *root.parents)]
    directories.append(Path(env.get("CARGO_HOME", str(Path.home() / ".cargo"))))
    return sorted({directory / name for directory in directories for name in ("config", "config.toml")
                   if (directory / name).is_file()})


def cache_key(root, env, compiler):
    required = ("RUNNER_OS", "RUNNER_ARCH", "ImageOS", "ImageVersion")
    if any(not env.get(name) for name in required):
        raise ValueError("runner OS, architecture and exact image identity are required")
    if not compiler or any(not value for value in compiler.values()):
        raise ValueError("observed compiler identities are required")
    if any(env.get(name) for name in LAYOUT_ENV):
        raise ValueError("custom Cargo output/target layouts are outside this pilot")
    paths = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
    paths.update(path for path in run("git", "ls-files", "-z", "--", "**/Cargo.toml").split("\0") if path)
    if len(paths) < 4:
        raise ValueError("workspace member manifests are required")
    files = {}
    for name in sorted(paths):
        path = root / name
        if path.is_symlink() or not path.is_file() or not path.stat().st_size:
            raise ValueError(f"missing or unsupported cache input: {name}")
        path.resolve().relative_to(root)
        files[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    for index, path in enumerate(config_files(root, env)):
        config = tomllib.loads(path.read_text())
        if any(config.get("build", {}).get(name) for name in ("target", "target-dir", "build-dir")):
            raise ValueError("custom Cargo output/target layouts are outside this pilot")
        files[f"cargo-config-{index}"] = hashlib.sha256(path.read_bytes()).hexdigest()
    identity = {"runner": {name: env[name] for name in required}, "rust": RUST,
                "compiler": compiler, "files": files,
                "cargo_home": str(Path(env.get("CARGO_HOME", str(Path.home() / ".cargo"))).resolve()),
                "environment": {name: value for name, value in env.items() if name.startswith(BUILD_ENV)},
                "build": ["cargo", "build", "--locked", "--release", "-p", "job"]}
    digest = hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    return f"{PREFIX}-{digest}"


def measure(root, phase):
    release = root / "target/release"
    size = int(run("du", "--summarize", "--bytes", str(release)).split()[0]) if release.is_dir() else 0
    free = int(run("df", "--block-size=1", "--output=avail", str(root)).splitlines()[-1])
    print(f"Native dependency cache {phase}: du_bytes={size} df_available_bytes={free}", flush=True)
    return size, free


def save_allowed(size, free):
    return 0 < size <= MAX_BYTES and free >= size + HEADROOM_BYTES


def output(**values):
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as handle:
        for name, value in values.items():
            handle.write(f"{name.replace('_', '-')}={str(value).lower() if isinstance(value, bool) else value}\n")


def prune(root, prime=False):
    if not (root / "target/release").exists():
        return
    # A fresh runner has no registry index even when target/release is restored.
    # Native dry-run resolves only metadata, without removing files or fetching
    # crate sources. Keep the actual pruning locked/offline, and bound priming.
    commands = []
    if prime:
        commands.append(["rustup", "run", RUST, "cargo", "clean", "--locked", "--release", "--workspace", "--dry-run"])
    commands.append(["rustup", "run", RUST, "cargo", "clean", "--locked", "--offline", "--release", "--workspace"])
    successful = True
    for command in commands:
        started = time.monotonic()
        try:
            result = subprocess.run(command, timeout=120)
            successful = successful and result.returncode == 0
        except (OSError, subprocess.TimeoutExpired):
            successful = False
        phase = "metadata dry-run" if "--dry-run" in command else "offline workspace clean"
        print(f"Native dependency cache {phase}: seconds={time.monotonic() - started:.3f}", flush=True)
    # Cargo owns package selection, including workspace build scripts/fingerprints.
    # Never reuse a restored workspace executable if metadata/pruning failed.
    if not successful:
        print("Native dependency cache: metadata/pruning failed; discard the disposable release tree", flush=True)
        shutil.rmtree(root / "target/release")


def main(command):
    env = os.environ
    try:
        root = source_root(env)
        if command == "prepare":
            enabled, can_save = policy(env)
            if not enabled:
                raise ValueError("only dev pushes and dev-targeted pull requests use this pilot")
            _, free = measure(root, "before restore")
            if free < 2 * MAX_BYTES + HEADROOM_BYTES:
                raise ValueError("insufficient headroom for cache download and extraction")
            compiler = {"rustc": run("rustup", "run", RUST, "rustc", "-vV"),
                        "cargo": run("rustup", "run", RUST, "cargo", "-V"),
                        "cc": run("cc", "--version"), "cxx": run("c++", "--version"),
                        "ld": run("ld", "--version")}
            key = cache_key(root, env, compiler)
            output(enabled=True, can_save=can_save, key=key, path=str(root / "target/release"))
        elif command == "prune":
            measure(root, "after restore")
            if (root / "target/release").exists() and env.get("CACHE_MATCHED_KEY") != env.get("CACHE_EXPECTED_KEY"):
                print("Native dependency cache: discard a partial or unconfirmed restore", flush=True)
                shutil.rmtree(root / "target/release")
            else:
                prune(root, prime=True)
            measure(root, "after restore pruning")
        elif command == "save-check":
            measure(root, "after image assembly")
            prune(root)
            size, free = measure(root, "before save")
            allowed = save_allowed(size, free)
            output(save=allowed)
            if not allowed:
                print("Native dependency cache: skip save (empty, over 3 GiB, or insufficient archive headroom)", flush=True)
        else:
            raise ValueError("unknown cache command")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        if command != "prepare":
            raise
        # Optional acceleration never turns an unsupported runner into a false pass.
        print(f"Native dependency cache: skip restore/save ({error})", flush=True)
        output(enabled=False, can_save=False)


if __name__ == "__main__":
    main(sys.argv[1])
