# syntax=docker/dockerfile:1
FROM node:22-bookworm-slim@sha256:43ac6c60b8f89723f746e8a92ce91abd5017e627ce1ddfe4238355d3a30b772c AS web
WORKDIR /build
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
COPY apps/web ./apps/web
COPY contracts/generated ./contracts/generated
COPY tests/contracts ./tests/contracts
RUN npm --prefix apps/web run build

FROM rust:1.98.1-bookworm@sha256:93ce27a88655056a51dbdd8f5f2d7ddc071c7b0070fb288a37b5a285fc83971e AS server
ENV RUSTUP_TOOLCHAIN=1.98.1 CARGO_BUILD_JOBS=2
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release -p server -p runtime \
 && install -Dm755 target/release/server /out/server \
 && install -Dm755 target/release/runtime /out/runtime \
 && strip /out/server /out/runtime

FROM caddy:2.10.2@sha256:c3d7ee5d2b11f9dc54f947f68a734c84e9c9666c92c88a7f30b9cba5da182adb AS caddy
FROM debian:bookworm-slim@sha256:3783cc01769c7b2b1b83a5c5ad96c815348e28ed7da68e2e3687004faa906251
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl git ripgrep tini libssl3 libstdc++6 \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 1000 --create-home quazonai
COPY --from=server /out/server /out/runtime /opt/quazonai/bin/
COPY --from=web /build/apps/web/dist/ /opt/quazonai/web/
COPY --from=caddy /usr/bin/caddy /tmp/caddy
# The upstream Caddy image grants its binary a privileged-port file capability.
# Compose deliberately uses no-new-privileges, so install a plain executable copy;
# QuaZonai only listens on unprivileged port 8081 and does not need that capability.
RUN install -m 0755 /tmp/caddy /usr/bin/caddy && rm /tmp/caddy
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
