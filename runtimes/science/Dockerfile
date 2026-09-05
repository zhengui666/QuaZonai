# W0 science dependency resolution. Replace it with the verified transitive lock.
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
# Cargo.lock comes from the inspected, successful W0 compilation artifact.
# Format normalization remains temporary until its exact output is committed.
RUN rustup set auto-self-update disable \
    && rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy \
    && rustc --version \
    && cargo fetch --locked \
    && cargo fmt --all \
    && cargo clippy --locked --workspace --all-targets -- -D warnings \
    && cargo test --locked --workspace \
    && cargo build --locked --release --workspace

FROM python:3.12.14-slim-bookworm@sha256:782412e85d0f0984994c290652577d4018aff08145c85b262bb63dc0c7522254 AS runtime
ENV PYTHONNOUSERSITE=1 PYTHONDONTWRITEBYTECODE=1 PYTHONUNBUFFERED=1
ENV OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 MKL_NUM_THREADS=1 NUMEXPR_NUM_THREADS=1
COPY runtimes/science/requirements.in /opt/qz/requirements.in
RUN python -m pip install --no-cache-dir uv==0.10.7 \
    && uv pip compile /opt/qz/requirements.in --python-version 3.12 \
       --only-binary :all: --no-header --no-annotate --output-file /opt/qz/requirements.lock \
    && uv pip sync --system --only-binary :all: /opt/qz/requirements.lock
COPY --from=build /build/target/release/qz-job /usr/local/bin/qz-job
COPY --from=build /build/Cargo.lock /opt/qz/Cargo.lock
USER 65532:65532
WORKDIR /tmp
ENTRYPOINT ["/usr/local/bin/qz-job"]
