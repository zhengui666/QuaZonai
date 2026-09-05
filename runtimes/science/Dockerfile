# Locked W0 native runtime; all numerical work uses the upstream libraries.
# The published image carries 1.98.0; build with the 1.98.1 compiler fix.
FROM rust:1.98.0-slim-bookworm@sha256:1469a27c125cb5a3aebfa4f4e4665d935b02fb72cc093b2c974b3d740e43f157 AS rust-toolchain
FROM python:3.12.14-slim-bookworm@sha256:782412e85d0f0984994c290652577d4018aff08145c85b262bb63dc0c7522254 AS build
COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup
ENV PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY apps/qz-job apps/qz-job
COPY tests/native tests/native
# Cargo.lock comes from the inspected, successful W0 compilation artifact.
# CI rejects formatting drift; it must not rewrite source during a build.
RUN rustup set auto-self-update disable \
    && rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy \
    && rustc --version \
    && cargo fetch --locked \
    && cargo fmt --all --check \
    && cargo clippy --locked --workspace --all-targets -- -D warnings \
    && cargo test --locked --workspace --all-targets \
    && cargo build --locked --release --workspace --all-targets

FROM python:3.12.14-slim-bookworm@sha256:782412e85d0f0984994c290652577d4018aff08145c85b262bb63dc0c7522254 AS runtime
ENV PYTHONNOUSERSITE=1 PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
ENV OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 NUMEXPR_NUM_THREADS=1
COPY runtimes/science/requirements.lock /opt/qz/requirements.lock
RUN python -m pip install --no-cache-dir uv==0.10.7 \
    && uv pip sync --system --only-binary :all: /opt/qz/requirements.lock
COPY --from=build /build/target/release/qz-job /usr/local/bin/qz-job
COPY --from=build /build/Cargo.lock /opt/qz/Cargo.lock
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/qz-job"]

# Test-only native Codex image. No API keys, user profiles or model turns.
FROM python:3.12.14-slim-bookworm@sha256:782412e85d0f0984994c290652577d4018aff08145c85b262bb63dc0c7522254 AS codex-probe
COPY runtimes/codex/binary.lock /opt/qz/codex-binary.lock
RUN python -m pip install --no-cache-dir --no-deps --only-binary :all: --require-hashes -r /opt/qz/codex-binary.lock \
    && binary="$(find /usr/local/lib/python3.12/site-packages -type f -name codex -executable)" \
    && test -n "$binary" && test "$(printf '%s\n' "$binary" | wc -l)" -eq 1 \
    && ln -s "$binary" /usr/local/bin/codex-native
COPY --from=build /build/target/release/examples/codex-probe /usr/local/bin/codex-probe
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/codex-probe", "/usr/local/bin/codex-native"]
