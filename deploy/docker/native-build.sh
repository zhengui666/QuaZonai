#!/usr/bin/env bash
# Build each required native format once; preserve standard BAR artifacts before
# Cargo publishes the separate high-precision executable at the same target path.
set -euo pipefail
profile=${1:-release}
output=${2:?output directory is required}
args=()
if [[ "$profile" == release ]]; then args+=(--release); elif [[ "$profile" != debug ]]; then exit 2; fi
cargo build --locked "${args[@]}" -p job \
  --features native-sandbox-test,catalog-prepare,hyperliquid-offline \
  --bin job --bin source-tools --bin catalog-prepare
mkdir -p "$output"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/job" "$output/job"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/source-tools" "$output/source-tools"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/catalog-prepare" "$output/catalog-prepare"
cargo build --locked "${args[@]}" -p job \
  --features market-history,catalog-prepare,hyperliquid-offline \
  --bin job --bin catalog-prepare --bin market-history
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/job" "$output/job-high-precision"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/catalog-prepare" "$output/catalog-prepare-high-precision"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/market-history" "$output/market-history"
node runtimes/native/build-identity.mjs "$output"
