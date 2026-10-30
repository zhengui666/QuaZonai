#!/usr/bin/env bash
set -euo pipefail
directory=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
: "${CARGO:?Set CARGO to the Rust 1.99.1 cargo executable}"
: "${RUSTC:?Set RUSTC to the Rust 1.99.1 rustc executable}"
: "${CARGO_HOME:?Set CARGO_HOME to the dependency cache}"
: "${CARGO_TARGET_DIR:?Set an independent target directory}"

python3 - "$directory" <<'PY'
import hashlib
import json
from pathlib import Path
import sys
directory = Path(sys.argv[1])
repo = directory.parents[4]
paths = {
    "source_sha256": repo / "apps/job/src/signals.rs",
    "harness_sha256": directory / "src/main.rs",
    "lock_sha256": directory / "Cargo.lock",
}
(directory / "build-identity.json").write_text(json.dumps({
    key: hashlib.sha256(path.read_bytes()).hexdigest()
    for key, path in paths.items()
}, indent=2) + "\n")
PY
"$CARGO" build --release --locked --offline --manifest-path "$directory/Cargo.toml" -j 1 > "$directory/build.log" 2>&1
