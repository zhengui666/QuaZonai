# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim AS web
WORKDIR /build
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
COPY apps/web ./apps/web
COPY contracts/generated ./contracts/generated
COPY tests/contracts ./tests/contracts
RUN npm --prefix apps/web run build

FROM node:22-bookworm-slim AS codex
WORKDIR /build
COPY runtimes/codex/package.json runtimes/codex/package-lock.json ./
RUN npm ci --ignore-scripts --no-audit --no-fund \
 && mkdir /bin-release \
 && cp node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/bin/* /bin-release/ \
 && cp node_modules/@openai/codex-linux-x64/vendor/x86_64-unknown-linux-musl/codex-path/* /bin-release/ \
 && /bin-release/codex --version

FROM rust:1.98.1-bookworm AS server
ENV RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release -p server --bin server \
 && install -Dm755 target/release/server /out/server \
 && strip /out/server

FROM caddy:2.10.2 AS caddy
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git ripgrep tini libssl3 libstdc++6 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 1000 --create-home quazonai
COPY --from=server /out/server /opt/quazonai/bin/server
COPY --from=codex /bin-release/ /opt/quazonai/bin/
COPY --from=codex /build/node_modules/@openai/codex/ /usr/share/doc/quazonai/codex/
COPY --from=web /build/apps/web/dist/ /opt/quazonai/web/
COPY --from=caddy /usr/bin/caddy /usr/bin/caddy
COPY LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/doc/quazonai/
COPY deploy/docker/Caddyfile /opt/quazonai/Caddyfile
COPY --chmod=755 deploy/docker/entrypoint.sh /opt/quazonai/entrypoint.sh
ARG VERSION=development
ARG REVISION=unknown
LABEL org.opencontainers.image.title="QuaZonai" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
ENV PATH=/opt/quazonai/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    BIND=127.0.0.1:8080 DEVELOPMENT_HTTP=true PUBLIC_URL=http://localhost:8081
USER 1000:1000
EXPOSE 8081
HEALTHCHECK --interval=10s --timeout=12s --start-period=30s --retries=6 \
 CMD curl -fsS --max-time 5 -H "Host: ${PUBLIC_URL#http://}" http://127.0.0.1:8081/health/live >/dev/null \
  && curl -fsS --max-time 5 -H 'Accept: text/html' http://127.0.0.1:8081/ >/dev/null
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/quazonai/entrypoint.sh"]
CMD ["console"]
