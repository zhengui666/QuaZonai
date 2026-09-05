# W0 dependency-resolution image. Replace resolution with committed locks before release.
# The published image currently carries 1.98.0; build with the 1.98.1 compiler fix.
FROM rust:1.98.0-slim-bookworm AS rust-toolchain
FROM python:3.12.14-slim-bookworm AS build
COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
ENV CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup
ENV PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml rust-toolchain.toml ./
COPY apps/qz-job apps/qz-job
# This first W0 pass records the resolved dependency graph for inspection.
# Its success is not the locked-build or rewrite-complete acceptance check.
RUN rustup toolchain install 1.98.1 --profile minimal --component rustfmt --component clippy \
    && rustc --version \
    && cargo generate-lockfile \
    && cargo fmt --all \
    && cargo clippy --workspace --all-targets -- -D warnings \
    && cargo test --workspace \
    && cargo build --release --workspace

FROM python:3.12.14-slim-bookworm AS runtime
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
