#!/usr/bin/env bash
# Build each required native format once; preserve standard BAR artifacts before
# Cargo publishes the separate high-precision executable at the same target path.
set -euo pipefail
profile=${1:-release}
output=${2:?output directory is required}
args=()
if [[ "$profile" == release ]]; then args+=(--release); elif [[ "$profile" != debug ]]; then exit 2; fi
cargo build --locked "${args[@]}" -p job \
  --features native-paper-test,native-sandbox-test,catalog-prepare,polymarket-history,hyperliquid-offline \
  --bin job --bin source-tools --bin catalog-prepare --example browser_catalog_fixture
mkdir -p "$output"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/job" "$output/job"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/source-tools" "$output/source-tools"
# Core acceptance helpers use the identical profile/features, but are not shipped
# in either production image or the CLI distribution.
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/catalog-prepare" "$output/catalog-prepare"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/examples/browser_catalog_fixture" "$output/browser_catalog_fixture"
cargo build --locked "${args[@]}" -p job \
  --features market-history,catalog-prepare,hyperliquid-offline \
  --bin job --bin catalog-prepare --bin market-history
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/job" "$output/job-high-precision"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/catalog-prepare" "$output/catalog-prepare-high-precision"
install -m755 "${CARGO_TARGET_DIR:-target}/$profile/market-history" "$output/market-history"
