#!/bin/sh
# Executed only in the pinned Rust producer, from /build. No revision arguments:
# native identity follows input bytes; packaging revision belongs to the image.
set -eu
case "$1" in
  server)
    # Omit static symbols before the linker computes the build ID. Keep this
    # combined build and scope the profile change to the two shipped packages.
    cargo build --locked --release -p server -p runtime \
      --config 'profile.release.package.server.strip="symbols"' \
      --config 'profile.release.package.runtime.strip="symbols"'
    install -Dm755 target/release/server /out/server
    install -Dm755 target/release/runtime /out/runtime
    strip /out/server /out/runtime
    ;;
  operator)
    started=$(date +%s)
    disk_before=$(df -B1 --output=avail /build | tail -n 1 | tr -d ' ')
    CARGO_PROFILE_RELEASE_DEBUG=0 cargo build --locked --release -p job \
      --features polymarket-history,catalog-prepare --bin catalog-prepare --bin polymarket-history
    install -Dm755 target/release/catalog-prepare /operator/bin/catalog-prepare
    install -Dm755 target/release/polymarket-history /operator/bin/polymarket-history
    strip /operator/bin/catalog-prepare /operator/bin/polymarket-history
    elapsed=$(($(date +%s) - started))
    disk_after=$(df -B1 --output=avail /build | tail -n 1 | tr -d ' ')
    printf '{"schema_version":2,"input_sha256":"%s","recipe_sha256":"%s","platform":"linux/amd64","elf_sha256":{"server":"%s","runtime":"%s","catalog-prepare":"%s","polymarket-history":"%s"},"original_native_build_elapsed_seconds":%s,"original_disk_before_bytes":%s,"original_disk_after_bytes":%s,"cache":"original producer observations with shared BuildKit Cargo caches; retained unchanged on layer reuse; not a current or independent cold build"}\n' \
      "$(cat .native-input.sha256)" "$(cat .native-recipe.sha256)" \
      "$(sha256sum /out/server | cut -d ' ' -f 1)" "$(sha256sum /out/runtime | cut -d ' ' -f 1)" \
      "$(sha256sum /operator/bin/catalog-prepare | cut -d ' ' -f 1)" \
      "$(sha256sum /operator/bin/polymarket-history | cut -d ' ' -f 1)" \
      "$elapsed" "$disk_before" "$disk_after" > /operator/build-metrics.json
    ;;
  *) echo 'Unsupported native producer target' >&2; exit 1 ;;
esac
