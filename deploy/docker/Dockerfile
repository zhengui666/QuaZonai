# syntax=docker/dockerfile:1
FROM node:22.19.0-bookworm-slim@sha256:4a4884e8a44826194dff92ba316264f392056cbe243dcc9fd3551e71cea02b90 AS node

FROM node AS web
WORKDIR /build
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN --mount=type=cache,target=/root/.npm --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/proxy_ca; fi; \
    npm ci --prefix apps/web --ignore-scripts --no-audit --no-fund
COPY apps/web ./apps/web
COPY packages/contracts ./packages/contracts
RUN npm --prefix apps/web run build

FROM node AS dependencies
WORKDIR /opt/quazonai
COPY --chmod=644 apps/service/package.json apps/service/package-lock.json ./apps/service/
COPY --chmod=644 apps/research/package.json apps/research/package-lock.json ./apps/research/
COPY --chmod=644 apps/cli/package.json apps/cli/package-lock.json ./apps/cli/
# argon2's official package ships prebuilt platform binaries. No source bundler/SEA.
RUN --mount=type=cache,target=/root/.npm --mount=type=secret,id=proxy_ca \
    if [ -f /run/secrets/proxy_ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/proxy_ca; fi; \
    npm ci --prefix apps/service --omit=dev --no-audit --no-fund \
 && npm ci --prefix apps/research --omit=dev --ignore-scripts --no-audit --no-fund \
 && npm ci --prefix apps/cli --omit=dev --ignore-scripts --no-audit --no-fund

# Stage only public source files before combining them with dependencies. This
# keeps source changes from multiplying full dependency snapshots on vfs hosts.
# Explicit modes also support checkouts created under a private umask.
FROM scratch AS application-source
COPY --chmod=755 apps/service/src /opt/quazonai/apps/service/src
COPY --chmod=755 apps/research/src /opt/quazonai/apps/research/src
COPY --chmod=755 apps/research/plugins /opt/quazonai/apps/research/plugins
COPY --chmod=755 apps/cli/src /opt/quazonai/apps/cli/src
COPY --chmod=755 packages/contracts /opt/quazonai/packages/contracts
COPY --chmod=755 contracts/generated /opt/quazonai/contracts/generated
COPY --chmod=755 LICENSE NOTICE THIRD_PARTY_NOTICES.md /usr/share/doc/quazonai/
COPY --chmod=755 licenses/ /usr/share/doc/quazonai/licenses/
COPY --chmod=755 deploy/docker/entrypoint.sh /opt/quazonai/entrypoint.sh
COPY --chmod=755 deploy/docker/configure.mjs deploy/docker/maintenance.mjs /opt/quazonai/deploy/docker/

FROM application-source AS nautilus-source
COPY --chmod=755 apps/nautilus/package.json /opt/quazonai/apps/nautilus/package.json
COPY --chmod=755 apps/nautilus/src /opt/quazonai/apps/nautilus/src
COPY --chmod=755 deploy/docker/operator/catalog-prepare deploy/docker/operator/polymarket-history /opt/quazonai/operator/bin/
COPY --chmod=755 runtimes/data/source_plugins.py runtimes/data/acquire.py runtimes/data/providers.py runtimes/data/snapshot.py runtimes/data/hf_dataset.py runtimes/data/polymarket_v2.py runtimes/data/binance_vision.py /opt/quazonai/operator/

FROM node AS execution-base
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl bash tini libssl3 libstdc++6 libxml2 python3 zstd \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /opt/quazonai

# Deploy this image on the separately configured Nautilus host. It is never
# started by the application Compose project and is not copied into that image.
FROM execution-base AS nautilus
COPY --from=dependencies /opt/quazonai/apps/ ./apps/
# Includes the matching source-revision exporter used by operator data tools.
COPY --from=nautilus-source / /
# One payload layer; bind inputs are read-only and never enter the app target.
RUN --mount=type=bind,from=native,source=/,target=/native \
    --mount=type=bind,from=compiler,source=/,target=/compiler \
    mkdir -p /opt/quazonai/bin /opt/quazonai/operator/bin \
 && install -m755 /native/job /opt/quazonai/bin/job \
 && install -m755 /native/source-tools /opt/quazonai/operator/bin/source-tools \
 && cp -a /compiler/opt/rust /opt/rust \
 && cp -a /compiler/usr/share/doc/rust /usr/share/doc/rust
ARG VERSION=development
ARG REVISION=unknown
LABEL org.opencontainers.image.title="QuaZonai remote Nautilus" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
ENV QZ_JOB_BINARY=/opt/quazonai/bin/job QZ_NAUTILUS_HOST=0.0.0.0 QZ_NAUTILUS_PORT=8090 QZ_NAUTILUS_DATA_DIR=/data/nautilus QZ_SCRATCH_DIR=/tmp TMPDIR=/tmp
USER node
EXPOSE 8090
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=6 \
 CMD curl -fsS --max-time 4 http://127.0.0.1:8090/health >/dev/null
ENTRYPOINT ["/usr/bin/tini", "--", "node", "--experimental-strip-types", "/opt/quazonai/apps/nautilus/src/main.ts"]

FROM execution-base AS application
COPY --from=dependencies /opt/quazonai/apps/ ./apps/
COPY --from=application-source / /
COPY --from=web /build/apps/web/dist/ ./web/
ARG VERSION=development
ARG REVISION=unknown
LABEL org.opencontainers.image.title="QuaZonai" \
      org.opencontainers.image.source="https://github.com/zhengui666/QuaZonai" \
      org.opencontainers.image.version=$VERSION \
      org.opencontainers.image.revision=$REVISION
ENV QZ_SCRATCH_DIR=/tmp TMPDIR=/tmp
USER node
EXPOSE 8080
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=6 \
 CMD curl -fsS --max-time 4 http://127.0.0.1:8080/health/live >/dev/null
ENTRYPOINT ["/usr/bin/tini", "--", "/opt/quazonai/entrypoint.sh"]
CMD ["serve"]
